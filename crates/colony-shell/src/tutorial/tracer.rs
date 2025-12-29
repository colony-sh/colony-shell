//! TraceEmitter - writes structured trace events to fd 6 (stdtrace)
//!
//! The tracer is the bridge between the executor and the TUI/file output.
//! It emits JSON events that describe what's happening during script execution.

use crate::tutorial::events::{BranchType, ExpansionPhase, LoopType, TraceEvent};
use serde_json;
use std::collections::HashSet;
use std::io::{self, Write};
use std::os::unix::io::{FromRawFd, RawFd};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Instant;

/// File descriptor for stdtrace (Colony's trace channel)
#[allow(dead_code)]
pub const STDTRACE_FD: RawFd = 6;

/// Trace depth levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceDepth {
    /// Level 2: Shell mechanics (default)
    /// Shows what the shell is doing in human-readable terms
    #[default]
    Mechanics,
    /// Level 3: Deep internals
    /// Shows AST structure and syscall-level activity
    Internals,
}

/// Configuration for the tracer
#[derive(Debug, Clone)]
pub struct TracerConfig {
    /// Current trace depth
    pub depth: TraceDepth,
    /// Teach mode: explain concepts on first occurrence
    pub teach_mode: bool,
    /// Lines marked for detailed attention
    pub marked_lines: HashSet<u32>,
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self {
            depth: TraceDepth::Mechanics,
            teach_mode: false,
            marked_lines: HashSet::new(),
        }
    }
}

/// Output destination for trace events
#[allow(dead_code)]
pub enum TraceOutput {
    /// Write to fd 6 (stdtrace)
    Fd6,
    /// Write to a file
    File(std::fs::File),
    /// Send to a channel (for TUI)
    Channel(Sender<TraceEvent>),
    /// Write to a Vec (for testing)
    Buffer(Vec<TraceEvent>),
}

/// Emits structured trace events during script execution
#[allow(dead_code)]
pub struct TraceEmitter {
    config: TracerConfig,
    output: TraceOutput,
    /// Concepts already explained (for teach mode)
    seen_concepts: HashSet<String>,
    /// Current line being executed
    current_line: u32,
    /// Timer for line duration
    line_start: Option<Instant>,
    /// Script start time
    script_start: Option<Instant>,
}

#[allow(dead_code)]
impl TraceEmitter {
    /// Create a new tracer that writes to fd 6
    pub fn new_fd6(config: TracerConfig) -> Self {
        Self {
            config,
            output: TraceOutput::Fd6,
            seen_concepts: HashSet::new(),
            current_line: 0,
            line_start: None,
            script_start: None,
        }
    }

    /// Create a new tracer that writes to a file
    pub fn new_file(config: TracerConfig, path: &std::path::Path) -> io::Result<Self> {
        let file = std::fs::File::create(path)?;
        Ok(Self {
            config,
            output: TraceOutput::File(file),
            seen_concepts: HashSet::new(),
            current_line: 0,
            line_start: None,
            script_start: None,
        })
    }

    /// Create a new tracer that sends to a channel
    pub fn new_channel(config: TracerConfig, sender: Sender<TraceEvent>) -> Self {
        Self {
            config,
            output: TraceOutput::Channel(sender),
            seen_concepts: HashSet::new(),
            current_line: 0,
            line_start: None,
            script_start: None,
        }
    }

    /// Create a new tracer that buffers events (for testing)
    pub fn new_buffer(config: TracerConfig) -> Self {
        Self {
            config,
            output: TraceOutput::Buffer(Vec::new()),
            seen_concepts: HashSet::new(),
            current_line: 0,
            line_start: None,
            script_start: None,
        }
    }

    /// Get buffered events (only works with Buffer output)
    pub fn take_events(&mut self) -> Vec<TraceEvent> {
        match &mut self.output {
            TraceOutput::Buffer(events) => std::mem::take(events),
            _ => Vec::new(),
        }
    }

    /// Get current configuration
    pub fn config(&self) -> &TracerConfig {
        &self.config
    }

    /// Get mutable configuration
    pub fn config_mut(&mut self) -> &mut TracerConfig {
        &mut self.config
    }

