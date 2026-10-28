# F071: Unified Shell Debugger (Debug + X-Ray) — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a `colony-debug` crate providing a unified shell script debugger with two modes: **Debug** (breakpoints, stepping, inspection) and **X-Ray** (auto-play with real-time annotations explaining parsing, expansion, and execution). Both share the same trace infrastructure and TUI.

**Architecture:** `DebugController` hooks into `Executor::execute_statement_recursive()` and `ExpansionContext::expand_word()` via an `Option<Arc<DebugController>>`. It emits structured `TraceEvent` JSON to fd 6 (stddiag). A split-pane ratatui TUI renders source on the left and annotations/variables on the right. X-Ray mode is just the debugger running with auto-continue and complexity-based pacing.

**Tech Stack:** Rust, ratatui 0.29, crossterm, serde_json, tokio channels. Hooks into colony-exec, colony-expand, colony-parser.

**Important:** Follow `RUST_STYLE_GUIDE.md`. AST nodes already carry `Span` (line/column). Existing `xtrace.rs` traces commands via `set -x`. Existing `ReflectionObserver` trait provides an observer pattern. The executor is at `crates/colony-exec/src/lib.rs:3723` (`execute_statement_recursive`). Expansion is at `crates/colony-expand/src/lib.rs:481` (`expand_word`).

---

## File Structure

```
crates/colony-debug/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API
│   ├── events.rs           # TraceEvent enum (JSON serializable)
│   ├── controller.rs       # DebugController: breakpoints, stepping, state machine
│   ├── breakpoint.rs       # Breakpoint, BreakpointManager
│   ├── complexity.rs       # ComplexityScorer for X-Ray pacing
│   ├── explain.rs          # Human-readable explanations for shell constructs
│   └── tui/
│       ├── mod.rs          # DebugTui orchestration + event loop
│       ├── source_pane.rs  # Left: script source with line markers
│       └── info_pane.rs    # Right: annotations (X-Ray) or variables (Debug)
```

**Existing files to modify:**
- `Cargo.toml` (workspace root) — add `colony-debug` member
- `crates/colony-exec/src/lib.rs` — add `debug_controller` field, call hooks in `execute_statement_recursive`
- `crates/colony-expand/src/lib.rs` — add trace hook in `expand_word` and `expand_part`
- `crates/colony-shell/src/main.rs` — add `--debug` and `--xray` CLI flags

---

## Chunk 1: Trace Events + Controller Core

### Task 1: Create colony-debug crate with TraceEvent types

**Files:**
- Create: `crates/colony-debug/Cargo.toml`
- Create: `crates/colony-debug/src/lib.rs`
- Create: `crates/colony-debug/src/events.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "colony-debug"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
description = "Shell script debugger and X-Ray tracer for Colony Shell"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = { workspace = true }
tokio = { version = "1", features = ["sync", "time"] }
crossterm = { version = "0.28" }
ratatui = "0.29"
```

- [ ] **Step 2: Create events.rs — structured trace events**

```rust
//! Structured trace events emitted during script execution.
//! Serialized as JSON to fd 6 (stddiag).

use serde::Serialize;

/// A trace event emitted during execution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
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
    LineExit {
        line: u32,
        duration_us: u64,
    },

    /// First-occurrence concept explanation (X-Ray teach mode).
    Concept {
        name: String,
        explanation: String,
    },

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
```

- [ ] **Step 3: Create lib.rs**

```rust
//! Colony Shell debugger and X-Ray tracer.
//!
//! Provides `DebugController` for breakpoints and stepping,
//! `TraceEvent` for structured execution tracing, and a
//! split-pane ratatui TUI for both debug and X-Ray modes.

pub mod breakpoint;
pub mod complexity;
pub mod controller;
pub mod events;
pub mod explain;
pub mod tui;

pub use controller::{DebugController, DebugMode};
pub use events::{Complexity, TraceEvent};
```

- [ ] **Step 4: Add to workspace**

Add `"crates/colony-debug"` to the `members` array in the root `Cargo.toml`. Also add `colony-debug = { path = "crates/colony-debug" }` to workspace dependencies.

- [ ] **Step 5: Create stubs for remaining modules**

Create empty stub files for `breakpoint.rs`, `complexity.rs`, `controller.rs`, `explain.rs`, `tui/mod.rs`, `tui/source_pane.rs`, `tui/info_pane.rs` with module doc comments.

- [ ] **Step 6: Verify compile**

Run: `cargo check -p colony-debug`

- [ ] **Step 7: Commit**

```bash
git add crates/colony-debug/ Cargo.toml
git commit -m "feat(F071): colony-debug crate skeleton with TraceEvent types"
```

### Task 2: DebugController + BreakpointManager

**Files:**
- Modify: `crates/colony-debug/src/controller.rs`
- Modify: `crates/colony-debug/src/breakpoint.rs`

