//! Tuple space bridge for reactive agent coordination (P5-35)
//!
//! Wires colony-tuple events into colony-agent so that:
//! - Tool results are automatically written as tuples
//! - Configurable tuple patterns wake the agent for reactive processing
//! - `AgentTupleBridge` connects a `TupleSpace` + `SubscriptionManager` to agent processing

#[cfg(feature = "tuple")]
mod inner {
    use crate::tool::ToolOutput;
    use colony_tuple::subscription::{SubscriptionId, SubscriptionManager, TupleEvent};
    use colony_tuple::{Tuple, TupleSpace, Value};
    use tokio::sync::mpsc;
    use tracing::{debug, info};

    /// A pattern specification that the agent should react to
    #[derive(Debug, Clone)]
    pub struct WakePattern {
        pub name: String,
        pub pattern: Tuple,
    }

    impl WakePattern {
        pub fn new(name: impl Into<String>, pattern: Tuple) -> Self {
            Self {
                name: name.into(),
                pattern,
            }
        }

        /// Create a pattern from a colon-separated string like "task:assigned:*"
        ///
        /// Each segment becomes a `Value::Str`, except `*` which becomes `Value::Wildcard`.
        pub fn from_pattern_str(name: impl Into<String>, pattern_str: &str) -> Self {
            let fields: Vec<Value> = pattern_str
                .split(':')
                .map(|seg| {
                    if seg == "*" {
                        Value::Wildcard
                    } else {
                        Value::Str(seg.to_string())
                    }
                })
                .collect();

            Self {
                name: name.into(),
                pattern: Tuple::pattern(&fields),
            }
        }
    }

    /// Configuration for the tuple bridge
    #[derive(Debug, Clone)]
    pub struct TupleBridgeConfig {
        pub write_tool_results: bool,
        pub wake_patterns: Vec<WakePattern>,
        pub subscription_buffer_size: usize,
    }

    impl Default for TupleBridgeConfig {
        fn default() -> Self {
            Self {
                write_tool_results: true,
                wake_patterns: Vec::new(),
                subscription_buffer_size: 64,
            }
        }
    }

    impl TupleBridgeConfig {
        pub fn with_tool_results(mut self, enabled: bool) -> Self {
            self.write_tool_results = enabled;
            self
        }

        pub fn with_wake_pattern(mut self, pattern: WakePattern) -> Self {
            self.wake_patterns.push(pattern);
            self
        }

        pub fn with_buffer_size(mut self, size: usize) -> Self {
            self.subscription_buffer_size = size;
            self
        }
    }

    /// An event received from the tuple space that should wake the agent
    #[derive(Debug, Clone)]
    pub struct AgentWakeEvent {
        pub pattern_name: String,
        pub subscription_id: SubscriptionId,
        pub tuple: Tuple,
    }

    /// Bridges a TupleSpace with agent processing
    ///
    /// Responsibilities:
    /// - Writes tool results as tuples to the shared space
    /// - Subscribes to configurable patterns and forwards matching tuples
    ///   as wake events for the agent to process
    pub struct AgentTupleBridge {
        space: TupleSpace,
        subscriptions: SubscriptionManager,
        config: TupleBridgeConfig,
        active_subscriptions: Vec<(SubscriptionId, String)>,
        wake_receiver: Option<mpsc::Receiver<AgentWakeEvent>>,
        wake_sender: mpsc::Sender<AgentWakeEvent>,
    }

    impl AgentTupleBridge {
        pub fn new(
            space: TupleSpace,
            subscriptions: SubscriptionManager,
            config: TupleBridgeConfig,
        ) -> Self {
            let (wake_sender, wake_receiver) = mpsc::channel(config.subscription_buffer_size);

            Self {
                space,
                subscriptions,
                config,
                active_subscriptions: Vec::new(),
                wake_receiver: Some(wake_receiver),
                wake_sender,
            }
        }

