//! Pattern subscriptions for push-based notifications (P5-33)
//!
//! Subscribe to tuple patterns and receive notifications via tokio channels
//! when matching tuples are written to the space.

use crate::Tuple;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Unique subscription identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sub-{}", self.0)
    }
}

/// Notification sent to subscribers when a matching tuple is written
#[derive(Debug, Clone)]
pub struct TupleEvent {
    pub subscription_id: SubscriptionId,
    pub tuple: Tuple,
}

/// A registered subscription
struct Subscription {
    id: SubscriptionId,
    pattern: Tuple,
    sender: mpsc::Sender<TupleEvent>,
}

/// Manages pattern subscriptions
pub struct SubscriptionManager {
    subscriptions: Arc<Mutex<Vec<Subscription>>>,
    next_id: Arc<Mutex<u64>>,
}

impl SubscriptionManager {
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    /// Subscribe to a pattern, receiving notifications on the returned channel
    ///
    /// The buffer_size controls backpressure: if the receiver falls behind,
    /// notifications may be dropped after the buffer fills.
    pub fn subscribe(
        &self,
        pattern: Tuple,
        buffer_size: usize,
    ) -> (SubscriptionId, mpsc::Receiver<TupleEvent>) {
        let (sender, receiver) = mpsc::channel(buffer_size);

        let id = {
            let mut next = self.next_id.lock();
            let id = SubscriptionId(*next);
            *next += 1;
            id
        };

        let sub = Subscription {
            id,
            pattern: pattern.clone(),
            sender,
        };

        self.subscriptions.lock().push(sub);
        debug!(id = %id, "New pattern subscription registered");

        (id, receiver)
    }

    /// Remove a subscription
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut subs = self.subscriptions.lock();
        let before = subs.len();
        subs.retain(|s| s.id != id);
        let removed = subs.len() < before;

        if removed {
            debug!(id = %id, "Subscription removed");
        }