- [ ] **Step 1: Implement breakpoint.rs**

```rust
//! Breakpoint management.

use serde::{Deserialize, Serialize};

/// A breakpoint definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub id: u32,
    pub line: u32,
    pub file: Option<String>,
    pub condition: Option<String>,
    pub enabled: bool,
    pub hit_count: u32,
}

/// Manages breakpoints for a debug session.
pub struct BreakpointManager {
    breakpoints: Vec<Breakpoint>,
    next_id: u32,
}

impl BreakpointManager {
    pub fn new() -> Self {
        Self {
            breakpoints: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add(&mut self, line: u32, condition: Option<String>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.breakpoints.push(Breakpoint {
            id,
            line,
            file: None,
            condition,
            enabled: true,
            hit_count: 0,
        });
        id
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.breakpoints.len();
        self.breakpoints.retain(|b| b.id != id);
        self.breakpoints.len() != before
    }

    pub fn toggle(&mut self, id: u32) {
        if let Some(bp) = self.breakpoints.iter_mut().find(|b| b.id == id) {
            bp.enabled = !bp.enabled;
        }
    }

    /// Check if any enabled breakpoint matches the given line.
    /// Returns the breakpoint ID if hit.
    pub fn check(&mut self, line: u32) -> Option<u32> {
        for bp in &mut self.breakpoints {
            if bp.enabled && bp.line == line {
                bp.hit_count += 1;
                return Some(bp.id);
            }
        }
        None
    }

    pub fn list(&self) -> &[Breakpoint] {
        &self.breakpoints
    }

    pub fn clear(&mut self) {
        self.breakpoints.clear();
    }
}

impl Default for BreakpointManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_check() {
        let mut mgr = BreakpointManager::new();
        let id = mgr.add(10, None);
        assert_eq!(mgr.check(10), Some(id));
        assert_eq!(mgr.check(11), None);
    }

    #[test]
    fn disable_skips() {
        let mut mgr = BreakpointManager::new();
        let id = mgr.add(5, None);
        mgr.toggle(id);
        assert_eq!(mgr.check(5), None);
    }

    #[test]
    fn remove_works() {
        let mut mgr = BreakpointManager::new();
        let id = mgr.add(5, None);
        assert!(mgr.remove(id));
        assert_eq!(mgr.check(5), None);
    }

    #[test]
    fn hit_count_increments() {
        let mut mgr = BreakpointManager::new();
        mgr.add(5, None);
        mgr.check(5);
        mgr.check(5);
        assert_eq!(mgr.list()[0].hit_count, 2);
    }
}
```

- [ ] **Step 2: Implement controller.rs**

