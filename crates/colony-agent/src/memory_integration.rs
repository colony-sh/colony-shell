//! Colony-memory integration for persistent context
//!
//! On each user message, searches memory for relevant past context and
//! injects it into the system prompt. After each exchange, stores important
//! information as new memories. Supports configurable memory_enabled flag.

#[cfg(feature = "memory")]
mod inner {
    use crate::provider::{Message, MessageContent, MessageRole};
    use crate::AgentError;
    use colony_memory::{MemoryStore, Scope, SearchResult};
    use std::sync::Arc;
    use tracing::{debug, info, warn};

    /// Configuration for memory integration
    #[derive(Debug, Clone)]
    pub struct MemoryConfig {
        pub enabled: bool,
        pub max_memories_per_query: usize,
        pub relevance_threshold: f32,
        pub scope: Scope,
        pub auto_store: bool,
    }

    impl Default for MemoryConfig {
        fn default() -> Self {
            Self {
                enabled: true,
                max_memories_per_query: 5,
                relevance_threshold: 0.7,
                scope: Scope::Global,
                auto_store: true,
            }
        }
    }

    impl MemoryConfig {
        pub fn disabled() -> Self {
            Self {
                enabled: false,
                ..Default::default()
            }
        }

        pub fn with_scope(mut self, scope: Scope) -> Self {
            self.scope = scope;
            self
        }

        pub fn with_max_memories(mut self, n: usize) -> Self {
            self.max_memories_per_query = n;
            self
        }

        pub fn with_auto_store(mut self, auto: bool) -> Self {
            self.auto_store = auto;
            self
        }
    }

    /// Memory integration layer for the agent
    pub struct MemoryIntegration {
        store: Arc<MemoryStore>,
        config: MemoryConfig,
    }

    impl MemoryIntegration {
        pub fn new(store: Arc<MemoryStore>, config: MemoryConfig) -> Self {
            Self { store, config }
        }

        /// Search for relevant memories given a user message
        pub async fn search_relevant(
            &self,
            user_message: &str,
        ) -> Result<Vec<SearchResult>, AgentError> {
            if !self.config.enabled {
                return Ok(Vec::new());
            }

            let results = self
                .store
                .search_in_scope(
                    user_message,
                    self.config.max_memories_per_query,
                    Some(&self.config.scope),
                )
                .await
                .map_err(|e| AgentError::Protocol(format!("memory search failed: {}", e)))?;

            let filtered: Vec<SearchResult> = results
                .into_iter()
                .filter(|r| r.distance <= self.config.relevance_threshold)
                .collect();

            debug!("Found {} relevant memories for query", filtered.len());

            Ok(filtered)
        }

        /// Build a memory context string to inject into the system prompt
        pub async fn build_context(
            &self,
            user_message: &str,
        ) -> Result<Option<String>, AgentError> {
            if !self.config.enabled {
                return Ok(None);
            }

            let memories = self.search_relevant(user_message).await?;

            if memories.is_empty() {
                return Ok(None);
            }

            let mut context_parts = Vec::with_capacity(memories.len());
            for (i, result) in memories.iter().enumerate() {
                context_parts.push(format!(
                    "{}. {} (relevance: {:.2})",
                    i + 1,
                    result.entry.content,
                    1.0 - result.distance,
                ));
            }

            let context = format!(
                "\n[Relevant memories]\n{}\n[End memories]",
                context_parts.join("\n")
            );

            Ok(Some(context))
        }

        /// Inject memory context into the system prompt
        pub async fn inject_context(
            &self,
            system_prompt: &str,
            user_message: &str,
        ) -> Result<String, AgentError> {
            let Some(context) = self.build_context(user_message).await? else {
                return Ok(system_prompt.to_string());
            };

            Ok(format!("{}\n{}", system_prompt, context))
        }

        /// Store important information from an exchange as new memory
        pub async fn store_from_exchange(
            &self,
            user_message: &str,
            assistant_response: &str,
        ) -> Result<Option<String>, AgentError> {
            if !self.config.enabled || !self.config.auto_store {
                return Ok(None);
            }

            let importance = assess_importance(user_message, assistant_response);
            if importance < 0.3 {
                debug!(
                    "Exchange not important enough to store (importance: {:.2})",
                    importance
                );
                return Ok(None);
            }

            let summary = summarize_exchange(user_message, assistant_response);

            let id = self
                .store
                .store_with_importance(
                    &summary,
                    self.config.scope.clone(),
                    importance,
                    Some(serde_json::json!({
                        "source": "agent_exchange",
                    })),
                )
                .await
                .map_err(|e| AgentError::Protocol(format!("memory store failed: {}", e)))?;

            info!(
                "Stored memory {} (importance: {:.2}): {}",
                id,
                importance,
                if summary.len() > 80 {
                    &summary[..80]
                } else {
                    &summary
                }
            );

            Ok(Some(id))
        }

