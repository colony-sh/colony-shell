//! Debug controller -- state machine for debug and X-Ray modes.
//!
//! The controller sits between the executor and the TUI. The executor
//! calls `on_statement_enter` / `on_statement_exit` at each AST node.
//! The controller emits `TraceEvent`s to the TUI and blocks when paused,
//! waiting for `DebugCommand`s from the keyboard handler.

use std::collections::{HashMap, HashSet};

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
    /// Execution is proceeding normally (or auto-continuing in X-Ray).
    Running,
    /// Paused, waiting for user command.
    Paused,
    /// Step into: pause at the very next statement.
    StepInto,
    /// Step over: pause when we return to the same or shallower depth.
    StepOver { depth: u32 },
    /// Step out: pause when we return to a shallower depth.
    StepOut { depth: u32 },
    /// Execution finished.
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
    /// Breakpoint manager for line-based breakpoints.
    pub breakpoints: BreakpointManager,
    event_tx: mpsc::UnboundedSender<TraceEvent>,
    command_rx: mpsc::UnboundedReceiver<DebugCommand>,
    current_line: u32,
    call_depth: u32,
    variables: HashMap<String, String>,
    teach_seen: HashSet<String>,
}

impl DebugController {
    /// Create a new controller with the given mode and channels.
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
            teach_seen: HashSet::new(),
        }
    }

    /// Current operating mode.
    pub fn mode(&self) -> DebugMode {
        self.mode
    }

    /// Current execution state.
    pub fn state(&self) -> ExecState {
        self.state
    }

    /// Line number of the currently executing statement.
    pub fn current_line(&self) -> u32 {
        self.current_line
    }

    /// Current call depth (incremented on function/subshell entry).
    pub fn depth(&self) -> u32 {
        self.call_depth
    }

    /// Emit a trace event to the TUI.
    pub fn emit(&self, event: TraceEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Called by the executor before each statement.
    ///
    /// Returns `true` if execution should proceed immediately,
    /// `false` if the controller is paused and the caller should
    /// call `wait_for_command()`.
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

    /// Called when entering a function or subshell.
    pub fn push_frame(&mut self) {
        self.call_depth += 1;
    }

    /// Called when leaving a function or subshell.
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
    pub fn on_expand(
        &mut self,
        line: u32,
        phase: &str,
        input: &str,
        output: &str,
        explanation: &str,
    ) {
        self.emit(TraceEvent::Expand {
            line,
            phase: phase.to_string(),
            input: input.to_string(),
            output: output.to_string(),
            explanation: explanation.to_string(),
        });
    }

    /// Wait for a continue/step command from the TUI.
    ///
    /// Blocks asynchronously until the state transitions out of `Paused`.
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

    /// Current variable state for the inspector.
    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }

    /// Track whether a concept has been shown in teach mode.
    /// Returns `true` if this is the first time the concept was seen.
    pub fn mark_concept_seen(&mut self, concept: &str) -> bool {
        self.teach_seen.insert(concept.to_string())
    }

    /// Check if execution has finished.
    pub fn is_finished(&self) -> bool {
        self.state == ExecState::Finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_controller(
        mode: DebugMode,
    ) -> (
        DebugController,
        mpsc::UnboundedReceiver<TraceEvent>,
        mpsc::UnboundedSender<DebugCommand>,
    ) {
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
        let _ = rx.try_recv().expect("LineEnter event");
        let event = rx.try_recv().expect("BreakpointHit event");
        assert!(matches!(event, TraceEvent::BreakpointHit { line: 5, .. }));
    }

    #[test]
    fn step_into_pauses_next() {
        let (mut ctrl, _, _cmd_tx) = make_controller(DebugMode::Debug);
        ctrl.state = ExecState::StepInto;
        let should_proceed = ctrl.on_statement_enter(1, "x=1");
        assert!(!should_proceed);
        assert_eq!(ctrl.state(), ExecState::Paused);
    }

    #[test]
    fn on_assign_tracks_variable() {
        let (mut ctrl, _, _) = make_controller(DebugMode::XRay);
        ctrl.on_assign("foo", "bar", 1);
        assert_eq!(ctrl.variables().get("foo").map(String::as_str), Some("bar"));
    }

    #[test]
    fn frame_depth_tracking() {
        let (mut ctrl, _, _) = make_controller(DebugMode::XRay);
        assert_eq!(ctrl.depth(), 0);
        ctrl.push_frame();
        assert_eq!(ctrl.depth(), 1);
        ctrl.push_frame();
        assert_eq!(ctrl.depth(), 2);
        ctrl.pop_frame();
        assert_eq!(ctrl.depth(), 1);
        ctrl.pop_frame();
        assert_eq!(ctrl.depth(), 0);
        // Saturates at zero
        ctrl.pop_frame();
        assert_eq!(ctrl.depth(), 0);
    }

    #[test]
    fn step_over_at_depth() {
        let (mut ctrl, _, _) = make_controller(DebugMode::Debug);
        ctrl.state = ExecState::Running;
        // Set step over at depth 0
        ctrl.state = ExecState::StepOver { depth: 0 };
        // At depth 0: should pause
        let should_proceed = ctrl.on_statement_enter(1, "echo a");
        assert!(!should_proceed);
    }

    #[test]
    fn step_over_skips_deeper() {
        let (mut ctrl, _, _) = make_controller(DebugMode::Debug);
        ctrl.state = ExecState::StepOver { depth: 0 };
        ctrl.push_frame(); // now at depth 1
        // Should continue because depth > step_over depth
        let should_proceed = ctrl.on_statement_enter(10, "inner cmd");
        assert!(should_proceed);
    }

    #[test]
    fn step_out_pauses_at_shallower() {
        let (mut ctrl, _, _) = make_controller(DebugMode::Debug);
        ctrl.push_frame(); // depth = 1
        ctrl.state = ExecState::StepOut { depth: 1 };
        ctrl.pop_frame(); // depth = 0
        // At depth 0 < step_out depth 1: should pause
        let should_proceed = ctrl.on_statement_enter(1, "echo done");
        assert!(!should_proceed);
    }

    #[test]
    fn handle_continue_command() {
        let (mut ctrl, _, _) = make_controller(DebugMode::Debug);
        assert_eq!(ctrl.state(), ExecState::Paused);
        ctrl.handle_command(DebugCommand::Continue);
        assert_eq!(ctrl.state(), ExecState::Running);
    }

    #[test]
    fn handle_toggle_mode() {
        let (mut ctrl, _, _) = make_controller(DebugMode::Debug);
        assert_eq!(ctrl.mode(), DebugMode::Debug);
        ctrl.handle_command(DebugCommand::ToggleMode);
        assert_eq!(ctrl.mode(), DebugMode::XRay);
        ctrl.handle_command(DebugCommand::ToggleMode);
        assert_eq!(ctrl.mode(), DebugMode::Debug);
    }

    #[test]
    fn handle_quit_finishes() {
        let (mut ctrl, _, _) = make_controller(DebugMode::Debug);
        ctrl.handle_command(DebugCommand::Quit);
        assert!(ctrl.is_finished());
    }

    #[tokio::test]
    async fn wait_for_command_unblocks_on_continue() {
        let (mut ctrl, _, cmd_tx) = make_controller(DebugMode::Debug);
        assert_eq!(ctrl.state(), ExecState::Paused);
        cmd_tx.send(DebugCommand::Continue).expect("send succeeds");
        ctrl.wait_for_command().await;
        assert_eq!(ctrl.state(), ExecState::Running);
    }

    #[tokio::test]
    async fn wait_for_command_finishes_on_channel_close() {
        let (mut ctrl, _, cmd_tx) = make_controller(DebugMode::Debug);
        drop(cmd_tx);
        ctrl.wait_for_command().await;
        assert!(ctrl.is_finished());
    }

    #[test]
    fn mark_concept_seen_returns_true_first_time() {
        let (mut ctrl, _, _) = make_controller(DebugMode::XRay);
        assert!(ctrl.mark_concept_seen("parameter_default"));
        assert!(!ctrl.mark_concept_seen("parameter_default"));
    }

    #[test]
    fn on_expand_emits_event() {
        let (mut ctrl, mut rx, _) = make_controller(DebugMode::XRay);
        ctrl.on_expand(3, "parameter", "${x:-val}", "val", "default used");
        let event = rx.try_recv().expect("should have event");
        assert!(matches!(event, TraceEvent::Expand { line: 3, .. }));
    }

    #[test]
    fn on_statement_exit_emits_line_exit() {
        let (mut ctrl, mut rx, _) = make_controller(DebugMode::XRay);
        ctrl.on_statement_exit(5, 1234);
        let event = rx.try_recv().expect("should have event");
        assert!(matches!(event, TraceEvent::LineExit { line: 5, duration_us: 1234 }));
    }

    #[test]
    fn running_xray_proceeds_without_breakpoints() {
        let (mut ctrl, _, _) = make_controller(DebugMode::XRay);
        let should_proceed = ctrl.on_statement_enter(1, "echo start");
        assert!(should_proceed);
        let should_proceed = ctrl.on_statement_enter(2, "echo next");
        assert!(should_proceed);
    }

    #[test]
    fn finished_state_blocks() {
        let (mut ctrl, _, _) = make_controller(DebugMode::Debug);
        ctrl.state = ExecState::Finished;
        let should_proceed = ctrl.on_statement_enter(1, "echo done");
        assert!(!should_proceed);
    }
}