```rust
//! Debug controller — state machine for debug and X-Ray modes.

use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::breakpoint::BreakpointManager;
use crate::events::TraceEvent;

/// Operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMode {
    /// Traditional debugger: pause at breakpoints, step through.
    Debug,
    /// X-Ray: auto-play with annotations and complexity-based pacing.
    XRay,
}

/// Execution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecState {
    Running,
    Paused,
    StepInto,
    StepOver { depth: u32 },
    StepOut { depth: u32 },
    Finished,
}

/// Commands sent from the TUI to the controller.
#[derive(Debug, Clone)]
pub enum DebugCommand {
    Continue,
    StepInto,
    StepOver,
    StepOut,
    Pause,
    AddBreakpoint { line: u32, condition: Option<String> },
    RemoveBreakpoint { id: u32 },
    ToggleBreakpoint { id: u32 },
    ToggleMode,
    ToggleDepth,
    Quit,
}

/// The debug controller. Shared between the executor thread and the TUI.
pub struct DebugController {
    mode: DebugMode,
    state: ExecState,
    pub breakpoints: BreakpointManager,
    event_tx: mpsc::UnboundedSender<TraceEvent>,
    command_rx: mpsc::UnboundedReceiver<DebugCommand>,
    current_line: u32,
    call_depth: u32,
    variables: HashMap<String, String>,
    teach_seen: std::collections::HashSet<String>,
}

impl DebugController {
    pub fn new(
        mode: DebugMode,
        event_tx: mpsc::UnboundedSender<TraceEvent>,
        command_rx: mpsc::UnboundedReceiver<DebugCommand>,
    ) -> Self {
        let initial_state = match mode {
            DebugMode::Debug => ExecState::Paused,
            DebugMode::XRay => ExecState::Running,
        };

        Self {
            mode,
            state: initial_state,
            breakpoints: BreakpointManager::new(),
            event_tx,
            command_rx,
            current_line: 0,
            call_depth: 0,
            variables: HashMap::new(),
            teach_seen: std::collections::HashSet::new(),
        }
    }

    pub fn mode(&self) -> DebugMode {
        self.mode
    }

    pub fn state(&self) -> ExecState {
        self.state
    }

    pub fn current_line(&self) -> u32 {
        self.current_line
    }

    /// Emit a trace event to the TUI.
    pub fn emit(&self, event: TraceEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Called by the executor before each statement.
    /// Returns true if execution should proceed, false if waiting for user input.
    pub fn on_statement_enter(&mut self, line: u32, source: &str) -> bool {
        self.current_line = line;

        self.emit(TraceEvent::LineEnter {
            line,
            column: 1,
            source: source.to_string(),
        });

        // Check breakpoints
        if let Some(bp_id) = self.breakpoints.check(line) {
            self.emit(TraceEvent::BreakpointHit {
                id: bp_id,
                line,
                condition: None,
            });
            self.state = ExecState::Paused;
        }

        // Handle stepping
        match self.state {
            ExecState::Running => true,
            ExecState::StepInto => {
                self.state = ExecState::Paused;
                false
            }
            ExecState::StepOver { depth } if self.call_depth <= depth => {
                self.state = ExecState::Paused;
                false
            }
            ExecState::StepOver { .. } => true,
            ExecState::StepOut { depth } if self.call_depth < depth => {
                self.state = ExecState::Paused;
                false
            }
            ExecState::StepOut { .. } => true,
            ExecState::Paused => false,
            ExecState::Finished => false,
        }
    }

    /// Called by the executor after a statement finishes.
    pub fn on_statement_exit(&mut self, line: u32, duration_us: u64) {
        self.emit(TraceEvent::LineExit { line, duration_us });
    }

    /// Called when entering a function/subshell.
    pub fn push_frame(&mut self) {
        self.call_depth += 1;
    }

    /// Called when leaving a function/subshell.
    pub fn pop_frame(&mut self) {
        self.call_depth = self.call_depth.saturating_sub(1);
    }

    /// Record a variable assignment for the inspector.
    pub fn on_assign(&mut self, name: &str, value: &str, line: u32) {
        self.variables.insert(name.to_string(), value.to_string());
        self.emit(TraceEvent::Assign {
            line,
            name: name.to_string(),
            value: value.to_string(),
        });
    }

    /// Record an expansion phase.
    pub fn on_expand(&mut self, line: u32, phase: &str, input: &str, output: &str, explanation: &str) {
        self.emit(TraceEvent::Expand {
            line,
            phase: phase.to_string(),
            input: input.to_string(),
            output: output.to_string(),
            explanation: explanation.to_string(),
        });
    }

    /// Wait for a continue/step command from the TUI (blocking).
    /// Called when the controller is paused.
    pub async fn wait_for_command(&mut self) {
        while self.state == ExecState::Paused {
            let Some(cmd) = self.command_rx.recv().await else {
                self.state = ExecState::Finished;
                return;
            };
            self.handle_command(cmd);
        }
    }

    fn handle_command(&mut self, cmd: DebugCommand) {
        match cmd {
            DebugCommand::Continue => self.state = ExecState::Running,
            DebugCommand::StepInto => self.state = ExecState::StepInto,
            DebugCommand::StepOver => {
                self.state = ExecState::StepOver {
                    depth: self.call_depth,
                };
            }
            DebugCommand::StepOut => {
                self.state = ExecState::StepOut {
                    depth: self.call_depth,
                };
            }
            DebugCommand::Pause => self.state = ExecState::Paused,
            DebugCommand::AddBreakpoint { line, condition } => {
                self.breakpoints.add(line, condition);
            }
            DebugCommand::RemoveBreakpoint { id } => {
                self.breakpoints.remove(id);
            }
            DebugCommand::ToggleBreakpoint { id } => {
                self.breakpoints.toggle(id);
            }
            DebugCommand::ToggleMode => {
                self.mode = match self.mode {
                    DebugMode::Debug => DebugMode::XRay,
                    DebugMode::XRay => DebugMode::Debug,
                };
            }
            DebugCommand::ToggleDepth => {} // Handled by TUI
            DebugCommand::Quit => self.state = ExecState::Finished,
        }
    }

    /// Get current variable state for the inspector.
    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_controller(mode: DebugMode) -> (DebugController, mpsc::UnboundedReceiver<TraceEvent>, mpsc::UnboundedSender<DebugCommand>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let ctrl = DebugController::new(mode, event_tx, cmd_rx);
        (ctrl, event_rx, cmd_tx)
    }

    #[test]
    fn xray_starts_running() {
        let (ctrl, _, _) = make_controller(DebugMode::XRay);
        assert_eq!(ctrl.state(), ExecState::Running);
    }

    #[test]
    fn debug_starts_paused() {
        let (ctrl, _, _) = make_controller(DebugMode::Debug);
        assert_eq!(ctrl.state(), ExecState::Paused);
    }

    #[test]
    fn breakpoint_pauses() {
        let (mut ctrl, mut rx, _) = make_controller(DebugMode::XRay);
        ctrl.breakpoints.add(5, None);
        let should_proceed = ctrl.on_statement_enter(5, "echo hello");
        assert!(!should_proceed);
        assert_eq!(ctrl.state(), ExecState::Paused);
        // Should have emitted LineEnter + BreakpointHit
        let _ = rx.try_recv().unwrap(); // LineEnter
        let event = rx.try_recv().unwrap(); // BreakpointHit
        assert!(matches!(event, TraceEvent::BreakpointHit { line: 5, .. }));
    }

    #[test]
    fn step_into_pauses_next() {
        let (mut ctrl, _, cmd_tx) = make_controller(DebugMode::Debug);
        ctrl.state = ExecState::StepInto;
        let should_proceed = ctrl.on_statement_enter(1, "x=1");
        assert!(!should_proceed);
        assert_eq!(ctrl.state(), ExecState::Paused);
    }

    #[test]
    fn on_assign_tracks_variable() {
        let (mut ctrl, _, _) = make_controller(DebugMode::XRay);
        ctrl.on_assign("foo", "bar", 1);
        assert_eq!(ctrl.variables().get("foo").unwrap(), "bar");
    }

    #[test]
    fn frame_depth_tracking() {
        let (mut ctrl, _, _) = make_controller(DebugMode::XRay);
        assert_eq!(ctrl.call_depth, 0);
        ctrl.push_frame();
        assert_eq!(ctrl.call_depth, 1);
        ctrl.pop_frame();
        assert_eq!(ctrl.call_depth, 0);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p colony-debug`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-debug/src/controller.rs crates/colony-debug/src/breakpoint.rs
