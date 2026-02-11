//! Trace event types for tutorial mode
//!
//! These events are emitted during script execution and consumed by the TUI
//! or written to fd 6 (stddiag) as JSON.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A single trace event emitted during script execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceEvent {
    /// Script execution started
    ScriptStart { path: PathBuf, lines: u32 },

    /// Entering a new line/statement
    LineEnter { line: u32, source: String },

    /// Parse information for current statement
    Parse {
        line: u32,
        /// Human-readable AST summary (Level 2)
        ast_summary: String,
        /// Full AST as JSON (Level 3, optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        ast_full: Option<serde_json::Value>,
    },

    /// Expansion phase completed
    Expand {
        line: u32,
        /// Which expansion phase (tilde, parameter, command, arithmetic, etc.)
        phase: ExpansionPhase,
        /// Input before expansion
        input: String,
        /// Output after expansion
        output: String,
        /// Human-readable explanation
        explanation: String,
    },

    /// Variable assignment
    Assign {
        line: u32,
        name: String,
        value: String,
        /// Previous value if any
        #[serde(skip_serializing_if = "Option::is_none")]
        previous: Option<String>,
    },

    /// Command resolution (PATH lookup)
    Resolve {
        line: u32,
        command: String,
        /// Resolved path
        resolved: PathBuf,
    },

    /// External command execution
    Execute {
        line: u32,
        command: String,
        args: Vec<String>,
        /// Environment variables set for this command
        #[serde(skip_serializing_if = "HashMap::is_empty")]
        env: HashMap<String, String>,
    },

    /// Builtin command execution
    Builtin {
        line: u32,
        name: String,
        args: Vec<String>,
    },

    /// Redirect applied
    Redirect {
        line: u32,
        /// e.g., "stdout to file.txt", "stdin from data.txt"
        description: String,
        /// Full redirect syntax
        syntax: String,
    },

    /// Pipeline stage
    PipelineStage {
        line: u32,
        stage: u32,
        total_stages: u32,
        command: String,
    },

    /// Loop entry
    LoopEnter {
        line: u32,
        loop_type: LoopType,
        /// For `for` loops: the variable name
        #[serde(skip_serializing_if = "Option::is_none")]
        variable: Option<String>,
        /// For `for` loops: the iteration values
        #[serde(skip_serializing_if = "Option::is_none")]
        values: Option<Vec<String>>,
    },

    /// Loop iteration
    LoopIteration {
        line: u32,
        iteration: u32,
        /// Current value (for `for` loops)
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },

    /// Loop exit
    LoopExit { line: u32, iterations: u32 },

    /// Conditional branch taken
    Branch {
        line: u32,
        branch_type: BranchType,
        /// Which branch was taken (if/elif/else, case pattern, etc.)
        taken: String,
        /// Condition that was evaluated
        condition: String,
        /// Result of condition evaluation
        result: bool,
    },

    /// Function call
    FunctionCall {
        line: u32,
        name: String,
        args: Vec<String>,
    },

    /// Function return
    FunctionReturn {
        line: u32,
        name: String,
        exit_code: i32,
    },

    /// Command exit status
    ExitStatus {
        line: u32,
        code: i32,
        /// Signal name if killed by signal
        #[serde(skip_serializing_if = "Option::is_none")]
        signal: Option<String>,
    },

    /// Concept explanation (teach mode)
    Concept {
        /// Concept identifier (e.g., "parameter_expansion_default")
        id: String,
        /// Human-readable name
        name: String,
        /// Detailed explanation
        explanation: String,
        /// Related forms/variants
        #[serde(skip_serializing_if = "Vec::is_empty")]
        related: Vec<String>,
    },

    /// External command hint
    CommandHint {
        line: u32,
        command: String,
        /// Brief description of what this command does
        hint: String,
    },

    /// Line execution completed
    LineExit {
        line: u32,
        /// Duration in microseconds
        duration_us: u64,
    },

    /// Script execution completed
    ScriptEnd {
        exit_code: i32,
        /// Total duration in milliseconds
        duration_ms: u64,
    },

    /// Error during execution
    Error { line: u32, message: String },

    /// Command output (stdout)
    Output { line: u32, text: String },

    /// Environment/variable state snapshot
    Environment {
        line: u32,
        /// Shell variables (name -> value)
        variables: HashMap<String, String>,
        /// Recently changed variables (for highlighting)
        #[serde(skip_serializing_if = "Vec::is_empty")]
        changed: Vec<String>,
    },
}

/// Expansion phases in order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpansionPhase {
    /// ~user expansion
    Tilde,
    /// ${var}, $var expansion
    Parameter,
    /// $(...) and `...` expansion
    Command,
    /// $((...)) expansion
    Arithmetic,
    /// Splitting on IFS
    FieldSplit,
    /// *, ?, [...] expansion
    Pathname,
    /// Quote removal
    QuoteRemoval,
}

impl ExpansionPhase {
    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Tilde => "tilde",
            Self::Parameter => "parameter",
            Self::Command => "command substitution",
            Self::Arithmetic => "arithmetic",
            Self::FieldSplit => "field splitting",
            Self::Pathname => "pathname (glob)",
            Self::QuoteRemoval => "quote removal",
        }
    }
}

/// Types of loops
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopType {
    For,
    While,
    Until,
    Select,
}

/// Types of conditional branches
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchType {
    If,
    Elif,
    Else,
    Case,
    And, // &&
    Or,  // ||
}

/// Complexity level for pacing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplexityLevel {
    /// Simple commands, basic assignments
    #[default]
    Low,
    /// Parameter expansion, simple redirects
    Medium,
    /// Pipelines, command substitution, loops
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = TraceEvent::LineEnter {
            line: 5,
            source: "echo \"hello\"".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"line_enter\""));
        assert!(json.contains("\"line\":5"));

        // Roundtrip
        let parsed: TraceEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            TraceEvent::LineEnter { line, source } => {
                assert_eq!(line, 5);
                assert_eq!(source, "echo \"hello\"");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_expand_event() {
        let event = TraceEvent::Expand {
            line: 3,
            phase: ExpansionPhase::Parameter,
            input: "${1:-default}".to_string(),
            output: "default".to_string(),
            explanation: "$1 is unset, using default value".to_string(),
        };

        let json = serde_json::to_string_pretty(&event).unwrap();
        assert!(json.contains("\"phase\":\"parameter\""));
    }
}