    /// Toggle trace depth
    pub fn toggle_depth(&mut self) {
        self.config.depth = match self.config.depth {
            TraceDepth::Mechanics => TraceDepth::Internals,
            TraceDepth::Internals => TraceDepth::Mechanics,
        };
    }

    /// Toggle teach mode
    pub fn toggle_teach_mode(&mut self) {
        self.config.teach_mode = !self.config.teach_mode;
    }

    /// Mark a line for detailed attention
    pub fn mark_line(&mut self, line: u32) {
        self.config.marked_lines.insert(line);
    }

    /// Unmark a line
    pub fn unmark_line(&mut self, line: u32) {
        self.config.marked_lines.remove(&line);
    }

    /// Check if a line is marked
    pub fn is_line_marked(&self, line: u32) -> bool {
        self.config.marked_lines.contains(&line)
    }

    /// Emit an event
    pub fn emit(&mut self, event: TraceEvent) {
        match &mut self.output {
            TraceOutput::Fd6 => {
                // Safety: fd 6 should be set up by the shell
                // In production, we'd validate this
                if let Ok(json) = serde_json::to_string(&event) {
                    let mut output = unsafe { std::fs::File::from_raw_fd(STDTRACE_FD) };
                    let _ = writeln!(output, "{}", json);
                    // Don't close fd 6 when output is dropped
                    std::mem::forget(output);
                }
            }
            TraceOutput::File(file) => {
                if let Ok(json) = serde_json::to_string(&event) {
                    let _ = writeln!(file, "{}", json);
                }
            }
            TraceOutput::Channel(sender) => {
                let _ = sender.send(event);
            }
            TraceOutput::Buffer(events) => {
                events.push(event);
            }
        }
    }

    /// Emit script start event
    pub fn emit_script_start(&mut self, path: PathBuf, lines: u32) {
        self.script_start = Some(Instant::now());
        self.emit(TraceEvent::ScriptStart { path, lines });
    }

    /// Emit line enter event
    pub fn emit_line_enter(&mut self, line: u32, source: String) {
        self.current_line = line;
        self.line_start = Some(Instant::now());
        self.emit(TraceEvent::LineEnter { line, source });
    }

    /// Emit line exit event
    pub fn emit_line_exit(&mut self, line: u32) {
        let duration_us = self
            .line_start
            .map(|start| start.elapsed().as_micros() as u64)
            .unwrap_or(0);
        self.line_start = None;
        self.emit(TraceEvent::LineExit { line, duration_us });
    }

