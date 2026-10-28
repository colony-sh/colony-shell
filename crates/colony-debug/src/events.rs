//! Structured trace events emitted during script execution.
//!
//! Serialized as JSON to fd 6 (stddiag) and consumed by the debug TUI.

use serde::Serialize;

/// A trace event emitted during execution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum TraceEvent {
    /// Entering a new source line.
    LineEnter {
        line: u32,
        column: u32,
        source: String,
    },

    /// Parser produced an AST node.
    Parse {
        line: u32,
        node_kind: String,
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ast_detail: Option<String>,
    },

    /// An expansion phase ran and transformed text.
    Expand {
        line: u32,
        phase: String,
        input: String,
        output: String,
        explanation: String,
    },

    /// A variable was assigned.
    Assign {
        line: u32,
        name: String,
        value: String,
    },

    /// An external command is about to execute.
    Execute {
        line: u32,
        command: String,
        resolved_path: String,
        args: Vec<String>,
    },

    /// Command finished with exit code.
    ExitCode {
        line: u32,
        command: String,
        code: i32,
        duration_us: u64,
    },

    /// Control flow decision (if/while/for/case).
    ControlFlow {
        line: u32,
        kind: String,
        decision: String,
        detail: String,
    },

    /// Loop iteration.
    LoopIteration {
        line: u32,
        variable: String,
        value: String,
        iteration: u32,
        total: Option<u32>,
    },

    /// Pipeline setup.
    PipelineSetup {
        line: u32,
        commands: Vec<String>,
        stage_count: usize,
    },

    /// Redirect applied.
    Redirect {
        line: u32,
        kind: String,
        target: String,
    },

    /// Finished executing a line.
    LineExit { line: u32, duration_us: u64 },

    /// First-occurrence concept explanation (X-Ray teach mode).
    Concept { name: String, explanation: String },

    /// Debugger hit a breakpoint.
    BreakpointHit {
        id: u32,
        line: u32,
        condition: Option<String>,
    },

    /// Variable watch triggered.
    WatchTriggered {
        expression: String,
        old_value: String,
        new_value: String,
    },
}

/// Complexity level for X-Ray pacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Complexity {
    Low,
    Medium,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_event_serializes_to_json() {
        let event = TraceEvent::LineEnter {
            line: 5,
            column: 1,
            source: "echo hello".to_string(),
        };
        let json = serde_json::to_string(&event).expect("serialization should succeed");
        assert!(json.contains("\"type\":\"LineEnter\""));
        assert!(json.contains("\"line\":5"));
    }

    #[test]
    fn expand_event_serializes() {
        let event = TraceEvent::Expand {
            line: 3,
            phase: "parameter".to_string(),
            input: "${1:-default}".to_string(),
            output: "default".to_string(),
            explanation: "$1 is unset, using default value".to_string(),
        };
        let json = serde_json::to_string(&event).expect("serialization should succeed");
        assert!(json.contains("\"phase\":\"parameter\""));
    }

    #[test]
    fn breakpoint_hit_serializes() {
        let event = TraceEvent::BreakpointHit {
            id: 1,
            line: 10,
            condition: Some("$x > 0".to_string()),
        };
        let json = serde_json::to_string(&event).expect("serialization should succeed");
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"condition\":\"$x > 0\""));
    }

    #[test]
    fn complexity_ordering() {
        assert!(Complexity::Low < Complexity::Medium);
        assert!(Complexity::Medium < Complexity::High);
    }
}
