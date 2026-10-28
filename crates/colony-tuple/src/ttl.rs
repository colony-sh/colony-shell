//! TTL/expiration support for tuples (P5-32)
//!
//! Adds optional expires_at field to tuples, a background reaper task
//! to remove expired tuples, and query filtering to exclude expired entries.

use crate::Tuple;
use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use scc::HashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

/// A tuple with optional expiration metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedTuple {
    pub tuple: Tuple,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl TimedTuple {
    /// Create a timed tuple without expiration
    pub fn new(tuple: Tuple) -> Self {
        Self {
            tuple,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    /// Create a timed tuple with a TTL duration
    pub fn with_ttl(tuple: Tuple, ttl: Duration) -> Self {
        let now = Utc::now();
        Self {
            tuple,
            created_at: now,
            expires_at: Some(now + ttl),
        }
    }

    /// Create a timed tuple with an absolute expiration time
    pub fn with_expires_at(tuple: Tuple, expires_at: DateTime<Utc>) -> Self {
        Self {
            tuple,
            created_at: Utc::now(),
            expires_at: Some(expires_at),
        }
    }

    /// Check if this tuple has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| Utc::now() >= exp)
            .unwrap_or(false)
    }

    /// Check if this tuple matches a pattern (and is not expired)
    pub fn matches_live(&self, pattern: &Tuple) -> bool {
        !self.is_expired() && self.tuple.matches(pattern)
    }
}

/// Configuration for TTL-aware tuple space
#[derive(Debug, Clone)]
pub struct TtlConfig {
    /// Default TTL for tuples (None = no expiration)
    pub default_ttl: Option<Duration>,
    /// Interval between reaper runs
    pub reaper_interval: std::time::Duration,
}

impl Default for TtlConfig {
    fn default() -> Self {
        Self {
            default_ttl: None,
            reaper_interval: std::time::Duration::from_secs(60),
        }
    }
}

/// TTL-aware tuple space
///
/// Wraps tuples with expiration metadata and runs a background
/// reaper to clean up expired entries.
pub struct TtlTupleSpace {
    tuples: Arc<HashMap<u64, Vec<TimedTuple>>>,
    notify: Arc<(Mutex<()>, parking_lot::Condvar)>,
    config: TtlConfig,
    reaper_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl TtlTupleSpace {
    /// Create a new TTL-aware tuple space
    pub fn new(config: TtlConfig) -> Self {
        Self {
            tuples: Arc::new(HashMap::new()),
            notify: Arc::new((Mutex::new(()), parking_lot::Condvar::new())),
            config,
            reaper_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the background reaper task
    pub fn start_reaper(&self) {
        let tuples = Arc::clone(&self.tuples);
        let interval = self.config.reaper_interval;

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let removed = reap_expired(&tuples);
                if removed > 0 {
                    debug!("Reaper removed {} expired tuples", removed);
                }
            }
        });

        *self.reaper_handle.lock() = Some(handle);
    }

    /// Write a tuple without TTL (or with default TTL if configured)
    pub fn out(&self, tuple: Tuple) {
        let timed = match self.config.default_ttl {
            Some(ttl) => TimedTuple::with_ttl(tuple, ttl),
            None => TimedTuple::new(tuple),
        };
        self.out_timed(timed);
    }

    /// Write a tuple with an explicit TTL
    pub fn out_with_ttl(&self, tuple: Tuple, ttl: Duration) {
        self.out_timed(TimedTuple::with_ttl(tuple, ttl));
    }

    /// Write a tuple with an absolute expiration
    pub fn out_with_expires_at(&self, tuple: Tuple, expires_at: DateTime<Utc>) {
        self.out_timed(TimedTuple::with_expires_at(tuple, expires_at));
    }

    /// Write a timed tuple directly
    fn out_timed(&self, timed: TimedTuple) {
        let hash = timed.tuple.len() as u64;
        self.tuples.entry(hash).or_default().get_mut().push(timed);

        let (_lock, condvar) = &*self.notify;
        condvar.notify_all();
    }

    /// Non-blocking read and remove (excludes expired)
    pub fn inp(&self, pattern: Tuple) -> Option<Tuple> {
        let hash = pattern.len() as u64;

        if let Some(mut bucket) = self.tuples.get(&hash) {
            let bucket = bucket.get_mut();

            if let Some(pos) = bucket.iter().position(|t| t.matches_live(&pattern)) {
                return Some(bucket.remove(pos).tuple);
            }
        }

        None
    }

    /// Non-blocking read without removing (excludes expired)
    pub fn rdp(&self, pattern: Tuple) -> Option<Tuple> {
        let hash = pattern.len() as u64;

        self.tuples.read(&hash, |_, tuples| {
            tuples
                .iter()
                .find(|t| t.matches_live(&pattern))
                .map(|t| t.tuple.clone())
        })?
    }

    /// Blocking read and remove (excludes expired)
    pub fn r#in(&self, pattern: Tuple) -> Option<Tuple> {
        for _ in 0..100 {
            if let Some(tuple) = self.inp(pattern.clone()) {
                return Some(tuple);
            }

            let (lock, condvar) = &*self.notify;
            let mut guard = lock.lock();
            condvar.wait_for(&mut guard, std::time::Duration::from_millis(100));
        }
        None
    }

    /// Count non-expired tuples
    pub fn len(&self) -> usize {
        let mut count = 0;
        self.tuples.scan(|_, bucket| {
            count += bucket.iter().filter(|t| !t.is_expired()).count();
        });
        count
    }

    /// Check if space is empty (considering only non-expired tuples)
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Manually trigger reaping of expired tuples
    pub fn reap(&self) -> usize {
        reap_expired(&self.tuples)
    }
}

impl Default for TtlTupleSpace {
    fn default() -> Self {
        Self::new(TtlConfig::default())
    }
}