    /// Emit script end event
    pub fn emit_script_end(&mut self, exit_code: i32) {
        let duration_ms = self
            .script_start
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);
        self.emit(TraceEvent::ScriptEnd {
            exit_code,
            duration_ms,
        });
    }

    /// Emit parse event
    pub fn emit_parse(
        &mut self,
        line: u32,
        ast_summary: String,
        ast_full: Option<serde_json::Value>,
    ) {
        let ast_full = if self.config.depth == TraceDepth::Internals {
            ast_full
        } else {
            None
        };
        self.emit(TraceEvent::Parse {
            line,
            ast_summary,
            ast_full,
        });
    }

    /// Emit expansion event
    pub fn emit_expand(
        &mut self,
        line: u32,
        phase: ExpansionPhase,
        input: String,
        output: String,
        explanation: String,
    ) {
        self.emit(TraceEvent::Expand {
            line,
            phase,
            input,
            output,
            explanation,
        });
    }

    /// Emit assignment event
    pub fn emit_assign(
        &mut self,
        line: u32,
        name: String,
        value: String,
        previous: Option<String>,
    ) {
        self.emit(TraceEvent::Assign {
            line,
            name,
            value,
            previous,
        });
    }

    /// Emit command resolution event
    pub fn emit_resolve(&mut self, line: u32, command: String, resolved: PathBuf) {
        self.emit(TraceEvent::Resolve {
            line,
            command,
            resolved,
        });
    }

    /// Emit execute event
    pub fn emit_execute(
        &mut self,
        line: u32,
        command: String,
        args: Vec<String>,
        env: std::collections::HashMap<String, String>,
    ) {
        self.emit(TraceEvent::Execute {
            line,
            command,
            args,
            env,
        });
    }

    /// Emit builtin event
    pub fn emit_builtin(&mut self, line: u32, name: String, args: Vec<String>) {
        self.emit(TraceEvent::Builtin { line, name, args });
    }

    /// Emit exit status event
    pub fn emit_exit_status(&mut self, line: u32, code: i32, signal: Option<String>) {
        self.emit(TraceEvent::ExitStatus { line, code, signal });
    }

    /// Emit loop enter event
    pub fn emit_loop_enter(
        &mut self,
        line: u32,
        loop_type: LoopType,
        variable: Option<String>,
        values: Option<Vec<String>>,
    ) {
        self.emit(TraceEvent::LoopEnter {
            line,
            loop_type,
            variable,
            values,
        });
    }

    /// Emit loop iteration event
    pub fn emit_loop_iteration(&mut self, line: u32, iteration: u32, value: Option<String>) {
        self.emit(TraceEvent::LoopIteration {
            line,
            iteration,
            value,
        });
    }

    /// Emit loop exit event
    pub fn emit_loop_exit(&mut self, line: u32, iterations: u32) {
        self.emit(TraceEvent::LoopExit { line, iterations });
    }

    /// Emit branch event
    pub fn emit_branch(
        &mut self,
        line: u32,
        branch_type: BranchType,
        taken: String,
        condition: String,
        result: bool,
    ) {
        self.emit(TraceEvent::Branch {
            line,
            branch_type,
            taken,
            condition,
            result,
        });
    }

    /// Emit a concept explanation (teach mode)
    ///
    /// Returns true if the concept was emitted (first occurrence),
    /// false if it was already explained.
    pub fn emit_concept_if_new(
        &mut self,
        id: &str,
        name: &str,
        explanation: &str,
        related: Vec<String>,
    ) -> bool {
        if !self.config.teach_mode {
            return false;
        }

        if self.seen_concepts.contains(id) {
            return false;
        }

        self.seen_concepts.insert(id.to_string());
        self.emit(TraceEvent::Concept {
            id: id.to_string(),
            name: name.to_string(),
            explanation: explanation.to_string(),
            related,
        });
        true
    }

    /// Emit command hint
    pub fn emit_command_hint(&mut self, line: u32, command: String, hint: String) {
        self.emit(TraceEvent::CommandHint {
            line,
            command,
            hint,
        });
    }

    /// Emit error event
    pub fn emit_error(&mut self, line: u32, message: String) {
        self.emit(TraceEvent::Error { line, message });
    }

    /// Reset seen concepts (for --teach-reset)
    pub fn reset_concepts(&mut self) {
        self.seen_concepts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_tracer() {
        let config = TracerConfig::default();
        let mut tracer = TraceEmitter::new_buffer(config);

        tracer.emit_line_enter(1, "echo hello".to_string());
        tracer.emit_line_exit(1);

        let events = tracer.take_events();
        assert_eq!(events.len(), 2);

        match &events[0] {
            TraceEvent::LineEnter { line, source } => {
                assert_eq!(*line, 1);
                assert_eq!(source, "echo hello");
            }
            _ => panic!("Expected LineEnter"),
        }
    }

    #[test]
    fn test_teach_mode() {
        let config = TracerConfig {
            teach_mode: true,
            ..Default::default()
        };
        let mut tracer = TraceEmitter::new_buffer(config);

        // First occurrence should emit
        assert!(tracer.emit_concept_if_new(
            "param_default",
            "Parameter Expansion with Default",
            "explanation here",
            vec![]
        ));

        // Second occurrence should not emit
        assert!(!tracer.emit_concept_if_new(
            "param_default",
            "Parameter Expansion with Default",
            "explanation here",
            vec![]
        ));

        let events = tracer.take_events();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_depth_toggle() {
        let config = TracerConfig::default();
        let mut tracer = TraceEmitter::new_buffer(config);

        assert_eq!(tracer.config().depth, TraceDepth::Mechanics);
        tracer.toggle_depth();
        assert_eq!(tracer.config().depth, TraceDepth::Internals);
        tracer.toggle_depth();
        assert_eq!(tracer.config().depth, TraceDepth::Mechanics);
    }
}