        /// Take the wake event receiver for the agent to poll
        ///
        /// This can only be called once; subsequent calls return `None`.
        pub fn take_wake_receiver(&mut self) -> Option<mpsc::Receiver<AgentWakeEvent>> {
            self.wake_receiver.take()
        }

        /// Subscribe to all configured wake patterns
        ///
        /// For each `WakePattern` in the config, registers a subscription with the
        /// `SubscriptionManager` and spawns a forwarding task that converts
        /// `TupleEvent`s into `AgentWakeEvent`s.
        pub fn activate_subscriptions(&mut self) {
            for wp in &self.config.wake_patterns {
                let (sub_id, rx) = self
                    .subscriptions
                    .subscribe(wp.pattern.clone(), self.config.subscription_buffer_size);

                info!(
                    pattern_name = %wp.name,
                    subscription_id = %sub_id,
                    "Activated wake pattern subscription"
                );

                self.active_subscriptions.push((sub_id, wp.name.clone()));

                let sender = self.wake_sender.clone();
                let pattern_name = wp.name.clone();
                tokio::spawn(forward_subscription_events(
                    rx,
                    sender,
                    pattern_name,
                    sub_id,
                ));
            }
        }

        /// Write a tool result as a tuple to the shared space
        ///
        /// The tuple format is: `("tool_result", tool_name, success, data_json)`
        pub fn write_tool_result(&self, tool_name: &str, output: &ToolOutput) {
            if !self.config.write_tool_results {
                return;
            }

            let data_json = serde_json::to_string(&output.data).unwrap_or_default();

            let tuple = Tuple::from(vec![
                Value::Str("tool_result".to_string()),
                Value::Str(tool_name.to_string()),
                Value::Bool(output.success),
                Value::Str(data_json),
            ]);

            self.space.out(tuple.clone());
            self.subscriptions.notify(&tuple);

            debug!(
                tool = tool_name,
                success = output.success,
                "Wrote tool result tuple"
            );
        }

        /// Write an arbitrary tuple and notify subscribers
        pub fn write_tuple(&self, tuple: Tuple) {
            self.space.out(tuple.clone());
            self.subscriptions.notify(&tuple);
        }

        /// Deactivate all subscriptions
        pub fn deactivate_subscriptions(&mut self) {
            for (sub_id, name) in self.active_subscriptions.drain(..) {
                self.subscriptions.unsubscribe(sub_id);
                debug!(
                    pattern_name = %name,
                    subscription_id = %sub_id,
                    "Deactivated wake pattern subscription"
                );
            }
        }

        /// Get the number of active subscriptions
        pub fn active_subscription_count(&self) -> usize {
            self.active_subscriptions.len()
        }
    }

    impl Drop for AgentTupleBridge {
        fn drop(&mut self) {
            self.deactivate_subscriptions();
        }
    }

    async fn forward_subscription_events(
        mut rx: mpsc::Receiver<TupleEvent>,
        sender: mpsc::Sender<AgentWakeEvent>,
        pattern_name: String,
        sub_id: SubscriptionId,
    ) {
        while let Some(event) = rx.recv().await {
            let wake_event = AgentWakeEvent {
                pattern_name: pattern_name.clone(),
                subscription_id: event.subscription_id,
                tuple: event.tuple,
            };

            if sender.send(wake_event).await.is_err() {
                debug!(
                    pattern_name = %pattern_name,
                    subscription_id = %sub_id,
                    "Wake event receiver dropped, stopping forwarder"
                );
                break;
            }
        }
    }
}

#[cfg(feature = "tuple")]
pub use inner::*;

#[cfg(not(feature = "tuple"))]
pub mod stub {
    #[derive(Debug, Clone, Default)]
    pub struct TupleBridgeConfig {
        pub write_tool_results: bool,
    }
}

#[cfg(not(feature = "tuple"))]
pub use stub::*;