        /// Check if memory is enabled
        pub(crate) fn is_enabled(&self) -> bool {
            self.config.enabled
        }

        /// Get the memory config
        pub(crate) fn config(&self) -> &MemoryConfig {
            &self.config
        }
    }

    /// Assess the importance of an exchange (0.0 = trivial, 1.0 = critical)
    pub(super) fn assess_importance(user_message: &str, assistant_response: &str) -> f32 {
        let mut score = 0.0f32;

        let user_lower = user_message.to_lowercase();
        let response_lower = assistant_response.to_lowercase();

        // Longer exchanges tend to be more important
        let combined_len = user_message.len() + assistant_response.len();
        if combined_len > 500 {
            score += 0.2;
        }

        // Preference/configuration indicators
        let preference_words = [
            "prefer",
            "always",
            "never",
            "remember",
            "important",
            "configuration",
            "setting",
            "convention",
        ];
        for word in &preference_words {
            if user_lower.contains(word) || response_lower.contains(word) {
                score += 0.15;
                break;
            }
        }

        // Technical decisions
        let decision_words = [
            "decided",
            "choosing",
            "approach",
            "architecture",
            "pattern",
            "strategy",
        ];
        for word in &decision_words {
            if user_lower.contains(word) || response_lower.contains(word) {
                score += 0.15;
                break;
            }
        }

        // Error resolution
        if user_lower.contains("error") || user_lower.contains("fix") || user_lower.contains("bug")
        {
            score += 0.1;
        }

        // Clamp to 0.0..1.0
        score.min(1.0)
    }

    /// Create a condensed summary of an exchange for storage
    pub(super) fn summarize_exchange(user_message: &str, assistant_response: &str) -> String {
        format!(
            "User asked: {}\nAssistant: {}",
            truncate_chars(user_message, 200),
            truncate_chars(assistant_response, 300)
        )
    }

    /// Truncate to `max` chars (not bytes, so multibyte text can't panic).
    fn truncate_chars(s: &str, max: usize) -> String {
        match s.char_indices().nth(max) {
            Some((end, _)) => format!("{}...", &s[..end]),
            None => s.to_string(),
        }
    }
}

#[cfg(feature = "memory")]
pub use inner::*;

/// Stub types when memory feature is disabled
#[cfg(not(feature = "memory"))]
pub mod stub {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct MemoryConfig {
        pub enabled: bool,
    }

    impl MemoryConfig {
        pub fn disabled() -> Self {
            Self { enabled: false }
        }
    }
}

#[cfg(not(feature = "memory"))]
pub use stub::*;

#[cfg(test)]
#[cfg(feature = "memory")]
mod tests {
    use super::inner::{assess_importance, summarize_exchange};
    use super::*;

    #[test]
    fn test_memory_config_default() {
        let config = MemoryConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_memories_per_query, 5);
        assert!(config.auto_store);
    }

    #[test]
    fn test_memory_config_disabled() {
        let config = MemoryConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_memory_config_builder() {
        let config = MemoryConfig::default()
            .with_scope(colony_memory::Scope::Project("test".to_string()))
            .with_max_memories(10)
            .with_auto_store(false);

        assert_eq!(config.max_memories_per_query, 10);
        assert!(!config.auto_store);
    }

    #[test]
    fn test_assess_importance_trivial() {
        let score = assess_importance("hi", "hello!");
        assert!(score < 0.3);
    }

    #[test]
    fn test_assess_importance_preference() {
        let score = assess_importance(
            "I prefer to always use dark mode",
            "Noted, I'll remember your preference for dark mode.",
        );
        assert!(score >= 0.15);
    }

    #[test]
    fn test_assess_importance_decision() {
        let score = assess_importance(
            "We decided to use the strategy pattern for this architecture",
            "Good approach. The strategy pattern will allow...",
        );
        assert!(score >= 0.15);
    }

    #[test]
    fn test_summarize_exchange_short() {
        let summary = summarize_exchange("Hello", "Hi there!");
        assert!(summary.contains("User asked: Hello"));
        assert!(summary.contains("Assistant: Hi there!"));
    }

    #[test]
    fn test_summarize_exchange_truncates() {
        let long_msg = "x".repeat(500);
        let summary = summarize_exchange(&long_msg, &long_msg);
        assert!(summary.len() < 1000);
        assert!(summary.contains("..."));
    }

    #[test]
    fn test_summarize_exchange_multibyte_does_not_panic() {
        let long_msg = "é".repeat(500);
        let summary = summarize_exchange(&long_msg, &long_msg);
        assert!(summary.contains("..."));
    }
}

#[cfg(test)]
#[cfg(not(feature = "memory"))]
mod tests_no_memory {
    use super::stub::*;

    #[test]
    fn test_stub_config() {
        let config = MemoryConfig::disabled();
        assert!(!config.enabled);
    }
}