git commit -m "feat(F071): DebugController state machine + BreakpointManager"
```

---

## Chunk 2: Complexity Scoring + Explanations

### Task 3: ComplexityScorer and explain module

**Files:**
- Modify: `crates/colony-debug/src/complexity.rs`
- Modify: `crates/colony-debug/src/explain.rs`

- [ ] **Step 1: Implement complexity.rs**

```rust
//! Complexity scoring for X-Ray auto-pacing.
//!
//! Scores statements by structural complexity to determine how long
//! the X-Ray should pause on each line.

use crate::events::Complexity;

/// Score a statement's complexity from its source text and AST kind.
/// Returns a complexity level that determines X-Ray pacing.
pub fn score(node_kind: &str, source: &str) -> Complexity {
    // Compound commands are always high
    if matches!(
        node_kind,
        "If" | "While" | "Until" | "For" | "Case" | "Try" | "Match"
    ) {
        return Complexity::High;
    }

    // Pipelines with multiple stages
    if node_kind == "Pipeline" && source.matches('|').count() >= 2 {
        return Complexity::High;
    }
    if node_kind == "Pipeline" {
        return Complexity::Medium;
    }

    // Command substitution
    if source.contains("$(") || source.contains('`') {
        return Complexity::Medium;
    }

    // Parameter expansion with modifiers
    if source.contains("${") {
        return Complexity::Medium;
    }

    // Redirections
    if source.contains(">&") || source.contains("<(") || source.contains(">(") {
        return Complexity::Medium;
    }

    // Process substitution
    if source.contains("<(") || source.contains(">(") {
        return Complexity::High;
    }

    // Simple assignments with arithmetic
    if source.contains("$((") {
        return Complexity::Medium;
    }

    Complexity::Low
}

