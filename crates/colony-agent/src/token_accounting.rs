//! Token accounting and usage tracking
//!
//! Tracks input/output tokens per request, accumulates per-conversation
//! and per-session totals, supports configurable budgets with warnings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Token usage for a single request
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RequestUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl RequestUsage {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Accumulated token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStats {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub request_count: u64,
    pub per_conversation: HashMap<String, ConversationUsage>,
}

impl UsageStats {
    pub fn total_tokens(&self) -> u64 {
        self.total_input_tokens + self.total_output_tokens
    }
}

/// Per-conversation usage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub request_count: u64,
}

impl ConversationUsage {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Budget threshold warning level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetWarning {
    None,
    Warning,
    Critical,
    Exceeded,
}

/// Configurable token budget with threshold warnings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    pub max_tokens: u64,
    pub warning_threshold: f64,
    pub critical_threshold: f64,
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            max_tokens: 1_000_000,
            warning_threshold: 0.75,
            critical_threshold: 0.90,
        }
    }
}

impl TokenBudget {
    pub fn new(max_tokens: u64) -> Self {
        Self {
            max_tokens,
            ..Default::default()
        }
    }

    pub fn with_thresholds(mut self, warning: f64, critical: f64) -> Self {
        self.warning_threshold = warning;
        self.critical_threshold = critical;
        self
    }

    /// Check budget status based on current usage
    pub fn check(&self, used_tokens: u64) -> BudgetWarning {
        if used_tokens >= self.max_tokens {
            BudgetWarning::Exceeded
        } else {
            let ratio = used_tokens as f64 / self.max_tokens as f64;
            if ratio >= self.critical_threshold {
                BudgetWarning::Critical
            } else if ratio >= self.warning_threshold {
                BudgetWarning::Warning
            } else {
                BudgetWarning::None
            }
        }
    }

    pub fn remaining(&self, used_tokens: u64) -> u64 {
        self.max_tokens.saturating_sub(used_tokens)
    }
}

/// Token usage tracker for a session
pub struct TokenTracker {
    stats: UsageStats,
    budget: Option<TokenBudget>,
    current_conversation: Option<String>,
}

impl TokenTracker {
    pub fn new() -> Self {
        Self {
            stats: UsageStats::default(),
            budget: None,
            current_conversation: None,
        }
    }

    pub fn with_budget(mut self, budget: TokenBudget) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Set the current conversation for per-conversation tracking
    pub fn set_conversation(&mut self, conversation_id: impl Into<String>) {
        self.current_conversation = Some(conversation_id.into());
    }

    /// Record usage from a request
    pub fn record(&mut self, usage: RequestUsage) -> BudgetWarning {
        self.stats.total_input_tokens += usage.input_tokens;
        self.stats.total_output_tokens += usage.output_tokens;
        self.stats.request_count += 1;

        if let Some(ref conv_id) = self.current_conversation {
            let conv = self
                .stats
                .per_conversation
                .entry(conv_id.clone())
                .or_default();
            conv.input_tokens += usage.input_tokens;
            conv.output_tokens += usage.output_tokens;
            conv.request_count += 1;
        }

        self.budget_warning()
    }

    /// Get current budget warning level
    pub fn budget_warning(&self) -> BudgetWarning {
        match &self.budget {
            Some(budget) => budget.check(self.stats.total_tokens()),
            None => BudgetWarning::None,
        }
    }

    /// Get remaining tokens in budget
    pub fn budget_remaining(&self) -> Option<u64> {
        self.budget
            .as_ref()
            .map(|b| b.remaining(self.stats.total_tokens()))
    }

    /// Get current usage statistics
    pub fn stats(&self) -> &UsageStats {
        &self.stats
    }

    /// Get usage for a specific conversation
    pub fn conversation_usage(&self, conversation_id: &str) -> Option<&ConversationUsage> {
        self.stats.per_conversation.get(conversation_id)
    }

    /// Reset session statistics
    pub fn reset(&mut self) {
        self.stats = UsageStats::default();
    }
}