#[cfg(test)]
#[cfg(feature = "tuple")]
mod tests {
    use super::*;
    use colony_tuple::subscription::SubscriptionManager;
    use colony_tuple::{Tuple, TupleSpace, Value};

    #[test]
    fn test_wake_pattern_from_str() {
        let wp = WakePattern::from_pattern_str("task_assigned", "task:assigned:*");
        assert_eq!(wp.name, "task_assigned");
        assert_eq!(wp.pattern.fields.len(), 3);
        assert_eq!(wp.pattern.fields[0], Value::Str("task".to_string()));
        assert_eq!(wp.pattern.fields[1], Value::Str("assigned".to_string()));
        assert_eq!(wp.pattern.fields[2], Value::Wildcard);
    }

    #[test]
    fn test_wake_pattern_new() {
        let pattern = Tuple::pattern(&[Value::Str("event".to_string()), Value::Wildcard]);
        let wp = WakePattern::new("events", pattern.clone());
        assert_eq!(wp.name, "events");
        assert_eq!(wp.pattern, pattern);
    }

    #[test]
    fn test_bridge_config_defaults() {
        let config = TupleBridgeConfig::default();
        assert!(config.write_tool_results);
        assert!(config.wake_patterns.is_empty());
        assert_eq!(config.subscription_buffer_size, 64);
    }

    #[test]
    fn test_bridge_config_builder() {
        let config = TupleBridgeConfig::default()
            .with_tool_results(false)
            .with_buffer_size(128)
            .with_wake_pattern(WakePattern::from_pattern_str("test", "a:*"));

        assert!(!config.write_tool_results);
        assert_eq!(config.subscription_buffer_size, 128);
        assert_eq!(config.wake_patterns.len(), 1);
    }

    #[test]
    fn test_bridge_write_tool_result() {
        let space = TupleSpace::new();
        let subs = SubscriptionManager::new();
        let config = TupleBridgeConfig::default();
        let bridge = AgentTupleBridge::new(space.clone(), subs, config);

        let output = crate::tool::ToolOutput::success(serde_json::json!({"result": "ok"}));
        bridge.write_tool_result("shell_exec", &output);

        let pattern = Tuple::pattern(&[
            Value::Str("tool_result".to_string()),
            Value::Str("shell_exec".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]);
        let found = space.rdp(pattern);
        assert!(found.is_some());

        let tuple = found.expect("tuple should exist");
        assert_eq!(tuple.fields[0], Value::Str("tool_result".to_string()));
        assert_eq!(tuple.fields[1], Value::Str("shell_exec".to_string()));
        assert_eq!(tuple.fields[2], Value::Bool(true));
    }

    #[test]
    fn test_bridge_write_disabled() {
        let space = TupleSpace::new();
        let subs = SubscriptionManager::new();
        let config = TupleBridgeConfig::default().with_tool_results(false);
        let bridge = AgentTupleBridge::new(space.clone(), subs, config);

        let output = crate::tool::ToolOutput::success(serde_json::json!({"result": "ok"}));
        bridge.write_tool_result("shell_exec", &output);

        assert!(space.is_empty());
    }

    #[test]
    fn test_bridge_write_arbitrary_tuple() {
        let space = TupleSpace::new();
        let subs = SubscriptionManager::new();
        let config = TupleBridgeConfig::default();
        let bridge = AgentTupleBridge::new(space.clone(), subs, config);

        let tuple = Tuple::from(vec![Value::Str("custom".to_string()), Value::Int(42)]);
        bridge.write_tuple(tuple);

        let pattern = Tuple::pattern(&[Value::Str("custom".to_string()), Value::Wildcard]);
        assert!(space.rdp(pattern).is_some());
    }

    #[tokio::test]
    async fn test_bridge_activate_and_receive_wake_events() {
        let space = TupleSpace::new();
        let subs = SubscriptionManager::new();

        let config = TupleBridgeConfig::default()
            .with_wake_pattern(WakePattern::from_pattern_str("task_done", "task:done:*"));

        let mut bridge = AgentTupleBridge::new(space.clone(), subs.clone(), config);
        let mut rx = bridge.take_wake_receiver().expect("first take succeeds");

        bridge.activate_subscriptions();
        assert_eq!(bridge.active_subscription_count(), 1);

        let tuple = Tuple::from(vec![
            Value::Str("task".to_string()),
            Value::Str("done".to_string()),
            Value::Str("build".to_string()),
        ]);
        subs.notify(&tuple);

        let event = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
            .await
            .expect("should receive within timeout")
            .expect("channel should not be closed");

        assert_eq!(event.pattern_name, "task_done");
        assert_eq!(event.tuple.fields[2], Value::Str("build".to_string()));
    }

    #[tokio::test]
    async fn test_bridge_non_matching_tuples_not_forwarded() {
        let space = TupleSpace::new();
        let subs = SubscriptionManager::new();

        let config = TupleBridgeConfig::default()
            .with_wake_pattern(WakePattern::from_pattern_str("task_done", "task:done:*"));

        let mut bridge = AgentTupleBridge::new(space.clone(), subs.clone(), config);
        let mut rx = bridge.take_wake_receiver().expect("first take succeeds");

        bridge.activate_subscriptions();

        let tuple = Tuple::from(vec![
            Value::Str("event".to_string()),
            Value::Str("unrelated".to_string()),
            Value::Str("data".to_string()),
        ]);
        subs.notify(&tuple);

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_err(), "should timeout with no matching event");
    }

