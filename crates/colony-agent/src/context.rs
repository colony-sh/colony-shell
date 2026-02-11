//! Context window management
//!
//! Tracks total token count in conversation, prunes oldest messages when
//! approaching the context limit, and summarizes pruned content. Keeps
//! system prompt and recent N messages intact.

use crate::provider::{Message, MessageContent, MessageRole};
use serde::{Deserialize, Serialize};

/// Configuration for context window management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    pub max_context_tokens: usize,
    pub keep_recent_messages: usize,
    pub prune_threshold: f64,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 128_000,
            keep_recent_messages: 10,
            prune_threshold: 0.85,
        }
    }
}

impl ContextConfig {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_context_tokens: max_tokens,
            ..Default::default()
        }
    }

    pub fn with_keep_recent(mut self, n: usize) -> Self {
        self.keep_recent_messages = n;
        self
    }

    pub fn with_prune_threshold(mut self, threshold: f64) -> Self {
        self.prune_threshold = threshold;
        self
    }
}

/// Context window manager
pub struct ContextManager {
    config: ContextConfig,
    estimated_tokens: usize,
}

impl ContextManager {
    pub fn new(config: ContextConfig) -> Self {
        Self {
            config,
            estimated_tokens: 0,
        }
    }

    /// Estimate token count for a message (rough: ~4 chars per token)
    pub fn estimate_tokens(message: &Message) -> usize {
        let text_len = match &message.content {
            MessageContent::Text(t) => t.len(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .map(|b| match b {
                    crate::provider::ContentBlock::Text { text } => text.len(),
                    crate::provider::ContentBlock::ToolUse { input, .. } => {
                        input.to_string().len()
                    }
                    crate::provider::ContentBlock::ToolResult { content, .. } => content.len(),
                })
                .sum(),
        };
        // Role overhead + content tokens (rough estimate: 4 chars/token)
        4 + text_len / 4
    }

    /// Estimate total tokens for a list of messages
    pub fn estimate_total(messages: &[Message]) -> usize {
        messages.iter().map(Self::estimate_tokens).sum()
    }

    /// Update internal token estimate
    pub fn update_estimate(&mut self, messages: &[Message]) {
        self.estimated_tokens = Self::estimate_total(messages);
    }

    /// Check if context is approaching the limit
    pub fn should_prune(&self, messages: &[Message]) -> bool {
        let total = Self::estimate_total(messages);
        total as f64 > self.config.max_context_tokens as f64 * self.config.prune_threshold
    }

    /// Prune messages to fit within context window
    ///
    /// Strategy:
    /// 1. Always keep system messages (index 0 if system)
    /// 2. Always keep the most recent N messages
    /// 3. Summarize pruned messages into a single condensed context message
    pub fn prune(&mut self, messages: &[Message]) -> Vec<Message> {
        if !self.should_prune(messages) {
            self.update_estimate(messages);
            return messages.to_vec();
        }

        let mut result = Vec::new();

        // Find system messages to preserve
        let system_messages: Vec<&Message> = messages
            .iter()
            .filter(|m| matches!(m.role, MessageRole::System))
            .collect();

        // Non-system messages
        let non_system: Vec<&Message> = messages
            .iter()
            .filter(|m| !matches!(m.role, MessageRole::System))
            .collect();

        let keep_count = self.config.keep_recent_messages.min(non_system.len());
        let prune_count = non_system.len().saturating_sub(keep_count);

        // Add system messages first
        for msg in &system_messages {
            result.push((*msg).clone());
        }

        // If there are messages to prune, create a summary
        if prune_count > 0 {
            let pruned_messages = &non_system[..prune_count];
            let summary = self.summarize_messages(pruned_messages);

            result.push(Message {
                role: MessageRole::System,
                content: MessageContent::Text(summary),
            });
        }

        // Add recent messages
        let recent_start = non_system.len().saturating_sub(keep_count);
        for msg in &non_system[recent_start..] {
            result.push((*msg).clone());
        }

        self.update_estimate(&result);
        result
    }

    /// Create a condensed summary of pruned messages
    fn summarize_messages(&self, messages: &[&Message]) -> String {
        let mut summary_parts = Vec::new();

        for msg in messages {
            let role_str = match msg.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => continue,
            };

            let content = match &msg.content {
                MessageContent::Text(t) => {
                    if t.len() > 100 {
                        format!("{}...", &t[..100])
                    } else {
                        t.clone()
                    }
                }
                MessageContent::Blocks(_) => "[tool interaction]".to_string(),
            };

            summary_parts.push(format!("{}: {}", role_str, content));
        }

        format!(
            "[Context summary - {} earlier messages pruned]\n{}",
            messages.len(),
            summary_parts.join("\n")
        )
    }

    /// Get estimated current token count
    pub fn estimated_tokens(&self) -> usize {
        self.estimated_tokens
    }