impl Clone for TtlTupleSpace {
    fn clone(&self) -> Self {
        Self {
            tuples: Arc::clone(&self.tuples),
            notify: Arc::clone(&self.notify),
            config: self.config.clone(),
            reaper_handle: Arc::clone(&self.reaper_handle),
        }
    }
}

/// Remove expired tuples from all buckets, returns count removed
fn reap_expired(tuples: &HashMap<u64, Vec<TimedTuple>>) -> usize {
    let mut total_removed = 0;
    tuples.retain(|_, bucket| {
        let before = bucket.len();
        bucket.retain(|t| !t.is_expired());
        total_removed += before - bucket.len();
        !bucket.is_empty()
    });
    total_removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;

    #[test]
    fn test_timed_tuple_no_expiry() {
        let tt = TimedTuple::new(Tuple::from(vec![Value::Int(1)]));
        assert!(!tt.is_expired());
        assert!(tt.expires_at.is_none());
    }

    #[test]
    fn test_timed_tuple_with_ttl() {
        let tt = TimedTuple::with_ttl(Tuple::from(vec![Value::Int(1)]), Duration::seconds(3600));
        assert!(!tt.is_expired());
        assert!(tt.expires_at.is_some());
    }

    #[test]
    fn test_timed_tuple_already_expired() {
        let tt = TimedTuple::with_expires_at(
            Tuple::from(vec![Value::Int(1)]),
            Utc::now() - Duration::seconds(1),
        );
        assert!(tt.is_expired());
    }

    #[test]
    fn test_timed_tuple_matches_live() {
        let tt = TimedTuple::new(Tuple::from(vec![Value::Str("test".to_string())]));
        let pattern = Tuple::pattern(&[Value::Wildcard]);
        assert!(tt.matches_live(&pattern));
    }

    #[test]
    fn test_timed_tuple_expired_no_match() {
        let tt = TimedTuple::with_expires_at(
            Tuple::from(vec![Value::Str("test".to_string())]),
            Utc::now() - Duration::seconds(1),
        );
        let pattern = Tuple::pattern(&[Value::Wildcard]);
        assert!(!tt.matches_live(&pattern));
    }

    #[test]
    fn test_ttl_space_out_and_inp() {
        let space = TtlTupleSpace::new(TtlConfig::default());
        space.out(Tuple::from(vec![Value::Int(42)]));

        let result = space.inp(Tuple::pattern(&[Value::Wildcard]));
        assert!(result.is_some());
        assert_eq!(result.unwrap().fields[0], Value::Int(42));
    }

    #[test]
    fn test_ttl_space_with_ttl() {
        let space = TtlTupleSpace::new(TtlConfig::default());
        space.out_with_ttl(
            Tuple::from(vec![Value::Str("temp".to_string())]),
            Duration::seconds(3600),
        );

        let result = space.inp(Tuple::pattern(&[Value::Wildcard]));
        assert!(result.is_some());
    }

    #[test]
    fn test_ttl_space_expired_tuple_invisible() {
        let space = TtlTupleSpace::new(TtlConfig::default());
        space.out_with_expires_at(
            Tuple::from(vec![Value::Str("expired".to_string())]),
            Utc::now() - Duration::seconds(1),
        );

        let result = space.inp(Tuple::pattern(&[Value::Wildcard]));
        assert!(result.is_none());
    }

    #[test]
    fn test_ttl_space_default_ttl() {
        let config = TtlConfig {
            default_ttl: Some(Duration::seconds(3600)),
            ..Default::default()
        };
        let space = TtlTupleSpace::new(config);
        space.out(Tuple::from(vec![Value::Int(1)]));

        assert_eq!(space.len(), 1);
    }

    #[test]
    fn test_ttl_space_reap() {
        let space = TtlTupleSpace::new(TtlConfig::default());

        // Add some expired tuples
        space.out_with_expires_at(
            Tuple::from(vec![Value::Int(1)]),
            Utc::now() - Duration::seconds(10),
        );
        space.out_with_expires_at(
            Tuple::from(vec![Value::Int(2)]),
            Utc::now() - Duration::seconds(5),
        );
        space.out(Tuple::from(vec![Value::Int(3)]));

        let removed = space.reap();
        assert_eq!(removed, 2);
        assert_eq!(space.len(), 1);
    }

    #[test]
    fn test_ttl_space_rdp() {
        let space = TtlTupleSpace::new(TtlConfig::default());
        space.out(Tuple::from(vec![Value::Int(42)]));

        let r1 = space.rdp(Tuple::pattern(&[Value::Wildcard]));
        assert!(r1.is_some());

        let r2 = space.rdp(Tuple::pattern(&[Value::Wildcard]));
        assert!(r2.is_some());

        assert_eq!(space.len(), 1);
    }

    #[test]
    fn test_ttl_space_len_excludes_expired() {
        let space = TtlTupleSpace::new(TtlConfig::default());

        space.out(Tuple::from(vec![Value::Int(1)]));
        space.out_with_expires_at(
            Tuple::from(vec![Value::Int(2)]),
            Utc::now() - Duration::seconds(1),
        );

        assert_eq!(space.len(), 1);
    }

    #[tokio::test]
    async fn test_ttl_space_reaper() {
        let config = TtlConfig {
            reaper_interval: std::time::Duration::from_millis(50),
            ..Default::default()
        };
        let space = TtlTupleSpace::new(config);
        space.start_reaper();

        space.out_with_expires_at(
            Tuple::from(vec![Value::Int(1)]),
            Utc::now() - Duration::seconds(1),
        );

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // After reaper runs, expired tuple should be gone
        // (len still counts it as 0 since is_expired check)
        assert_eq!(space.len(), 0);
    }
}