        removed
    }

    /// Notify all matching subscriptions about a new tuple
    ///
    /// Called when a tuple is written to the space.
    /// Drops subscriptions whose receivers have been closed.
    pub fn notify(&self, tuple: &Tuple) {
        let mut subs = self.subscriptions.lock();
        let mut to_remove = Vec::new();

        for sub in subs.iter() {
            if tuple.matches(&sub.pattern) {
                let event = TupleEvent {
                    subscription_id: sub.id,
                    tuple: tuple.clone(),
                };

                match sub.sender.try_send(event) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        to_remove.push(sub.id);
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        warn!(
                            id = %sub.id,
                            "Subscription buffer full, dropping notification"
                        );
                    }
                }
            }
        }

        for id in to_remove {
            subs.retain(|s| s.id != id);
            debug!(id = %id, "Removed closed subscription");
        }
    }

    /// List active subscription IDs and their patterns
    pub fn list(&self) -> Vec<(SubscriptionId, Tuple)> {
        self.subscriptions
            .lock()
            .iter()
            .map(|s| (s.id, s.pattern.clone()))
            .collect()
    }

    /// Get the number of active subscriptions
    pub fn count(&self) -> usize {
        self.subscriptions.lock().len()
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SubscriptionManager {
    fn clone(&self) -> Self {
        Self {
            subscriptions: Arc::clone(&self.subscriptions),
            next_id: Arc::clone(&self.next_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;

    #[tokio::test]
    async fn test_subscribe_and_notify() {
        let mgr = SubscriptionManager::new();

        let pattern = Tuple::pattern(&[Value::Str("event".to_string()), Value::Wildcard]);
        let (id, mut rx) = mgr.subscribe(pattern, 10);

        let tuple = Tuple::from(vec![
            Value::Str("event".to_string()),
            Value::Str("file_changed".to_string()),
        ]);
        mgr.notify(&tuple);

        let event = rx.recv().await.unwrap();
        assert_eq!(event.subscription_id, id);
        assert_eq!(event.tuple.fields[1], Value::Str("file_changed".to_string()));
    }

    #[tokio::test]
    async fn test_no_notify_for_non_matching() {
        let mgr = SubscriptionManager::new();

        let pattern = Tuple::pattern(&[Value::Str("event".to_string()), Value::Wildcard]);
        let (_id, mut rx) = mgr.subscribe(pattern, 10);

        let tuple = Tuple::from(vec![
            Value::Str("task".to_string()),
            Value::Str("build".to_string()),
        ]);
        mgr.notify(&tuple);

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let mgr = SubscriptionManager::new();

        let pattern1 = Tuple::pattern(&[Value::Str("event".to_string()), Value::Wildcard]);
        let (_id1, mut rx1) = mgr.subscribe(pattern1, 10);

        let pattern2 = Tuple::pattern(&[Value::Wildcard, Value::Wildcard]);
        let (_id2, mut rx2) = mgr.subscribe(pattern2, 10);

        let tuple = Tuple::from(vec![
            Value::Str("event".to_string()),
            Value::Str("test".to_string()),
        ]);
        mgr.notify(&tuple);

        assert!(rx1.recv().await.is_some());
        assert!(rx2.recv().await.is_some());
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let mgr = SubscriptionManager::new();

        let pattern = Tuple::pattern(&[Value::Wildcard]);
        let (id, mut rx) = mgr.subscribe(pattern, 10);

        assert_eq!(mgr.count(), 1);
        assert!(mgr.unsubscribe(id));
        assert_eq!(mgr.count(), 0);

        mgr.notify(&Tuple::from(vec![Value::Int(1)]));

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err() || result.unwrap().is_none());
    }

    #[test]
    fn test_list_subscriptions() {
        let mgr = SubscriptionManager::new();

        let p1 = Tuple::pattern(&[Value::Str("a".to_string())]);
        let p2 = Tuple::pattern(&[Value::Str("b".to_string())]);

        let (id1, _rx1) = mgr.subscribe(p1, 10);
        let (id2, _rx2) = mgr.subscribe(p2, 10);

        let list = mgr.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].0, id1);
        assert_eq!(list[1].0, id2);
    }

    #[tokio::test]
    async fn test_closed_receiver_cleanup() {
        let mgr = SubscriptionManager::new();

        let pattern = Tuple::pattern(&[Value::Wildcard]);
        let (_id, rx) = mgr.subscribe(pattern, 10);

        drop(rx);
        assert_eq!(mgr.count(), 1);

        mgr.notify(&Tuple::from(vec![Value::Int(1)]));
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_subscription_id_display() {
        let id = SubscriptionId(42);
        assert_eq!(format!("{}", id), "sub-42");
    }

    #[tokio::test]
    async fn test_wildcard_pattern_matches_all() {
        let mgr = SubscriptionManager::new();

        let pattern = Tuple::pattern(&[Value::Wildcard, Value::Wildcard]);
        let (_id, mut rx) = mgr.subscribe(pattern, 10);

        mgr.notify(&Tuple::from(vec![Value::Int(1), Value::Int(2)]));
        mgr.notify(&Tuple::from(vec![
            Value::Str("a".to_string()),
            Value::Bool(true),
        ]));

        assert!(rx.recv().await.is_some());
        assert!(rx.recv().await.is_some());
    }

    #[tokio::test]
    async fn test_buffer_full_drops_notification() {
        let mgr = SubscriptionManager::new();

        let pattern = Tuple::pattern(&[Value::Wildcard]);
        let (_id, _rx) = mgr.subscribe(pattern, 1);

        mgr.notify(&Tuple::from(vec![Value::Int(1)]));
        mgr.notify(&Tuple::from(vec![Value::Int(2)]));
        // Second notification may be dropped due to buffer size 1

        assert_eq!(mgr.count(), 1);
    }
}