    /// Get remaining token budget
    pub fn remaining_tokens(&self) -> usize {
        self.config
            .max_context_tokens
            .saturating_sub(self.estimated_tokens)
    }

    /// Get context utilization as a fraction (0.0 to 1.0+)
    pub fn utilization(&self) -> f64 {
        self.estimated_tokens as f64 / self.config.max_context_tokens as f64
    }

    /// Get the context config
    pub fn config(&self) -> &ContextConfig {
        &self.config
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new(ContextConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_message(role: MessageRole, text: &str) -> Message {
        Message {
            role,
            content: MessageContent::Text(text.to_string()),
        }
    }

    #[test]
    fn test_estimate_tokens() {
        let msg = make_message(MessageRole::User, "Hello world");
        let tokens = ContextManager::estimate_tokens(&msg);
        assert!(tokens > 0);
        assert!(tokens < 20);
    }

    #[test]
    fn test_estimate_total() {
        let messages = vec![
            make_message(MessageRole::User, "Hello"),
            make_message(MessageRole::Assistant, "Hi there!"),
        ];
        let total = ContextManager::estimate_total(&messages);
        assert!(total > 0);
    }

    #[test]
    fn test_should_prune_under_limit() {
        let manager = ContextManager::new(ContextConfig::new(100_000));
        let messages = vec![
            make_message(MessageRole::User, "Hello"),
            make_message(MessageRole::Assistant, "Hi!"),
        ];
        assert!(!manager.should_prune(&messages));
    }

    #[test]
    fn test_should_prune_over_limit() {
        let manager = ContextManager::new(ContextConfig::new(10));
        let long_text = "x".repeat(100);
        let messages = vec![
            make_message(MessageRole::User, &long_text),
            make_message(MessageRole::Assistant, &long_text),
        ];
        assert!(manager.should_prune(&messages));
    }

    #[test]
    fn test_prune_keeps_recent() {
        let config = ContextConfig {
            max_context_tokens: 20,
            keep_recent_messages: 2,
            prune_threshold: 0.5,
        };
        let mut manager = ContextManager::new(config);

        let long = "x".repeat(100);
        let messages = vec![
            make_message(MessageRole::User, &long),
            make_message(MessageRole::Assistant, &long),
            make_message(MessageRole::User, "recent1"),
            make_message(MessageRole::Assistant, "recent2"),
        ];

        let pruned = manager.prune(&messages);

        // Should have: summary message + 2 recent
        assert!(pruned.len() <= 4);

        let last_two: Vec<&str> = pruned
            .iter()
            .rev()
            .take(2)
            .rev()
            .filter_map(|m| match &m.content {
                MessageContent::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(last_two.contains(&"recent1"));
        assert!(last_two.contains(&"recent2"));
    }

    #[test]
    fn test_prune_preserves_system_messages() {
        let config = ContextConfig {
            max_context_tokens: 20,
            keep_recent_messages: 1,
            prune_threshold: 0.5,
        };
        let mut manager = ContextManager::new(config);

        let long = "x".repeat(100);
        let messages = vec![
            make_message(MessageRole::System, "system prompt"),
            make_message(MessageRole::User, &long),
            make_message(MessageRole::Assistant, &long),
            make_message(MessageRole::User, "latest"),
        ];

        let pruned = manager.prune(&messages);

        let has_system = pruned
            .iter()
            .any(|m| matches!(&m.content, MessageContent::Text(t) if t == "system prompt"));
        assert!(has_system);
    }

    #[test]
    fn test_no_prune_when_not_needed() {
        let mut manager = ContextManager::new(ContextConfig::new(1_000_000));
        let messages = vec![
            make_message(MessageRole::User, "Hello"),
            make_message(MessageRole::Assistant, "Hi!"),
        ];
        let result = manager.prune(&messages);
        assert_eq!(result.len(), messages.len());
    }

    #[test]
    fn test_utilization() {
        let mut manager = ContextManager::new(ContextConfig::new(1000));
        let messages = vec![make_message(
            MessageRole::User,
            &"x".repeat(2000),
        )];
        manager.update_estimate(&messages);
        assert!(manager.utilization() > 0.0);
    }

    #[test]
    fn test_remaining_tokens() {
        let mut manager = ContextManager::new(ContextConfig::new(1000));
        assert_eq!(manager.remaining_tokens(), 1000);

        let messages = vec![make_message(
            MessageRole::User,
            &"x".repeat(400),
        )];
        manager.update_estimate(&messages);
        assert!(manager.remaining_tokens() < 1000);
    }

    #[test]
    fn test_context_config_builder() {
        let config = ContextConfig::new(50_000)
            .with_keep_recent(5)
            .with_prune_threshold(0.7);

        assert_eq!(config.max_context_tokens, 50_000);
        assert_eq!(config.keep_recent_messages, 5);
        assert!((config.prune_threshold - 0.7).abs() < f64::EPSILON);
    }
}
