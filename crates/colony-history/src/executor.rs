//! Executor detection for identifying command origins.
//!
//! Detects whether a command was executed by the user, colony-agent,
//! or an external AI agent (Claude Code, Codex, Cursor, etc.) via
//! environment variable heuristics.

use std::collections::HashMap;

use crate::record::Executor;

/// A pattern for detecting an external AI agent via environment variable.
#[derive(Debug, Clone)]
pub struct AgentPattern {
    /// Environment variable to check for presence.
    pub env_var: String,
    /// Name to use when this agent is detected.
    pub name: String,
}

/// Detects the executor origin of a command by inspecting environment
/// variables.
pub struct ExecutorDetector {
    patterns: Vec<AgentPattern>,
}

impl Default for ExecutorDetector {
    /// Create a detector with default patterns for known AI agents.
    fn default() -> Self {
        Self::new(vec![
            AgentPattern {
                env_var: "CLAUDE_CODE_ENTRYPOINT".to_string(),
                name: "claude-code".to_string(),
            },
            AgentPattern {
                env_var: "CODEX".to_string(),
                name: "codex".to_string(),
            },
            AgentPattern {
                env_var: "CURSOR_SESSION".to_string(),
                name: "cursor".to_string(),
            },
            AgentPattern {
                env_var: "WINDSURF_SESSION".to_string(),
                name: "windsurf".to_string(),
            },
            AgentPattern {
                env_var: "AIDER".to_string(),
                name: "aider".to_string(),
            },
        ])
    }
}

impl ExecutorDetector {
    /// Create a detector with custom patterns.
    pub fn new(patterns: Vec<AgentPattern>) -> Self {
        Self { patterns }
    }

    /// Detect the executor by checking environment variables.
    ///
    /// Returns the first matching external agent pattern, or
    /// `Executor::User` if none match.
    pub fn detect(&self, env: &HashMap<String, String>) -> Executor {
        for pattern in &self.patterns {
            if env.contains_key(&pattern.env_var) {
                return Executor::External(pattern.name.clone());
            }
        }
        Executor::User
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_default() {
        let detector = ExecutorDetector::new(vec![]);
        let result = detector.detect(&HashMap::new());
        assert_eq!(result, Executor::User);
    }

    #[test]
    fn test_detects_claude_code() {
        let detector = ExecutorDetector::default();
        let mut env = HashMap::new();
        env.insert("CLAUDE_CODE_ENTRYPOINT".to_string(), "1".to_string());
        let result = detector.detect(&env);
        assert_eq!(result, Executor::External("claude-code".to_string()));
    }

    #[test]
    fn test_detects_codex() {
        let detector = ExecutorDetector::default();
        let mut env = HashMap::new();
        env.insert("CODEX".to_string(), "true".to_string());
        let result = detector.detect(&env);
        assert_eq!(result, Executor::External("codex".to_string()));
    }

    #[test]
    fn test_custom_detection_pattern() {
        let detector = ExecutorDetector::new(vec![AgentPattern {
            env_var: "MY_AGENT".to_string(),
            name: "my-agent".to_string(),
        }]);
        let mut env = HashMap::new();
        env.insert("MY_AGENT".to_string(), "true".to_string());
        let result = detector.detect(&env);
        assert_eq!(result, Executor::External("my-agent".to_string()));
    }

    #[test]
    fn test_first_match_wins() {
        let detector = ExecutorDetector::default();
        let mut env = HashMap::new();
        env.insert("CLAUDE_CODE_ENTRYPOINT".to_string(), "1".to_string());
        env.insert("CODEX".to_string(), "1".to_string());
        let result = detector.detect(&env);
        // CLAUDE_CODE_ENTRYPOINT is first in default list
        assert_eq!(result, Executor::External("claude-code".to_string()));
    }

    #[test]
    fn test_no_match_returns_user() {
        let detector = ExecutorDetector::default();
        let mut env = HashMap::new();
        env.insert("UNRELATED_VAR".to_string(), "1".to_string());
        let result = detector.detect(&env);
        assert_eq!(result, Executor::User);
    }
}
