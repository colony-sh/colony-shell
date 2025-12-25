//! Tutorial Runner - Main orchestrator for "Script X-Ray" mode
//!
//! This module connects the TUI, tracer, and executor to provide
//! an interactive tutorial experience for any shell script.

use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use colony_exec::Executor;
use colony_expand::ExpansionContext;
use colony_parser::pest_api::parse;
use colony_parser::{Statement, Word, WordPart};

use crate::tutorial::complexity::ComplexityScorer;
use crate::tutorial::events::{BranchType, ComplexityLevel, ExpansionPhase, LoopType, TraceEvent};
use crate::tutorial::tracer::{TraceDepth, TracerConfig};
use crate::tutorial::tui::TutorialTui;

/// Configuration for tutorial mode
#[derive(Debug, Clone)]
pub struct TutorialConfig {
    /// Trace depth (Level 2 = mechanics, Level 3 = internals)
    pub depth: TraceDepth,
    /// Enable teach mode (explain concepts on first occurrence)
    pub teach_mode: bool,
    /// Auto-advance speed multiplier (1.0 = normal)
    pub speed: f32,
    /// Pause on high complexity statements
    pub pause_on_complex: bool,
}

impl Default for TutorialConfig {
    fn default() -> Self {
        Self {
            depth: TraceDepth::Mechanics,
            teach_mode: false,
            speed: 1.0,
            pause_on_complex: true,
        }
    }
}

/// Result type for tutorial runner
pub type TutorialResult<T> = Result<T, TutorialError>;

/// Errors that can occur during tutorial execution
#[derive(Debug)]
pub enum TutorialError {
    /// IO error (terminal, file)
    Io(io::Error),
    /// Script parsing error
    Parse(String),
    /// Execution error
    Execution(String),
}

impl std::fmt::Display for TutorialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Parse(e) => write!(f, "Parse error: {}", e),
            Self::Execution(e) => write!(f, "Execution error: {}", e),
        }
    }
}

impl std::error::Error for TutorialError {}

impl From<io::Error> for TutorialError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Main tutorial runner
///
/// Orchestrates the TUI, tracer, and executor to provide an interactive
/// tutorial experience for shell scripts.
pub struct TutorialRunner {
    /// Path to the script being analyzed
    script_path: std::path::PathBuf,
    /// Script source code
    source: String,
    /// Tutorial configuration
    config: TutorialConfig,
}

impl TutorialRunner {
    /// Create a new tutorial runner for a script
    pub fn new(script_path: &Path, config: TutorialConfig) -> TutorialResult<Self> {
        let source = std::fs::read_to_string(script_path)?;
        
        Ok(Self {
            script_path: script_path.to_path_buf(),
            source,
            config,
        })
    }