/// Pacing delay in milliseconds based on complexity.
pub fn pacing_ms(complexity: Complexity) -> u64 {
    match complexity {
        Complexity::Low => 400,
        Complexity::Medium => 1200,
        Complexity::High => 2500,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_echo_is_low() {
        assert_eq!(score("SimpleCommand", "echo hello"), Complexity::Low);
    }

    #[test]
    fn pipeline_is_medium() {
        assert_eq!(score("Pipeline", "ls | grep foo"), Complexity::Medium);
    }

    #[test]
    fn multi_pipeline_is_high() {
        assert_eq!(
            score("Pipeline", "cat file | grep pat | sort | uniq"),
            Complexity::High
        );
    }

    #[test]
    fn if_statement_is_high() {
        assert_eq!(score("If", "if [ -f foo ]; then"), Complexity::High);
    }

    #[test]
    fn command_sub_is_medium() {
        assert_eq!(
            score("SimpleCommand", "echo $(date)"),
            Complexity::Medium
        );
    }

    #[test]
    fn param_expansion_is_medium() {
        assert_eq!(
            score("SimpleCommand", "echo ${var:-default}"),
            Complexity::Medium
        );
    }
}
```

- [ ] **Step 2: Implement explain.rs**

```rust
//! Human-readable explanations for shell constructs.
//! Used in X-Ray teach mode for first-occurrence concept boxes.

use std::collections::HashSet;

/// Generates first-occurrence explanations for shell concepts.
pub struct ConceptExplainer {
    seen: HashSet<String>,
    teach_mode: bool,
}

impl ConceptExplainer {
    pub fn new(teach_mode: bool) -> Self {
        Self {
            seen: HashSet::new(),
            teach_mode,
        }
    }

    /// Try to explain an expansion phase. Returns None if already explained
    /// or teach mode is off.
    pub fn explain_expansion(&mut self, phase: &str, input: &str) -> Option<(String, String)> {
        if !self.teach_mode {
            return None;
        }

        let concept = self.classify_expansion(phase, input)?;
        if !self.seen.insert(concept.clone()) {
            return None;
        }

        let explanation = self.expansion_explanation(&concept, input);
        Some((concept, explanation))
    }

    /// Brief inline explanation (always shown, not just teach mode).
    pub fn brief_expansion(phase: &str, input: &str, output: &str) -> String {
        match phase {
            "parameter" => {
                if input.contains(":-") {
                    format!("{input} — use default if unset → \"{output}\"")
                } else if input.contains(":=") {
                    format!("{input} — assign default if unset → \"{output}\"")
                } else if input.contains(":+") {
                    format!("{input} — alternate if set → \"{output}\"")
                } else if input.contains("##") || input.contains("%%") {
                    format!("{input} — pattern strip → \"{output}\"")
                } else {
                    format!("{input} → \"{output}\"")
                }
            }
            "command_sub" => format!("$({input}) captured → \"{output}\""),
            "arithmetic" => format!("$(({input})) = {output}"),
            "tilde" => format!("{input} → {output}"),
            "pathname" => format!("glob {input} matched → {output}"),
            _ => format!("{input} → {output}"),
        }
    }

    /// Explain a control flow decision.
    pub fn brief_control_flow(kind: &str, decision: &str) -> String {
        match kind {
            "if" => format!("condition is {decision}"),
            "while" | "until" => format!("loop condition: {decision}"),
            "for" => format!("iterating: {decision}"),
            "case" => format!("matching pattern: {decision}"),
            _ => decision.to_string(),
        }
    }

    fn classify_expansion(&self, phase: &str, input: &str) -> Option<String> {
        match phase {
            "parameter" => {
                if input.contains(":-") {
                    Some("parameter_default".to_string())
                } else if input.contains(":=") {
                    Some("parameter_assign".to_string())
                } else if input.contains(":+") {
                    Some("parameter_alternate".to_string())
                } else if input.contains(":?") {
                    Some("parameter_error".to_string())
                } else if input.contains("##") || input.contains("#") {
                    Some("parameter_prefix_strip".to_string())
                } else if input.contains("%%") || input.contains('%') {
                    Some("parameter_suffix_strip".to_string())
                } else if input.contains("//") {
                    Some("parameter_replace".to_string())
                } else {
                    Some("parameter_simple".to_string())
                }
            }
            "command_sub" => Some("command_substitution".to_string()),
            "arithmetic" => Some("arithmetic_expansion".to_string()),
            "tilde" => Some("tilde_expansion".to_string()),
            "pathname" => Some("pathname_expansion".to_string()),
            "brace" => Some("brace_expansion".to_string()),
            _ => None,
        }
    }

    fn expansion_explanation(&self, concept: &str, input: &str) -> String {
        match concept {
            "parameter_default" => format!(
                "${{var:-word}} is parameter expansion with default.\n\
                 If the variable is unset or null, the word after :- is used.\n\
                 Here: {input}"
            ),
            "parameter_assign" => format!(
                "${{var:=word}} assigns a default if the variable is unset.\n\
                 Unlike :-, this also sets the variable for later use.\n\
                 Here: {input}"
            ),
            "parameter_alternate" => format!(
                "${{var:+word}} uses the word only if the variable IS set.\n\
                 Useful for conditional flags: ${{verbose:+--verbose}}\n\
                 Here: {input}"
            ),
            "command_substitution" => format!(
                "$(...) runs a command and captures its stdout.\n\
                 The output replaces the $() in the parent command.\n\
                 Here: $({input})"
            ),
            "arithmetic_expansion" => format!(
                "$((...)) evaluates arithmetic expressions.\n\
                 Supports +, -, *, /, %, ** and C-style operators.\n\
                 Here: $(({input}))"
            ),
            "tilde_expansion" => "~ expands to $HOME. ~user expands to that user's home directory.".to_string(),
            "pathname_expansion" => format!(
                "Glob patterns match filenames:\n\
                 * = any characters, ? = one character, [...] = character class\n\
                 Here: {input}"
            ),
            _ => format!("Shell expansion: {input}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teach_mode_explains_first_occurrence() {
        let mut exp = ConceptExplainer::new(true);
        let result = exp.explain_expansion("parameter", "${x:-default}");
        assert!(result.is_some());
        let (concept, _) = result.unwrap();
        assert_eq!(concept, "parameter_default");
    }

    #[test]
    fn second_occurrence_returns_none() {
        let mut exp = ConceptExplainer::new(true);
        exp.explain_expansion("parameter", "${x:-default}");
        let result = exp.explain_expansion("parameter", "${y:-other}");
        assert!(result.is_none());
    }

    #[test]
    fn teach_mode_off_returns_none() {
        let mut exp = ConceptExplainer::new(false);
        let result = exp.explain_expansion("parameter", "${x:-default}");
        assert!(result.is_none());
    }

    #[test]
    fn brief_expansion_formats() {
        let result = ConceptExplainer::brief_expansion("parameter", "${x:-default}", "default");
        assert!(result.contains("use default if unset"));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p colony-debug`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-debug/src/complexity.rs crates/colony-debug/src/explain.rs
git commit -m "feat(F071): ComplexityScorer for X-Ray pacing + ConceptExplainer for teach mode"
```

---

## Chunk 3: Split-Pane TUI

### Task 4: Debug TUI with source pane and info pane

**Files:**
- Modify: `crates/colony-debug/src/tui/mod.rs`
- Modify: `crates/colony-debug/src/tui/source_pane.rs`
- Modify: `crates/colony-debug/src/tui/info_pane.rs`

- [ ] **Step 1: Implement source_pane.rs**

Left pane: script source with line numbers, current line highlight, breakpoint markers.

```rust
//! Source pane — displays script source with line markers.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use std::collections::HashSet;

pub struct SourcePane {
    lines: Vec<String>,
    current_line: u32,
    breakpoint_lines: HashSet<u32>,
    scroll_offset: u32,
}

impl SourcePane {
    pub fn new(source: &str) -> Self {
        Self {
            lines: source.lines().map(String::from).collect(),
            current_line: 0,
            breakpoint_lines: HashSet::new(),
            scroll_offset: 0,
        }
    }

    pub fn set_current_line(&mut self, line: u32) {
        self.current_line = line;
        // Auto-scroll to keep current line visible
        let visible_height = 20u32; // Updated during render
        if line > self.scroll_offset + visible_height.saturating_sub(3) {
            self.scroll_offset = line.saturating_sub(visible_height / 2);
        } else if line < self.scroll_offset + 2 {
            self.scroll_offset = line.saturating_sub(2);
        }
    }

    pub fn set_breakpoints(&mut self, lines: HashSet<u32>) {
        self.breakpoint_lines = lines;
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let visible_height = area.height.saturating_sub(2) as u32;
        let start = self.scroll_offset as usize;
        let end = (start + visible_height as usize).min(self.lines.len());

        let mut text_lines: Vec<Line> = Vec::new();

        for (idx, line_content) in self.lines[start..end].iter().enumerate() {
            let line_num = start + idx + 1;
            let is_current = line_num as u32 == self.current_line;
            let is_breakpoint = self.breakpoint_lines.contains(&(line_num as u32));

            let marker = if is_current {
                "▸"
            } else if is_breakpoint {
                "●"
            } else {
                " "
            };

            let marker_style = if is_breakpoint {
                Style::default().fg(Color::Red)
            } else if is_current {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let num_style = Style::default().fg(Color::DarkGray);
            let content_style = if is_current {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            text_lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), marker_style),
                Span::styled(format!("{line_num:>4} "), num_style),
                Span::styled(line_content.as_str(), content_style),
            ]));
        }

        let block = Block::default()
            .title(" Source ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let paragraph = Paragraph::new(text_lines).block(block);
        frame.render_widget(paragraph, area);
    }
}
```

- [ ] **Step 2: Implement info_pane.rs**

Right pane: annotations in X-Ray mode, variables in Debug mode.

```rust
//! Info pane — annotations (X-Ray) or variable inspector (Debug).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::collections::HashMap;

use crate::controller::DebugMode;
use crate::events::TraceEvent;

/// A rendered section in the info pane.
struct InfoSection {
    title: String,
    lines: Vec<Line<'static>>,
}

pub struct InfoPane {
    sections: Vec<InfoSection>,
    mode: DebugMode,
}

impl InfoPane {
    pub fn new(mode: DebugMode) -> Self {
        Self {
            sections: Vec::new(),
            mode,
        }
    }

    pub fn set_mode(&mut self, mode: DebugMode) {
        self.mode = mode;
    }

    /// Clear for a new statement.
    pub fn clear(&mut self) {
        self.sections.clear();
    }

    /// Add a trace event as a rendered section.
    pub fn push_event(&mut self, event: &TraceEvent) {
        match event {
            TraceEvent::Parse { node_kind, summary, .. } => {
                self.sections.push(InfoSection {
                    title: "PARSE".to_string(),
                    lines: vec![
                        Line::from(Span::styled(
                            format!("{node_kind}: {summary}"),
                            Style::default().fg(Color::Cyan),
                        )),
                    ],
                });
            }
            TraceEvent::Expand { phase, input, output, explanation, .. } => {
                self.sections.push(InfoSection {
                    title: format!("EXPAND ({phase})"),
                    lines: vec![
                        Line::from(vec![
                            Span::styled(input.as_str(), Style::default().fg(Color::Yellow)),
                            Span::raw(" → "),
                            Span::styled(format!("\"{output}\""), Style::default().fg(Color::Green)),
                        ]),
                        Line::from(Span::styled(
                            explanation.as_str(),
                            Style::default().fg(Color::DarkGray).italic(),
                        )),
                    ],
                });
            }
            TraceEvent::Assign { name, value, .. } => {
                self.sections.push(InfoSection {
                    title: "ASSIGN".to_string(),
                    lines: vec![Line::from(vec![
                        Span::styled(name.as_str(), Style::default().fg(Color::Magenta)),
                        Span::raw("="),
                        Span::styled(format!("\"{value}\""), Style::default().fg(Color::Green)),
                    ])],
                });
            }
            TraceEvent::Execute { command, resolved_path, args, .. } => {
                self.sections.push(InfoSection {
                    title: "EXECUTE".to_string(),
                    lines: vec![
                        Line::from(Span::styled(
                            format!("{command} → {resolved_path}"),
                            Style::default().fg(Color::Blue),
                        )),
                        Line::from(Span::styled(
                            format!("args: {:?}", args),
                            Style::default().fg(Color::DarkGray),
                        )),
                    ],
                });
            }
            TraceEvent::ControlFlow { kind, decision, detail, .. } => {
                self.sections.push(InfoSection {
                    title: format!("FLOW ({kind})"),
                    lines: vec![
                        Line::from(Span::styled(
                            decision.as_str(),
                            Style::default().fg(Color::Yellow),
                        )),
                        Line::from(Span::styled(
                            detail.as_str(),
                            Style::default().fg(Color::DarkGray),
                        )),
                    ],
                });
            }
            TraceEvent::Concept { name, explanation } => {
                self.sections.push(InfoSection {
                    title: format!("CONCEPT: {name}"),
                    lines: explanation
                        .lines()
                        .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::White))))
                        .collect(),
                });
            }
            TraceEvent::BreakpointHit { id, line, .. } => {
                self.sections.push(InfoSection {
                    title: "BREAKPOINT".to_string(),
                    lines: vec![Line::from(Span::styled(
                        format!("Hit breakpoint #{id} at line {line}"),
                        Style::default().fg(Color::Red).bold(),
                    ))],
                });
            }
            _ => {}
        }
    }

    /// Render the variables panel (Debug mode).
    pub fn push_variables(&mut self, variables: &HashMap<String, String>) {
        if variables.is_empty() {
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut sorted: Vec<_> = variables.iter().collect();
        sorted.sort_by_key(|(k, _)| k.as_str());
        for (name, value) in sorted {
            lines.push(Line::from(vec![
                Span::styled(name.clone(), Style::default().fg(Color::Magenta)),
                Span::raw("="),
                Span::styled(format!("\"{value}\""), Style::default().fg(Color::Green)),
            ]));
        }
        self.sections.push(InfoSection {
            title: "VARIABLES".to_string(),
            lines,
        });
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mut all_lines: Vec<Line> = Vec::new();

        for section in &self.sections {
            all_lines.push(Line::from(Span::styled(
                format!("─ {} ─", section.title),
                Style::default().fg(Color::Cyan).bold(),
            )));
            all_lines.extend(section.lines.iter().cloned());
            all_lines.push(Line::raw(""));
        }

        if all_lines.is_empty() {
            all_lines.push(Line::from(Span::styled(
                match self.mode {
                    DebugMode::XRay => "Waiting for execution...",
                    DebugMode::Debug => "Paused. Press [s]tep or [c]ontinue.",
                },
                Style::default().fg(Color::DarkGray).italic(),
            )));
        }

        let title = match self.mode {
            DebugMode::XRay => " X-Ray ",
            DebugMode::Debug => " Debug ",
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let paragraph = Paragraph::new(all_lines).block(block).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }
}
```

- [ ] **Step 3: Implement tui/mod.rs — main TUI event loop**

```rust
//! Debug TUI — split-pane interface for debug and X-Ray modes.

pub mod info_pane;
pub mod source_pane;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::complexity;
use crate::controller::{DebugCommand, DebugMode};
use crate::events::TraceEvent;

use info_pane::InfoPane;
use source_pane::SourcePane;

/// Run the debug TUI. Blocks until the user quits.
pub async fn run(
    source: &str,
    mode: DebugMode,
    mut event_rx: mpsc::UnboundedReceiver<TraceEvent>,
    command_tx: mpsc::UnboundedSender<DebugCommand>,
) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let mut source_pane = SourcePane::new(source);
    let mut info_pane = InfoPane::new(mode);

    loop {
        // Render
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(frame.area());

            let main_and_status = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(frame.area());

            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(main_and_status[0]);

            source_pane.render(frame, panes[0]);
            info_pane.render(frame, panes[1]);

            // Status bar
            let status = match mode {
                DebugMode::XRay => "[space] pause  [enter] step  [d] depth  [q] quit",
                DebugMode::Debug => "[s] step  [n] next  [c] continue  [b] breakpoint  [q] quit",
            };
            let status_widget = Paragraph::new(status)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(status_widget, main_and_status[1]);
        })?;

        // Handle input (non-blocking)
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => {
                        let _ = command_tx.send(DebugCommand::Quit);
                        break;
                    }
                    KeyCode::Char(' ') => {
                        let _ = command_tx.send(DebugCommand::Pause);
                    }
                    KeyCode::Enter | KeyCode::Char('s') => {
                        let _ = command_tx.send(DebugCommand::StepInto);
                    }
                    KeyCode::Char('n') => {
                        let _ = command_tx.send(DebugCommand::StepOver);
                    }
                    KeyCode::Char('c') => {
                        let _ = command_tx.send(DebugCommand::Continue);
                    }
                    KeyCode::Char('o') => {
                        let _ = command_tx.send(DebugCommand::StepOut);
                    }
                    _ => {}
                }
            }
        }

        // Consume trace events
        while let Ok(event) = event_rx.try_recv() {
            match &event {
                TraceEvent::LineEnter { line, .. } => {
                    source_pane.set_current_line(*line);
                    info_pane.clear();
                }
                _ => {}
            }
            info_pane.push_event(&event);
        }
    }

    ratatui::restore();
    Ok(())
}
```

- [ ] **Step 4: Run tests and verify compile**

Run: `cargo test -p colony-debug`
Run: `cargo check -p colony-debug`

- [ ] **Step 5: Commit**

```bash
git add crates/colony-debug/src/tui/
git commit -m "feat(F071): split-pane debug TUI with source pane, info pane, keyboard controls"
```

---

## Chunk 4: Executor Integration

### Task 5: Hook DebugController into the executor

**Files:**
- Modify: `crates/colony-exec/Cargo.toml` — add `colony-debug` dependency
- Modify: `crates/colony-exec/src/lib.rs` — add debug_controller field, hook into execute_statement_recursive

- [ ] **Step 1: Add dependency**

In `crates/colony-exec/Cargo.toml`:
```toml
colony-debug = { workspace = true }
```

- [ ] **Step 2: Add debug_controller to Executor**

In the Executor struct definition, add:
```rust
pub debug_controller: Option<std::sync::Arc<tokio::sync::Mutex<colony_debug::DebugController>>>,
```

Initialize as `None` in the constructor.

- [ ] **Step 3: Hook into execute_statement_recursive**

At the top of `execute_statement_recursive` (lib.rs:3723), before the match dispatch:

```rust
// Debug/X-Ray hook
if let Some(ref debug) = self.debug_controller {
    let mut ctrl = debug.lock().await;
    let source = format!("{}", stmt); // or reconstruct from span
    let line = stmt.span().start.line;
    let should_proceed = ctrl.on_statement_enter(line, &source);
    if !should_proceed {
        ctrl.wait_for_command().await;
    }
}
```

At the end, after the result:
```rust
if let Some(ref debug) = self.debug_controller {
    let mut ctrl = debug.lock().await;
    ctrl.on_statement_exit(stmt.span().start.line, elapsed.as_micros() as u64);
}
```

The exact integration depends on the function structure. The key principle: wrap the existing dispatch with enter/exit hooks, without changing the dispatch logic itself.

- [ ] **Step 4: Add --debug and --xray CLI flags**

In `crates/colony-shell/src/main.rs`, add command-line argument parsing for:
- `--debug <script>` — start in Debug mode
- `--xray <script>` or `--tutorial <script>` — start in X-Ray mode
- `--teach` — enable teach mode (first-occurrence explanations)

When these flags are present:
1. Read the script source
2. Create tokio channels (event_tx/rx, command_tx/rx)
3. Create DebugController with the chosen mode
4. Spawn the TUI on a separate tokio task
5. Run the executor with the debug_controller set
6. Wait for both to complete

- [ ] **Step 5: Build and test**

Run: `cargo build -p colony-shell`
Run: `cargo test -p colony-debug`

- [ ] **Step 6: Commit**

```bash
git add crates/colony-exec/ crates/colony-shell/
git commit -m "feat(F071): hook DebugController into executor + --debug/--xray CLI flags"
```

- [ ] **Step 7: Update features.json**

Set F071 status to `"in_progress"` with note about core infrastructure completion.