    #[test]
    fn test_bridge_take_receiver_once() {
        let space = TupleSpace::new();
        let subs = SubscriptionManager::new();
        let config = TupleBridgeConfig::default();
        let mut bridge = AgentTupleBridge::new(space, subs, config);

        assert!(bridge.take_wake_receiver().is_some());
        assert!(bridge.take_wake_receiver().is_none());
    }

    #[tokio::test]
    async fn test_bridge_deactivate_subscriptions() {
        let space = TupleSpace::new();
        let subs = SubscriptionManager::new();

        let config = TupleBridgeConfig::default()
            .with_wake_pattern(WakePattern::from_pattern_str("a", "a:*"))
            .with_wake_pattern(WakePattern::from_pattern_str("b", "b:*"));

        let mut bridge = AgentTupleBridge::new(space, subs.clone(), config);
        let _rx = bridge.take_wake_receiver();

        bridge.activate_subscriptions();
        assert_eq!(bridge.active_subscription_count(), 2);
        assert_eq!(subs.count(), 2);

        bridge.deactivate_subscriptions();
        assert_eq!(bridge.active_subscription_count(), 0);
        assert_eq!(subs.count(), 0);
    }

    #[tokio::test]
    async fn test_bridge_tool_result_triggers_subscription() {
        let space = TupleSpace::new();
        let subs = SubscriptionManager::new();

        let config = TupleBridgeConfig::default().with_wake_pattern(WakePattern::new(
            "tool_results",
            Tuple::pattern(&[
                Value::Str("tool_result".to_string()),
                Value::Wildcard,
                Value::Wildcard,
                Value::Wildcard,
            ]),
        ));

        let mut bridge = AgentTupleBridge::new(space.clone(), subs.clone(), config);
        let mut rx = bridge.take_wake_receiver().expect("first take");

        bridge.activate_subscriptions();

        let output = crate::tool::ToolOutput::success(serde_json::json!({"file": "test.rs"}));
        bridge.write_tool_result("file_read", &output);

        let event = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
            .await
            .expect("should receive within timeout")
            .expect("channel open");

        assert_eq!(event.pattern_name, "tool_results");
        assert_eq!(event.tuple.fields[1], Value::Str("file_read".to_string()));
    }
}
