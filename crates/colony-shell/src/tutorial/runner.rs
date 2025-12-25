//! Tutorial Runner - Main orchestrator for "Script X-Ray" mode
//!
//! This module connects the TUI, tracer, and executor to provide
//! an interactive tutorial experience for any shell script.

use std::collections::HashMap;
use std::io;
use std::io::{Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use nix::unistd::{dup, dup2, pipe};
use ratatui::{backend::CrosstermBackend, Terminal};

use colony_exec::Executor;
use colony_expand::{ExpansionContext, Variable};
use colony_parser::pest_api::parse;
use colony_parser::{Statement, Word, WordPart};

use crate::tutorial::complexity::ComplexityScorer;
use crate::tutorial::events::{BranchType, ComplexityLevel, ExpansionPhase, LoopType, TraceEvent};
use crate::tutorial::explain::CommandExplainer;
use crate::tutorial::teach::{Concept, ConceptTracker};
use crate::tutorial::tracer::{TraceDepth, TracerConfig};
use crate::tutorial::tui::TutorialTui;

/// Configuration for tutorial mode
#[derive(Debug, Clone)]
pub struct TutorialConfig {
    pub depth: TraceDepth,
    pub teach_mode: bool,
    pub pause_on_complex: bool,
}

impl Default for TutorialConfig {
    fn default() -> Self {
        Self {
            depth: TraceDepth::Mechanics,
            teach_mode: false,
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
    /// Tracks which concepts have been explained (for teach mode)
    concept_tracker: ConceptTracker,
    /// Provides hints for external commands
    command_explainer: CommandExplainer,
}

impl TutorialRunner {
    /// Create a new tutorial runner for a script
    pub fn new(script_path: &Path, config: TutorialConfig) -> TutorialResult<Self> {
        let source = std::fs::read_to_string(script_path)?;
        
        Ok(Self {
            script_path: script_path.to_path_buf(),
            source,
            config,
            concept_tracker: ConceptTracker::new(),
            command_explainer: CommandExplainer::new(),
        })
    }

    /// Run the tutorial mode
    ///
    /// This sets up the terminal, creates the TUI, and runs the event loop.
    pub fn run(&mut self) -> TutorialResult<i32> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

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

    fn summarize_statement(stmt: &Statement) -> String {
        Self::format_statement_tree(stmt, 0)
    }
    
    fn format_statement_tree(stmt: &Statement, indent: usize) -> String {
        let prefix = "│ ".repeat(indent);
        let branch = if indent > 0 { "├─" } else { "" };
        
        match stmt {
            Statement::SimpleCommand(cmd) => {
                let mut lines = vec![format!("{}{}SimpleCommand", prefix, branch)];
                let child_prefix = format!("{}│ ", prefix);
                
                lines.push(format!("{}├─name: {}", child_prefix, Self::format_word_tree(&cmd.name, indent + 2)));
                
                for (i, arg) in cmd.args.iter().enumerate() {
                    let is_last = i == cmd.args.len() - 1 && cmd.redirects.is_empty() && cmd.env_assignments.is_empty();
                    let connector = if is_last { "└─" } else { "├─" };
                    lines.push(format!("{}{}arg[{}]: {}", child_prefix, connector, i, Self::format_word_tree(arg, indent + 2)));
                }
                
                for (i, redir) in cmd.redirects.iter().enumerate() {
                    let is_last = i == cmd.redirects.len() - 1 && cmd.env_assignments.is_empty();
                    let connector = if is_last { "└─" } else { "├─" };
                    lines.push(format!("{}{}redir: {}", child_prefix, connector, Self::format_redirect(redir)));
                }
                
                for (i, assign) in cmd.env_assignments.iter().enumerate() {
                    let is_last = i == cmd.env_assignments.len() - 1;
                    let connector = if is_last { "└─" } else { "├─" };
                    lines.push(format!("{}{}env: {}={}", child_prefix, connector, assign.name, Self::format_assignment_value(&assign.value)));
                }
                
                lines.join("\n")
            }
            Statement::Pipeline(pipeline) => {
                let mut lines = vec![format!("{}{}Pipeline", prefix, branch)];
                let child_prefix = format!("{}│ ", prefix);
                
                for (i, elem) in pipeline.elements.iter().enumerate() {
                    let is_last = i == pipeline.elements.len() - 1;
                    let connector = if is_last { "└─" } else { "├─" };
                    let pipe_op = elem.pipe_op.map(|op| match op {
                        colony_parser::PipeOp::Pipe => " |",
                        colony_parser::PipeOp::PipeErr => " |&",
                        colony_parser::PipeOp::Arrow => " ->",
                        colony_parser::PipeOp::FatArrow => " =>",
                        colony_parser::PipeOp::Alter => " ~>",
                    }).unwrap_or("");
                    lines.push(format!("{}{}[{}]{}", child_prefix, connector, i, pipe_op));
                    lines.push(Self::format_statement_tree(&elem.command, indent + 2));
                }
                
                lines.join("\n")
            }
            Statement::List(list) => {
                let mut lines = vec![format!("{}{}CommandList", prefix, branch)];
                let child_prefix = format!("{}│ ", prefix);
                
                lines.push(format!("{}├─first:", child_prefix));
                lines.push(Self::format_statement_tree(&list.first, indent + 2));
                
                for (i, (op, stmt)) in list.rest.iter().enumerate() {
                    let is_last = i == list.rest.len() - 1;
                    let connector = if is_last { "└─" } else { "├─" };
                    let op_str = match op {
                        colony_parser::ListOp::Semi => ";",
                        colony_parser::ListOp::And => "&&",
                        colony_parser::ListOp::Or => "||",
                        colony_parser::ListOp::Background => "&",
                    };
                    lines.push(format!("{}{}[{}]", child_prefix, connector, op_str));
                    lines.push(Self::format_statement_tree(stmt, indent + 2));
                }
                
                lines.join("\n")
            }
            Statement::If(if_stmt) => {
                let mut lines = vec![format!("{}{}IfStatement", prefix, branch)];
                let child_prefix = format!("{}│ ", prefix);
                lines.push(format!("{}├─condition: <test>", child_prefix));
                lines.push(format!("{}├─then: {} stmt(s)", child_prefix, if_stmt.then_body.len()));
                if !if_stmt.elif_branches.is_empty() {
                    lines.push(format!("{}├─elif: {} branch(es)", child_prefix, if_stmt.elif_branches.len()));
                }
                if let Some(ref else_body) = if_stmt.else_body {
                    lines.push(format!("{}└─else: {} stmt(s)", child_prefix, else_body.len()));
                }
                lines.join("\n")
            }
            Statement::For(for_stmt) => {
                let mut lines = vec![format!("{}{}ForLoop", prefix, branch)];
                let child_prefix = format!("{}│ ", prefix);
                lines.push(format!("{}├─var: {}", child_prefix, for_stmt.var));
                let words: Vec<String> = for_stmt.words.iter().map(|w| Self::format_word_tree(w, 0)).collect();
                lines.push(format!("{}├─in: [{}]", child_prefix, words.join(", ")));
                lines.push(format!("{}└─body: {} stmt(s)", child_prefix, for_stmt.body.len()));
                lines.join("\n")
            }
            Statement::While(w) => {
                let mut lines = vec![format!("{}{}WhileLoop", prefix, branch)];
                let child_prefix = format!("{}│ ", prefix);
                lines.push(format!("{}├─condition: <test>", child_prefix));
                lines.push(format!("{}└─body: {} stmt(s)", child_prefix, w.body.len()));
                lines.join("\n")
            }
            Statement::Until(u) => {
                let mut lines = vec![format!("{}{}UntilLoop", prefix, branch)];
                let child_prefix = format!("{}│ ", prefix);
                lines.push(format!("{}├─condition: <test>", child_prefix));
                lines.push(format!("{}└─body: {} stmt(s)", child_prefix, u.body.len()));
                lines.join("\n")
            }
            Statement::Case(c) => {
                let mut lines = vec![format!("{}{}CaseStatement", prefix, branch)];
                let child_prefix = format!("{}│ ", prefix);
                lines.push(format!("{}├─value: {}", child_prefix, Self::format_word_tree(&c.value, 0)));
                lines.push(format!("{}└─arms: {} pattern(s)", child_prefix, c.arms.len()));
                lines.join("\n")
            }
            Statement::FunctionDef(f) => {
                let mut lines = vec![format!("{}{}FunctionDef", prefix, branch)];
                let child_prefix = format!("{}│ ", prefix);
                lines.push(format!("{}├─name: {}", child_prefix, f.name));
                lines.push(format!("{}└─body: {} stmt(s)", child_prefix, f.body.len()));
                lines.join("\n")
            }
            Statement::Subshell(s) => {
                format!("{}{}Subshell ({} stmt(s))", prefix, branch, s.body.len())
            }
            Statement::BraceGroup(b) => {
                format!("{}{}BraceGroup ({} stmt(s))", prefix, branch, b.body.len())
            }
            Statement::Coproc(c) => {
                let name = c.name.as_deref().unwrap_or("COPROC");
                format!("{}{}Coproc: {}", prefix, branch, name)
            }
            Statement::Return(r) => {
                if r.value.is_some() {
                    format!("{}{}Return <value>", prefix, branch)
                } else {
                    format!("{}{}Return", prefix, branch)
                }
            }
            Statement::Empty => format!("{}{}Empty", prefix, branch),
            Statement::Conditional(_) => format!("{}{}[[ ... ]]", prefix, branch),
            Statement::Arithmetic(_) => format!("{}{}(( ... ))", prefix, branch),
            _ => format!("{}{}Statement", prefix, branch),
        }
    }
    
    fn format_word_tree(word: &Word, _indent: usize) -> String {
        if word.parts.len() == 1 {
            Self::format_word_part(&word.parts[0])
        } else {
            let parts: Vec<String> = word.parts.iter().map(|p| Self::format_word_part(p)).collect();
            format!("Word[{}]", parts.join(" + "))
        }
    }
    
    fn format_word_part(part: &WordPart) -> String {
        match part {
            WordPart::Literal(s) => format!("Lit({})", s),
            WordPart::SingleQuoted(s) => format!("SQ('{}')", s),
            WordPart::DoubleQuoted(inner) => {
                let parts: Vec<String> = inner.iter().map(|p| Self::format_word_part(p)).collect();
                format!("DQ({})", parts.join(" + "))
            }
            WordPart::Variable(var) => {
                let mut desc = format!("${}", var.name);
                if var.modifier.is_some() {
                    desc = format!("${{{}:mod}}", var.name);
                } else if var.prefix.is_some() {
                    desc = format!("${{#{}}}", var.name);
                } else if var.subscript.is_some() {
                    desc = format!("${{{}[sub]}}", var.name);
                }
                format!("Var({})", desc)
            }
            WordPart::CommandSubRaw(cmd) => format!("CmdSub({})", cmd),
            WordPart::ArithRaw(expr) => format!("Arith({})", expr),
            WordPart::Tilde(user) => {
                if let Some(u) = user {
                    format!("Tilde(~{})", u)
                } else {
                    "Tilde(~)".to_string()
                }
            }
            WordPart::ExtGlob { operator, pattern_list } => {
                let op = match operator {
                    colony_parser::ExtGlobOp::ZeroOrOne => "?",
                    colony_parser::ExtGlobOp::ZeroOrMore => "*",
                    colony_parser::ExtGlobOp::OneOrMore => "+",
                    colony_parser::ExtGlobOp::ExactlyOne => "@",
                    colony_parser::ExtGlobOp::Not => "!",
                };
                format!("ExtGlob({}({}))", op, pattern_list)
            }
            WordPart::BraceExpansion(s) => format!("Brace({{{}}})", s),
            WordPart::ProcessSub { direction, command } => {
                let op = if *direction == colony_parser::ProcessSubDirection::Input { "<" } else { ">" };
                format!("ProcSub({}({}))", op, command)
            }
            WordPart::AnsiCQuoted(s) => format!("ANSI($'{}')", s),
            WordPart::CommandSub(_) => "CmdSub(<ast>)".to_string(),
            _ => "<?>".to_string(),
        }
    }
    
    fn format_redirect(redir: &colony_parser::Redirect) -> String {
        use colony_parser::{RedirectKind, RedirectTarget, HereDocKind};
        
        let fd = redir.fd.map(|f| format!("fd{}:", f)).unwrap_or_default();
        let kind = match &redir.kind {
            RedirectKind::Output => ">",
            RedirectKind::Append => ">>",
            RedirectKind::Input => "<",
            RedirectKind::HereDoc(HereDocKind::Normal) => "<<",
            RedirectKind::HereDoc(HereDocKind::StripTabs) => "<<-",
            RedirectKind::HereDoc(HereDocKind::HereString) => "<<<",
            RedirectKind::DupFd => ">&",
            RedirectKind::DupFdInput => "<&",
            RedirectKind::ReadWrite => "<>",
            RedirectKind::Clobber => ">|",
            RedirectKind::AndOutput => "&>",
            RedirectKind::AndAppend => "&>>",
        };
        let target = match &redir.target {
            RedirectTarget::File(w) => Self::format_word_tree(w, 0),
            RedirectTarget::Fd(n) => format!("&{}", n),
            RedirectTarget::Close => "-".to_string(),
            RedirectTarget::HereDoc { content, .. } => format!("heredoc({} chars)", content.len()),
        };
        format!("{}{} {}", fd, kind, target)
    }
    
    fn format_assignment_value(val: &colony_parser::AssignmentValue) -> String {
        use colony_parser::AssignmentValue;
        match val {
            AssignmentValue::Word(w) => Self::format_word_tree(w, 0),
            AssignmentValue::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|w| Self::format_word_tree(w, 0)).collect();
                format!("Array({})", items.join(", "))
            }
            AssignmentValue::AssocArray(arr) => {
                let items: Vec<String> = arr.iter()
                    .map(|(k, v)| format!("[{}]={}", k, v))
                    .collect();
                format!("AssocArray({})", items.join(", "))
            }
        }
    }

    fn emit_word_expansions(word: &Word, line: u32, tx: &Sender<TraceEvent>, ctx: &ExpansionContext) {
        for part in &word.parts {
            Self::emit_word_part_expansion(part, line, tx, ctx);
        }
    }

    fn emit_word_part_expansion(part: &WordPart, line: u32, tx: &Sender<TraceEvent>, ctx: &ExpansionContext) {
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
                
                let output = ctx.get_var_or_env(&var_ref.name)
                    .map(|v| format!("\"{}\"", v))
                    .unwrap_or_else(|| "(unset)".to_string());
                    
                tx.send(TraceEvent::Expand {
                    line,
                    phase: ExpansionPhase::Parameter,
                    input,
                    output,
                    explanation: Self::explain_variable(var_ref),
                }).ok();
            }
            WordPart::CommandSubRaw(cmd) => {
                tx.send(TraceEvent::Expand {
                    line,
                    phase: ExpansionPhase::Command,
                    input: format!("$({})", cmd),
                    output: "(command executed)".to_string(),
                    explanation: "Executes command and substitutes its stdout".to_string(),
                }).ok();
            }
            WordPart::ArithRaw(expr) => {
                tx.send(TraceEvent::Expand {
                    line,
                    phase: ExpansionPhase::Arithmetic,
                    input: format!("$(({}))", expr),
                    output: "(arithmetic evaluated)".to_string(),
                    explanation: "Evaluates arithmetic expression".to_string(),
                }).ok();
            }
            WordPart::Tilde(user) => {
                let input = if let Some(u) = user {
                    format!("~{}", u)
                } else {
                    "~".to_string()
                };
                let output = ctx.home()
                    .map(|h| format!("\"{}\"", h.display()))
                    .unwrap_or_else(|| "(unknown)".to_string());
                tx.send(TraceEvent::Expand {
                    line,
                    phase: ExpansionPhase::Tilde,
                    input,
                    output,
                    explanation: "Expands to home directory".to_string(),
                }).ok();
            }
            WordPart::DoubleQuoted(parts) => {
                for p in parts {
                    Self::emit_word_part_expansion(p, line, tx, ctx);
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
                    output: "(process substitution)".to_string(),
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

    fn emit_statement_trace(&mut self, stmt: &Statement, line: u32, tx: &Sender<TraceEvent>, ctx: &ExpansionContext) {
        let summary = Self::summarize_statement(stmt);
        let ast_full = serde_json::to_value(stmt).ok();
        tx.send(TraceEvent::Parse {
            line,
            ast_summary: summary,
            ast_full,
        }).ok();

        // Always emit concept events - TUI will filter based on its teach_mode setting
        self.emit_concepts_for_statement(stmt, line, tx);

        match stmt {
            Statement::SimpleCommand(cmd) => {
                // Always emit command hints - TUI will filter based on its teach_mode setting
                if let Some(name) = cmd.name.as_literal() {
                    let args: Vec<String> = cmd.args.iter()
                        .filter_map(|w| w.as_literal().map(String::from))
                        .collect();
                    if let Some(hint) = self.command_explainer.format_hint(name, &args) {
                        tx.send(TraceEvent::CommandHint {
                            line,
                            command: name.to_string(),
                            hint,
                        }).ok();
                    }
                }
                
                Self::emit_word_expansions(&cmd.name, line, tx, ctx);
                for arg in &cmd.args {
                    Self::emit_word_expansions(arg, line, tx, ctx);
                }
                for assign in &cmd.env_assignments {
                    use colony_parser::AssignmentValue;
                    let value = match &assign.value {
                        AssignmentValue::Word(w) => w.as_literal()
                            .map(|s| format!("\"{}\"", s))
                            .unwrap_or_else(|| "(complex word)".to_string()),
                        AssignmentValue::Array(arr) => format!("({} elements)", arr.len()),
                        AssignmentValue::AssocArray(arr) => format!("(assoc: {} entries)", arr.len()),
                    };
                    let previous = ctx.get_var_or_env(&assign.name).map(String::from);
                    tx.send(TraceEvent::Assign {
                        line,
                        name: assign.name.clone(),
                        value,
                        previous,
                    }).ok();
                }
            }
            Statement::For(for_stmt) => {
                tx.send(TraceEvent::LoopEnter {
                    line,
                    loop_type: LoopType::For,
                    variable: Some(for_stmt.var.clone()),
                    values: None,
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
    
    /// Emit concept explanations for constructs in a statement (teach mode)
    fn emit_concepts_for_statement(&mut self, stmt: &Statement, line: u32, tx: &Sender<TraceEvent>) {
        // Detect concepts used in this statement and emit explanations for first occurrences
        let concepts = self.detect_concepts_in_statement(stmt);
        
        for concept in concepts {
            if self.concept_tracker.mark_seen(concept) {
                // First occurrence - emit full explanation
                tx.send(TraceEvent::Concept {
                    id: concept.id().to_string(),
                    name: concept.name().to_string(),
                    explanation: concept.explanation().to_string(),
                    related: concept.related().iter().map(|c| c.name().to_string()).collect(),
                }).ok();
            }
        }
    }
    
    fn detect_concepts_in_statement(&self, stmt: &Statement) -> Vec<Concept> {
        let mut concepts = Vec::new();
        
        match stmt {
            Statement::SimpleCommand(cmd) => {
                if let Some(name) = cmd.name.as_literal() {
                    if name == "[" || name == "test" {
                        concepts.push(Concept::TestBuiltin);
                    }
                }
                
                self.detect_concepts_in_word(&cmd.name, &mut concepts);
                for arg in &cmd.args {
                    self.detect_concepts_in_word(arg, &mut concepts);
                }
                
                for redirect in &cmd.redirects {
                    use colony_parser::RedirectKind;
                    match &redirect.kind {
                        RedirectKind::Output | RedirectKind::Clobber => concepts.push(Concept::OutputRedirect),
                        RedirectKind::Append | RedirectKind::AndAppend => concepts.push(Concept::AppendRedirect),
                        RedirectKind::Input | RedirectKind::ReadWrite => concepts.push(Concept::InputRedirect),
                        RedirectKind::HereDoc(kind) => {
                            use colony_parser::HereDocKind;
                            match kind {
                                HereDocKind::HereString => concepts.push(Concept::HereString),
                                _ => concepts.push(Concept::HereDoc),
                            }
                        }
                        RedirectKind::DupFd | RedirectKind::DupFdInput => {
                            concepts.push(Concept::FdDup);
                        }
                        RedirectKind::AndOutput => concepts.push(Concept::OutputRedirect),
                    }
                }
            }
            Statement::Pipeline(pipeline) => {
                if pipeline.elements.len() > 1 {
                    concepts.push(Concept::Pipeline);
                }
            }
            Statement::List(list) => {
                for (op, _) in &list.rest {
                    match op {
                        colony_parser::ListOp::And => concepts.push(Concept::AndList),
                        colony_parser::ListOp::Or => concepts.push(Concept::OrList),
                        colony_parser::ListOp::Background => concepts.push(Concept::Background),
                        _ => {}
                    }
                }
            }
            Statement::If(_) => concepts.push(Concept::IfStatement),
            Statement::Case(_) => concepts.push(Concept::CaseStatement),
            Statement::For(_) => concepts.push(Concept::ForLoop),
            Statement::While(_) => concepts.push(Concept::WhileLoop),
            Statement::Until(_) => concepts.push(Concept::UntilLoop),
            Statement::FunctionDef(_) => concepts.push(Concept::FunctionDefinition),
            Statement::Subshell(_) => concepts.push(Concept::Subshell),
            Statement::BraceGroup(_) => concepts.push(Concept::BraceGroup),
            Statement::Coproc(_) => concepts.push(Concept::Coprocess),
            Statement::Conditional(_) => concepts.push(Concept::TestBracket),
            Statement::Arithmetic(_) => concepts.push(Concept::ArithmeticCommand),
            _ => {}
        }
        
        concepts
    }
    
    fn detect_concepts_in_word(&self, word: &Word, concepts: &mut Vec<Concept>) {
        for part in &word.parts {
            match part {
                WordPart::Variable(var_ref) => {
                    match var_ref.name.as_str() {
                        "?" => concepts.push(Concept::LastExitCode),
                        "$" => concepts.push(Concept::CurrentPid),
                        "!" => concepts.push(Concept::LastBackgroundPid),
                        "#" => concepts.push(Concept::ParamCount),
                        "@" | "*" => concepts.push(Concept::AllParams),
                        "-" => concepts.push(Concept::ShellOptions),
                        n if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
                            concepts.push(Concept::PositionalParams);
                        }
                        _ => {}
                    }
                    
                    if let Some(ref modifier) = var_ref.modifier {
                        use colony_parser::VarModifier;
                        match modifier {
                            VarModifier::Default { .. } => concepts.push(Concept::ParameterDefault),
                            VarModifier::Assign { .. } => concepts.push(Concept::ParameterAssignDefault),
                            VarModifier::Error { .. } => concepts.push(Concept::ParameterError),
                            VarModifier::Alternate { .. } => concepts.push(Concept::ParameterAlternate),
                            VarModifier::Length => concepts.push(Concept::ParameterLength),
                            VarModifier::Slice { .. } => concepts.push(Concept::ParameterSubstring),
                            VarModifier::StripPrefix { .. } | VarModifier::StripPrefixGreedy { .. }
                            | VarModifier::StripSuffix { .. } | VarModifier::StripSuffixGreedy { .. } => {
                                concepts.push(Concept::ParameterPattern);
                            }
                            _ => {}
                        }
                    }
                    
                    if var_ref.subscript.is_some() {
                        concepts.push(Concept::IndexedArray);
                    }
                }
                WordPart::CommandSubRaw(_) => concepts.push(Concept::DollarParenSub),
                WordPart::ArithRaw(_) => concepts.push(Concept::ArithmeticExpansion),
                WordPart::Tilde(_) => {}
                WordPart::SingleQuoted(_) => concepts.push(Concept::SingleQuote),
                WordPart::DoubleQuoted(parts) => {
                    concepts.push(Concept::DoubleQuote);
                    for inner in parts {
                        self.detect_concepts_in_word_part(inner, concepts);
                    }
                }
                WordPart::AnsiCQuoted(_) => concepts.push(Concept::AnsiCQuote),
                WordPart::ProcessSub { direction, .. } => {
                    match direction {
                        colony_parser::ProcessSubDirection::Input => concepts.push(Concept::ProcessSubInput),
                        colony_parser::ProcessSubDirection::Output => concepts.push(Concept::ProcessSubOutput),
                    }
                }
                WordPart::ExtGlob { .. } => concepts.push(Concept::ExtGlob),
                WordPart::Literal(s) => {
                    if s.contains('*') && s.contains("**") {
                        concepts.push(Concept::GlobStar2);
                    } else if s.contains('*') {
                        concepts.push(Concept::GlobStar);
                    } else if s.contains('?') {
                        concepts.push(Concept::GlobQuestion);
                    } else if s.contains('[') {
                        concepts.push(Concept::GlobBracket);
                    }
                }
                _ => {}
            }
        }
    }
    
    fn detect_concepts_in_word_part(&self, part: &WordPart, concepts: &mut Vec<Concept>) {
        match part {
            WordPart::Variable(var_ref) => {
                if let Some(ref modifier) = var_ref.modifier {
                    use colony_parser::VarModifier;
                    match modifier {
                        VarModifier::Default { .. } => concepts.push(Concept::ParameterDefault),
                        VarModifier::Assign { .. } => concepts.push(Concept::ParameterAssignDefault),
                        VarModifier::Error { .. } => concepts.push(Concept::ParameterError),
                        VarModifier::Alternate { .. } => concepts.push(Concept::ParameterAlternate),
                        _ => {}
                    }
                }
            }
            WordPart::CommandSubRaw(_) => concepts.push(Concept::DollarParenSub),
            WordPart::ArithRaw(_) => concepts.push(Concept::ArithmeticExpansion),
            _ => {}
        }
    }

    fn collect_environment_snapshot(&self, ctx: &ExpansionContext) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        for (name, var, _attrs) in ctx.variables_with_attrs() {
            match var {
                Variable::String(s) => {
                    vars.insert(name.clone(), s.clone());
                }
                Variable::Array(arr) => {
                    vars.insert(name.clone(), format!("({} elements)", arr.len()));
                }
                Variable::AssocArray(map) => {
                    vars.insert(name.clone(), format!("(assoc: {} keys)", map.len()));
                }
                Variable::NameRef(target) => {
                    vars.insert(name.clone(), format!("&{}", target));
                }
            }
        }
        vars
    }

    fn detect_changed_variables(&self, _ctx: &ExpansionContext, stmt: &Statement) -> Vec<String> {
        let mut changed = Vec::new();
        match stmt {
            Statement::SimpleCommand(cmd) => {
                for assign in &cmd.env_assignments {
                    changed.push(assign.name.clone());
                }
            }
            _ => {}
        }
        changed
    }

    fn execute_with_capture(
        &self,
        executor: &mut Executor,
        stmt: &Statement,
    ) -> (String, Result<i32, colony_exec::ExecutionError>) {
        use std::fs::File;
        use std::os::unix::io::OwnedFd;
        use nix::unistd::close;
        
        let _ = io::stdout().flush();
        
        let (pipe_read, pipe_write): (OwnedFd, OwnedFd) = match pipe() {
            Ok(fds) => fds,
            Err(_) => {
                let result = futures::executor::block_on(async {
                    executor.execute_statement_recursive(stmt).await
                });
                return (String::new(), result);
            }
        };
        
        let read_fd = pipe_read.as_raw_fd();
        let write_fd = pipe_write.as_raw_fd();

        let saved_stdout: RawFd = match dup(1) {
            Ok(fd) => fd,
            Err(_) => {
                let result = futures::executor::block_on(async {
                    executor.execute_statement_recursive(stmt).await
                });
                return (String::new(), result);
            }
        };

        let saved_stderr: Option<RawFd> = dup(2).ok();
        if let Ok(devnull) = std::fs::File::open("/dev/null") {
            let _ = dup2(devnull.as_raw_fd(), 2);
        }

        if dup2(write_fd, 1).is_err() {
            let _ = dup2(saved_stdout, 1);
            let _ = close(saved_stdout);
            if let Some(fd) = saved_stderr {
                let _ = dup2(fd, 2);
                let _ = close(fd);
            }
            let result = futures::executor::block_on(async {
                executor.execute_statement_recursive(stmt).await
            });
            return (String::new(), result);
        }

        let result = futures::executor::block_on(async {
            executor.execute_statement_recursive(stmt).await
        });

        let _ = io::stdout().flush();
        
        let _ = dup2(saved_stdout, 1);
        let _ = close(saved_stdout);
        if let Some(fd) = saved_stderr {
            let _ = dup2(fd, 2);
            let _ = close(fd);
        }
        
        drop(pipe_write);

        let mut captured = String::new();
        let mut read_file = unsafe { File::from_raw_fd(read_fd) };
        std::mem::forget(pipe_read);
        
        use std::io::ErrorKind;
        use nix::fcntl::{fcntl, FcntlArg, OFlag};
        let _ = fcntl(read_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK));
        
        let mut buf = [0u8; 4096];
        loop {
            match read_file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => captured.push_str(&String::from_utf8_lossy(&buf[..n])),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        (captured, result)
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        tui: &mut TutorialTui,
        rx: &Receiver<TraceEvent>,
        tx: &Sender<TraceEvent>,
        program: &colony_parser::Program,
    ) -> TutorialResult<i32> {
        let source_clone = self.source.clone();
        let lines: Vec<&str> = source_clone.lines().collect();
        let mut current_stmt_idx = 0;
        let mut exit_code = 0;
        let mut last_execution = std::time::Instant::now();

        let context = ExpansionContext::new();
        let mut executor = Executor::new(context);

        loop {
            terminal.draw(|f| tui.render(f))?;

            if tui.should_quit() {
                break;
            }

            while let Ok(event) = rx.try_recv() {
                tui.handle_event(event);
            }

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                        break;
                    }
                    
                    tui.process_input(Event::Key(key));
                }
            }

            if tui.is_browsing_history() {
                continue;
            }

            let should_execute = if tui.is_stepping() {
                true
            } else if tui.is_playing() {
                let speed_ms = tui.get_playback_speed_ms();
                last_execution.elapsed() >= Duration::from_millis(speed_ms)
            } else {
                false
            };

            if should_execute && current_stmt_idx < program.statements.len() {
                let stmt = &program.statements[current_stmt_idx];
                
                if matches!(stmt, Statement::Empty) {
                    current_stmt_idx += 1;
                    continue;
                }
                
                let line_num = stmt.span().start.line;
                let line_idx = if line_num > 0 { (line_num - 1) as usize } else { current_stmt_idx };
                let line_source = lines.get(line_idx)
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                tx.send(TraceEvent::LineEnter {
                    line: line_num,
                    source: line_source.clone(),
                }).ok();

                self.emit_statement_trace(stmt, line_num, tx, executor.context());

                let complexity = ComplexityScorer::score(stmt);
                
                let (captured_output, result) = self.execute_with_capture(&mut executor, stmt);
                
                terminal.clear()?;
                
                if !captured_output.is_empty() {
                    tx.send(TraceEvent::Output {
                        line: line_num,
                        text: captured_output,
                    }).ok();
                }

                match result {
                    Ok(code) => {
                        exit_code = code;
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

                let env_vars = self.collect_environment_snapshot(executor.context());
                let changed_vars = self.detect_changed_variables(executor.context(), stmt);
                tx.send(TraceEvent::Environment {
                    line: line_num,
                    variables: env_vars,
                    changed: changed_vars,
                }).ok();

                tx.send(TraceEvent::LineExit {
                    line: line_num,
                    duration_us: 0,
                }).ok();

                current_stmt_idx += 1;
                last_execution = std::time::Instant::now();

                if tui.is_stepping() {
                    tui.finish_step();
                }

                if self.config.pause_on_complex && ComplexityScorer::should_auto_pause(complexity) {
                    tui.finish_step();
                }
            }

            if current_stmt_idx >= program.statements.len() && !tui.should_quit() {
                tx.send(TraceEvent::ScriptEnd {
                    exit_code,
                    duration_ms: 0,
                }).ok();
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