impl Default for TokenTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_usage() {
        let usage = RequestUsage {
            input_tokens: 100,
            output_tokens: 50,
        };
        assert_eq!(usage.total(), 150);
    }

    #[test]
    fn test_token_budget_check() {
        let budget = TokenBudget::new(1000);

        assert_eq!(budget.check(0), BudgetWarning::None);
        assert_eq!(budget.check(500), BudgetWarning::None);
        assert_eq!(budget.check(750), BudgetWarning::Warning);
        assert_eq!(budget.check(900), BudgetWarning::Critical);
        assert_eq!(budget.check(1000), BudgetWarning::Exceeded);
        assert_eq!(budget.check(1500), BudgetWarning::Exceeded);
    }

    #[test]
    fn test_token_budget_remaining() {
        let budget = TokenBudget::new(1000);
        assert_eq!(budget.remaining(300), 700);
        assert_eq!(budget.remaining(1000), 0);
        assert_eq!(budget.remaining(1500), 0);
    }

    #[test]
    fn test_token_budget_custom_thresholds() {
        let budget = TokenBudget::new(100)
            .with_thresholds(0.50, 0.80);

        assert_eq!(budget.check(49), BudgetWarning::None);
        assert_eq!(budget.check(50), BudgetWarning::Warning);
        assert_eq!(budget.check(80), BudgetWarning::Critical);
        assert_eq!(budget.check(100), BudgetWarning::Exceeded);
    }

    #[test]
    fn test_tracker_record() {
        let mut tracker = TokenTracker::new();

        tracker.record(RequestUsage {
            input_tokens: 100,
            output_tokens: 50,
        });

        assert_eq!(tracker.stats().total_input_tokens, 100);
        assert_eq!(tracker.stats().total_output_tokens, 50);
        assert_eq!(tracker.stats().request_count, 1);
        assert_eq!(tracker.stats().total_tokens(), 150);
    }

    #[test]
    fn test_tracker_accumulation() {
        let mut tracker = TokenTracker::new();

        tracker.record(RequestUsage {
            input_tokens: 100,
            output_tokens: 50,
        });
        tracker.record(RequestUsage {
            input_tokens: 200,
            output_tokens: 100,
        });

        assert_eq!(tracker.stats().total_input_tokens, 300);
        assert_eq!(tracker.stats().total_output_tokens, 150);
        assert_eq!(tracker.stats().request_count, 2);
    }

    #[test]
    fn test_tracker_per_conversation() {
        let mut tracker = TokenTracker::new();

        tracker.set_conversation("conv-1");
        tracker.record(RequestUsage {
            input_tokens: 100,
            output_tokens: 50,
        });

        tracker.set_conversation("conv-2");
        tracker.record(RequestUsage {
            input_tokens: 200,
            output_tokens: 100,
        });

        let conv1 = tracker.conversation_usage("conv-1").unwrap();
        assert_eq!(conv1.input_tokens, 100);
        assert_eq!(conv1.total_tokens(), 150);

        let conv2 = tracker.conversation_usage("conv-2").unwrap();
        assert_eq!(conv2.input_tokens, 200);
        assert_eq!(conv2.total_tokens(), 300);
    }

    #[test]
    fn test_tracker_with_budget() {
        let mut tracker = TokenTracker::new()
            .with_budget(TokenBudget::new(200));

        let warning = tracker.record(RequestUsage {
            input_tokens: 50,
            output_tokens: 50,
        });
        assert_eq!(warning, BudgetWarning::None);

        let warning = tracker.record(RequestUsage {
            input_tokens: 50,
            output_tokens: 0,
        });
        assert_eq!(warning, BudgetWarning::Warning);

        let warning = tracker.record(RequestUsage {
            input_tokens: 30,
            output_tokens: 0,
        });
        assert_eq!(warning, BudgetWarning::Critical);

        let warning = tracker.record(RequestUsage {
            input_tokens: 50,
            output_tokens: 0,
        });
        assert_eq!(warning, BudgetWarning::Exceeded);
    }

    #[test]
    fn test_tracker_budget_remaining() {
        let mut tracker = TokenTracker::new()
            .with_budget(TokenBudget::new(1000));

        assert_eq!(tracker.budget_remaining(), Some(1000));

        tracker.record(RequestUsage {
            input_tokens: 300,
            output_tokens: 200,
        });

        assert_eq!(tracker.budget_remaining(), Some(500));
    }

    #[test]
    fn test_tracker_no_budget() {
        let tracker = TokenTracker::new();
        assert_eq!(tracker.budget_warning(), BudgetWarning::None);
        assert_eq!(tracker.budget_remaining(), None);
    }

    #[test]
    fn test_tracker_reset() {
        let mut tracker = TokenTracker::new();

        tracker.record(RequestUsage {
            input_tokens: 100,
            output_tokens: 50,
        });
        assert_eq!(tracker.stats().total_tokens(), 150);

        tracker.reset();
        assert_eq!(tracker.stats().total_tokens(), 0);
        assert_eq!(tracker.stats().request_count, 0);
    }
}