    /// Run the tutorial mode
    ///
    /// This sets up the terminal, creates the TUI, and runs the event loop.
    pub fn run(&mut self) -> TutorialResult<i32> {
        // Set up terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Create TUI
        let tracer_config = TracerConfig {
            depth: self.config.depth,
            teach_mode: self.config.teach_mode,
            marked_lines: Default::default(),
        };
        
        let mut tui = TutorialTui::new(&self.script_path)?
            .with_config(&tracer_config);

        // Create trace channel for communication between executor and TUI
        let (tx, rx): (Sender<TraceEvent>, Receiver<TraceEvent>) = mpsc::channel();

        // Parse script to validate and get line count
        let program = parse(&self.source)
            .map_err(|e| TutorialError::Parse(e.to_string()))?;
        
        let line_count = self.source.lines().count() as u32;

        // Emit script start
        tx.send(TraceEvent::ScriptStart {
            path: self.script_path.clone(),
            lines: line_count,
        }).ok();

        // Run event loop
        let result = self.event_loop(&mut terminal, &mut tui, &rx, &tx, &program);

        // Restore terminal
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    /// Generate a human-readable AST summary for a statement
    fn summarize_statement(stmt: &Statement) -> String {
        match stmt {
            Statement::SimpleCommand(cmd) => {
                let name = cmd.name.as_literal().unwrap_or("<dynamic>");
                let arg_count = cmd.args.len();
                let redirect_count = cmd.redirects.len();
                let assign_count = cmd.env_assignments.len();
                
                let mut parts = vec![format!("Command: {}", name)];
                if arg_count > 0 {
                    parts.push(format!("{} arg(s)", arg_count));
                }
                if redirect_count > 0 {
                    parts.push(format!("{} redirect(s)", redirect_count));
                }
                if assign_count > 0 {
                    parts.push(format!("{} assignment(s)", assign_count));
                }
                parts.join(", ")
            }
            Statement::Pipeline(pipeline) => {
                let count = pipeline.elements.len();
                format!("Pipeline: {} command(s)", count)
            }
            Statement::List(list) => {
                let ops: Vec<&str> = list.rest.iter().map(|(op, _)| {
                    match op {
                        colony_parser::ListOp::Semi => ";",
                        colony_parser::ListOp::And => "&&",
                        colony_parser::ListOp::Or => "||",
                        colony_parser::ListOp::Background => "&",
                    }
                }).collect();
                format!("Command list with operators: {}", ops.join(", "))
            }
            Statement::If(_) => "If statement".to_string(),
            Statement::For(for_stmt) => {
                format!("For loop: {} in ...", for_stmt.var)
            }
            Statement::While(_) => "While loop".to_string(),
            Statement::Until(_) => "Until loop".to_string(),
            Statement::Case(_) => "Case statement".to_string(),
            Statement::FunctionDef(f) => {
                format!("Function definition: {}", f.name)
            }
            Statement::Subshell(_) => "Subshell (...)".to_string(),
            Statement::BraceGroup(_) => "Brace group {{ ... }}".to_string(),
            Statement::Coproc(c) => {
                let name = c.name.as_deref().unwrap_or("COPROC");
                format!("Coprocess: {}", name)
            }
            Statement::Return(r) => {
                if r.value.is_some() {
                    "Return with value".to_string()
                } else {
                    "Return".to_string()
                }
            }
            Statement::Empty => "Empty statement".to_string(),
            _ => "Statement".to_string(),
        }
    }

    /// Emit trace events for expansions in a word
    fn emit_word_expansions(word: &Word, line: u32, tx: &Sender<TraceEvent>) {
        for part in &word.parts {
            Self::emit_word_part_expansion(part, line, tx);
        }
    }

    /// Emit trace event for a word part expansion
    fn emit_word_part_expansion(part: &WordPart, line: u32, tx: &Sender<TraceEvent>) {
        use colony_parser::VarModifier;
        
        match part {
            WordPart::Variable(var_ref) => {
                let input = if var_ref.modifier.is_some() {
                    format!("${{{}:...}}", var_ref.name)
                } else if var_ref.prefix.is_some() {
                    format!("${{#{}}}", var_ref.name)
                } else {
                    format!("${}", var_ref.name)
                };
                tx.send(TraceEvent::Expand {
                    line,
                    phase: ExpansionPhase::Parameter,
                    input,
                    output: "<expanded at runtime>".to_string(),
                    explanation: Self::explain_variable(var_ref),
                }).ok();
            }
            WordPart::CommandSubRaw(cmd) => {
                tx.send(TraceEvent::Expand {
                    line,
                    phase: ExpansionPhase::Command,
                    input: format!("$({})", cmd),
                    output: "<executed at runtime>".to_string(),
                    explanation: "Executes command and substitutes its stdout".to_string(),
                }).ok();
            }
            WordPart::ArithRaw(expr) => {
                tx.send(TraceEvent::Expand {
                    line,
                    phase: ExpansionPhase::Arithmetic,
                    input: format!("$(({}))", expr),
                    output: "<computed at runtime>".to_string(),
                    explanation: "Evaluates arithmetic expression".to_string(),
                }).ok();
            }
            WordPart::Tilde(user) => {
                let input = if let Some(u) = user {
                    format!("~{}", u)
                } else {
                    "~".to_string()
                };
                tx.send(TraceEvent::Expand {
                    line,
                    phase: ExpansionPhase::Tilde,
                    input,
                    output: "<home directory>".to_string(),
                    explanation: "Expands to home directory".to_string(),
                }).ok();
            }
            WordPart::DoubleQuoted(parts) => {
                for p in parts {
                    Self::emit_word_part_expansion(p, line, tx);
                }
            }
            WordPart::ProcessSub { direction, command } => {
                let dir_str = if *direction == colony_parser::ProcessSubDirection::Input {
                    "<"
                } else {
                    ">"
                };
                tx.send(TraceEvent::Expand {
                    line,
                    phase: ExpansionPhase::Command,
                    input: format!("{}({})", dir_str, command),
                    output: "<process substitution>".to_string(),
                    explanation: "Creates a file descriptor for process I/O".to_string(),
                }).ok();
            }
            _ => {}
        }
    }

    /// Generate explanation for a variable reference
    fn explain_variable(var_ref: &colony_parser::VariableRef) -> String {
        use colony_parser::VarModifier;
        
        if let Some(ref modifier) = var_ref.modifier {
            match modifier {
                VarModifier::Default { check_null, .. } => {
                    let op = if *check_null { ":-" } else { "-" };
                    format!("Use default value if {} is unset{} ({})", var_ref.name, if *check_null { " or null" } else { "" }, op)
                }
                VarModifier::Assign { check_null, .. } => {
                    let op = if *check_null { ":=" } else { "=" };
                    format!("Assign default if {} is unset{} ({})", var_ref.name, if *check_null { " or null" } else { "" }, op)
                }
                VarModifier::Error { check_null, .. } => {
                    let op = if *check_null { ":?" } else { "?" };
                    format!("Error if {} is unset{} ({})", var_ref.name, if *check_null { " or null" } else { "" }, op)
                }
                VarModifier::Alternate { check_null, .. } => {
                    let op = if *check_null { ":+" } else { "+" };
                    format!("Use alternate if {} IS set{} ({})", var_ref.name, if *check_null { " and non-null" } else { "" }, op)
                }
                VarModifier::Length => format!("Length of {}", var_ref.name),
                VarModifier::Indices => format!("Indices of array {}", var_ref.name),
                VarModifier::Slice { .. } => format!("Substring/slice of {}", var_ref.name),
                VarModifier::StripPrefix { .. } => format!("Remove shortest prefix from {}", var_ref.name),
                VarModifier::StripPrefixGreedy { .. } => format!("Remove longest prefix from {}", var_ref.name),
                VarModifier::StripSuffix { .. } => format!("Remove shortest suffix from {}", var_ref.name),
                VarModifier::StripSuffixGreedy { .. } => format!("Remove longest suffix from {}", var_ref.name),
                VarModifier::UpperCase { all } => {
                    if *all { format!("Uppercase all of {}", var_ref.name) } else { format!("Uppercase first char of {}", var_ref.name) }
                }
                VarModifier::LowerCase { all } => {
                    if *all { format!("Lowercase all of {}", var_ref.name) } else { format!("Lowercase first char of {}", var_ref.name) }
                }
                VarModifier::ToggleCase { all } => {
                    if *all { format!("Toggle case of all in {}", var_ref.name) } else { format!("Toggle case of first char in {}", var_ref.name) }
                }
                VarModifier::Substitute { all, .. } => {
                    if *all { format!("Replace all matches in {}", var_ref.name) } else { format!("Replace first match in {}", var_ref.name) }
                }
            }
        } else if var_ref.prefix.is_some() {
            format!("Length of {}", var_ref.name)
        } else {
            format!("Value of ${}", var_ref.name)
        }
    }

    /// Emit detailed trace events for a statement before execution
    fn emit_statement_trace(&self, stmt: &Statement, line: u32, tx: &Sender<TraceEvent>) {
        // Emit parse summary
        let summary = Self::summarize_statement(stmt);
        tx.send(TraceEvent::Parse {
            line,
            ast_summary: summary,
            ast_full: None, // Could add JSON AST here for Level 3
        }).ok();

        // Emit expansion events for words in the statement
        match stmt {
            Statement::SimpleCommand(cmd) => {
                Self::emit_word_expansions(&cmd.name, line, tx);
                for arg in &cmd.args {
                    Self::emit_word_expansions(arg, line, tx);
                }
                // Emit assignment info
                for assign in &cmd.env_assignments {
                    tx.send(TraceEvent::Assign {
                        line,
                        name: assign.name.clone(),
                        value: "<assigned at runtime>".to_string(),
                        previous: None,
                    }).ok();
                }
            }
            Statement::For(for_stmt) => {
                tx.send(TraceEvent::LoopEnter {
                    line,
                    loop_type: LoopType::For,
                    variable: Some(for_stmt.var.clone()),
                    values: None, // Will be expanded at runtime
                }).ok();
            }
            Statement::While(_) => {
                tx.send(TraceEvent::LoopEnter {
                    line,
                    loop_type: LoopType::While,
                    variable: None,
                    values: None,
                }).ok();
            }
            Statement::Until(_) => {
                tx.send(TraceEvent::LoopEnter {
                    line,
                    loop_type: LoopType::Until,
                    variable: None,
                    values: None,
                }).ok();
            }
            Statement::If(_) => {
                tx.send(TraceEvent::Branch {
                    line,
                    branch_type: BranchType::If,
                    taken: "evaluating condition...".to_string(),
                    condition: "<condition>".to_string(),
                    result: false,
                }).ok();
            }
            Statement::Case(_) => {
                tx.send(TraceEvent::Branch {
                    line,
                    branch_type: BranchType::Case,
                    taken: "matching patterns...".to_string(),
                    condition: "<value>".to_string(),
                    result: false,
                }).ok();
            }
            _ => {}
        }
    }

    /// Main event loop
    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        tui: &mut TutorialTui,
        rx: &Receiver<TraceEvent>,
        tx: &Sender<TraceEvent>,
        program: &colony_parser::Program,
    ) -> TutorialResult<i32> {
        let lines: Vec<&str> = self.source.lines().collect();
        let mut current_stmt_idx = 0;
        let mut exit_code = 0;

        // Create executor context
        let context = ExpansionContext::new();
        let mut executor = Executor::new(context);

        loop {
            // Draw UI
            terminal.draw(|f| tui.render(f))?;

            // Check for quit
            if tui.should_quit() {
                break;
            }

            // Process trace events from channel
            while let Ok(event) = rx.try_recv() {
                tui.handle_event(event);
            }

            // Handle keyboard input (non-blocking with timeout)
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    // Handle Ctrl+C
                    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                        break;
                    }
                    
                    tui.process_input(Event::Key(key));
                }
            }

            // If playing or stepping, execute next statement
            if (tui.is_playing() || tui.is_stepping()) && current_stmt_idx < program.statements.len() {
                let stmt = &program.statements[current_stmt_idx];
                
                // Get line number (approximate - from statement index)
                let line_num = (current_stmt_idx + 1) as u32;
                let line_source = lines.get(current_stmt_idx)
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                // Emit line enter
                tx.send(TraceEvent::LineEnter {
                    line: line_num,
                    source: line_source.clone(),
                }).ok();

                // Emit detailed trace events for the statement
                self.emit_statement_trace(stmt, line_num, tx);

                // Compute complexity
                let complexity = ComplexityScorer::score(stmt);
                
                // Execute statement
                let result = futures::executor::block_on(async {
                    executor.execute_statement_recursive(stmt).await
                });

                match result {
                    Ok(code) => {
                        exit_code = code;
                        // Emit exit status
                        tx.send(TraceEvent::ExitStatus {
                            line: line_num,
                            code,
                            signal: None,
                        }).ok();
                    }
                    Err(e) => {
                        tx.send(TraceEvent::Error {
                            line: line_num,
                            message: e.to_string(),
                        }).ok();
                    }
                }

                // Emit line exit
                tx.send(TraceEvent::LineExit {
                    line: line_num,
                    duration_us: 0, // TODO: measure actual duration
                }).ok();

                current_stmt_idx += 1;

                // If stepping, pause after each step
                if tui.is_stepping() {
                    tui.finish_step();
                }

                // If high complexity and pause_on_complex, pause
                if self.config.pause_on_complex && ComplexityScorer::should_auto_pause(complexity) {
                    tui.finish_step();
                }
            }

            // If we've finished all statements
            if current_stmt_idx >= program.statements.len() && !tui.should_quit() {
                tx.send(TraceEvent::ScriptEnd {
                    exit_code,
                    duration_ms: 0, // TODO: measure actual duration
                }).ok();
                // Don't auto-quit, let user explore
            }
        }

        Ok(exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_default() {
        let config = TutorialConfig::default();
        assert_eq!(config.depth, TraceDepth::Mechanics);
        assert!(!config.teach_mode);
        assert_eq!(config.speed, 1.0);
        assert!(config.pause_on_complex);
    }

    #[test]
    fn test_runner_creation() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "echo hello").unwrap();
        
        let config = TutorialConfig::default();
        let runner = TutorialRunner::new(file.path(), config);
        assert!(runner.is_ok());
    }

    #[test]
    fn test_runner_invalid_path() {
        let config = TutorialConfig::default();
        let result = TutorialRunner::new(Path::new("/nonexistent/script.sh"), config);
        assert!(result.is_err());
    }
}
