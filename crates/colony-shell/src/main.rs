//! Colony Shell - POSIX-compliant shell for AI-native terminal

use colony_agent::{Agent, AgentConfig, NaturalLanguageDetector};
use colony_config::{discover_config, ColonyConfig, ConfigLocation, TrustManager, TrustStatus};
use colony_exec::{Executor, ExecutionError};
use colony_expand::ExpansionContext;
use colony_parser::pest_api::parse_statement;
use colony_parser::Statement;
use colony_tree_sitter::parse;
use colony_tuple::{Tuple, Value};
use miette::Result;
use std::io::{self, Write};

/// Control flow signal for loops and functions
#[derive(Debug, Clone, Copy, PartialEq)]
enum ControlFlow {
    /// Normal execution - continue
    Normal,
    /// Break out of loop
    Break,
    /// Continue to next iteration
    Continue,
    /// Return from function (not used yet)
    Return,
    /// Exit shell (handled separately)
    Exit,
}

/// Build AgentConfig from ColonyConfig
///
/// Merges .colonyrc settings with defaults:
/// - Custom personality/system prompt
/// - Temperature, max_tokens overrides
/// - Project goals and conventions injected into prompt
fn build_agent_config(colony_config: &Option<ColonyConfig>) -> AgentConfig {
    let mut system_prompt = String::from(
        "You are Colony Shell's AI assistant. You help users with \
         shell commands, system administration, and programming tasks.",
    );

    let mut use_caching = true;
    let mut max_tokens = 4096;
    let mut temperature = None;

    // Apply .colonyrc customizations
    if let Some(ref config) = colony_config {
        // Agent personality
        if let Some(ref agent_cfg) = config.agent {
            // Custom personality overrides default
            if let Some(ref personality) = agent_cfg.personality {
                system_prompt = personality.clone();
            }

            // Apply overrides
            if let Some(temp) = agent_cfg.temperature {
                temperature = Some(temp);
            }
            if let Some(tokens) = agent_cfg.max_tokens {
                max_tokens = tokens;
            }
        }

        // Inject project metadata into prompt
        if let Some(ref project) = config.project {
            system_prompt.push_str("\n\n## Project Context\n");

            if let Some(ref name) = project.name {
                system_prompt.push_str(&format!("Project: {}\n", name));
            }
            if let Some(ref desc) = project.description {
                system_prompt.push_str(&format!("Description: {}\n", desc));
            }
            if let Some(ref lang) = project.language {
                system_prompt.push_str(&format!("Language: {}\n", lang));
            }

            if !project.goals.is_empty() {
                system_prompt.push_str("\n### Project Goals\n");
                for goal in &project.goals {
                    system_prompt.push_str(&format!("- {}\n", goal));
                }
            }

            if !project.conventions.is_empty() {
                system_prompt.push_str("\n### Coding Conventions\n");
                for conv in &project.conventions {
                    system_prompt.push_str(&format!("- {}\n", conv));
                }
            }

            if !project.docs.is_empty() {
                system_prompt.push_str("\n### Documentation\n");
                for doc in &project.docs {
                    system_prompt.push_str(&format!("- {}\n", doc));
                }
            }
        }
    }

    AgentConfig {
        system_prompt,
        use_caching,
        max_tokens,
        temperature,
    }
}

/// Load .colonyrc with trust model (direnv-style)
///
/// Returns loaded config if trusted, None if not found or untrusted
async fn load_colonyrc_with_trust() -> Option<ColonyConfig> {
    // Discover config files
    let location = match discover_config() {
        Ok(Some(loc)) => loc,
        Ok(None) => return None, // No config found
        Err(e) => {
            eprintln!("⚠️  Config discovery error: {}", e);
            return None;
        }
    };

    // Check trust status
    match location.trust_status {
        TrustStatus::Trusted { .. } => {
            println!("📋 Loaded .colonyrc from: {}", location.path.display());
            Some(location.config)
        }

        TrustStatus::Untrusted { ref path, ref hash } => {
            println!();
            println!("⚠️  Found .colonyrc but directory not trusted");
            println!("   Path: {}", path.display());
            println!("   Hash: {}...", &hash[..16]);
            println!();
            println!("   This config will modify agent behavior.");
            print!("   Trust this directory? [y/N/show]: ");
            io::stdout().flush().ok();

            let mut response = String::new();
            if io::stdin().read_line(&mut response).is_ok() {
                match response.trim().to_lowercase().as_str() {
                    "y" | "yes" => {
                        // Trust and load
                        let mut trust_mgr = TrustManager::new().ok()?;
                        let content = std::fs::read_to_string(path).ok()?;

                        if trust_mgr.trust(path, &content).is_ok() {
                            println!("   ✓ Trusted and loaded .colonyrc");
                            Some(location.config)
                        } else {
                            eprintln!("   ✗ Failed to save trust");
                            None
                        }
                    }
                    "show" => {
                        // Show config content
                        if let Ok(content) = std::fs::read_to_string(path) {
                            println!("\n--- .colonyrc content ---");
                            println!("{}", content);
                            println!("--- end ---\n");
                        }
                        None // Don't load, user can re-run
                    }
                    _ => {
                        println!("   Config not loaded");
                        None
                    }
                }
            } else {
                None
            }
        }

        TrustStatus::Changed {
            ref path,
            ref current_hash,
            ref trusted_hash,
        } => {
            println!();
            println!("⚠️  .colonyrc has changed since last trust");
            println!("   Path: {}", path.display());
            println!("   Old hash: {}...", &trusted_hash[..16]);
            println!("   New hash: {}...", &current_hash[..16]);
            println!();
            print!("   Re-trust? [y/N/show]: ");
            io::stdout().flush().ok();

            let mut response = String::new();
            if io::stdin().read_line(&mut response).is_ok() {
                match response.trim().to_lowercase().as_str() {
                    "y" | "yes" => {
                        let mut trust_mgr = TrustManager::new().ok()?;
                        let content = std::fs::read_to_string(path).ok()?;

                        if trust_mgr.trust(path, &content).is_ok() {
                            println!("   ✓ Re-trusted and loaded .colonyrc");
                            Some(location.config)
                        } else {
                            eprintln!("   ✗ Failed to save trust");
                            None
                        }
                    }
                    "show" => {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            println!("\n--- .colonyrc content ---");
                            println!("{}", content);
                            println!("--- end ---\n");
                        }
                        None
                    }
                    _ => {
                        println!("   Config not loaded");
                        None
                    }
                }
            } else {
                None
            }
        }

        TrustStatus::NotFound => None,
    }
}

use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Editor, Helper};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() >= 3 && args[1] == "-c" {
        let command = &args[2];

        let mut context = ExpansionContext::new();
        if args.len() > 3 {
            context.set_positional_params(args[3..].to_vec());
        }

        let mut executor = Executor::new(context);

        // Execute the command and exit with its status
        let exit_code = execute_command_with_status(command, &mut executor).await;
        std::process::exit(exit_code);
    }

    // No -c flag: start interactive REPL
    println!("Colony Shell v{}", env!("CARGO_PKG_VERSION"));
    println!("POSIX-compliant shell with AI agent integration");
    println!();

    // Load .colonyrc configuration
    let colony_config = load_colonyrc_with_trust().await;

    // Build agent config from .colonyrc or defaults
    let agent_config = build_agent_config(&colony_config);

    // Initialize agent with auto-detected provider
    let agent = match Agent::with_config(agent_config).await {
        Ok(agent) => {
            let (provider, model) = agent.current_provider();
            println!("🤖 Agent mode enabled");
            println!("   Provider: {:?}, Model: {}", provider, model);
            println!("   Use # or ? prefix for natural language queries");
            Some(agent)
        }
        Err(e) => {
            println!("⚠️  Agent mode disabled: {}", e);
            println!("   Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or install Ollama");
            None
        }
    };

    println!("   Type 'exit' to quit");

    // Initialize executor with reactive tuple space
    let context = ExpansionContext::new();
    let executor = Executor::new(context);

    println!("🔄 Reactive environment enabled");
    println!("   State changes emit tuples to tuple space\n");

    // Start REPL
    run_repl(agent, executor).await
}

/// Rustyline helper with tree-sitter validator
struct ColonyHelper;

impl ColonyHelper {
    fn new() -> Self {
        Self
    }
}

impl Completer for ColonyHelper {
    type Candidate = String;

    fn complete(
        &self,
        _line: &str,
        _pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        Ok((0, vec![]))  // No completion for now
    }
}

impl Hinter for ColonyHelper {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        None  // No hints for now
    }
}

impl Highlighter for ColonyHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> std::borrow::Cow<'l, str> {
        // TODO: Emit OSC 2337 for syntax highlighting
        // For now, just return the line as-is
        std::borrow::Cow::Borrowed(line)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        false  // Don't force character-level highlighting
    }
}

impl Validator for ColonyHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();

        // Parse with tree-sitter for syntax validation
        let tree = parse(input);

        match tree {
            Some(tree) => {
                let root = tree.root_node();
                let has_error = root.has_error();

                tracing::trace!(
                    "tree-sitter validation: kind={}, has_error={}, byte_range={}..{}",
                    root.kind(),
                    has_error,
                    root.start_byte(),
                    root.end_byte()
                );

                if has_error {
                    // Incomplete input - allow user to continue
                    Ok(ValidationResult::Incomplete)
                } else {
                    Ok(ValidationResult::Valid(None))
                }
            }
            None => Ok(ValidationResult::Incomplete),
        }
    }
}

impl Helper for ColonyHelper {}

async fn run_repl(mut agent: Option<Agent>, mut executor: Executor) -> Result<()> {
    let helper = ColonyHelper::new();
    let mut rl = Editor::new()
        .map_err(|e| miette::miette!("failed to create readline: {}", e))?;
    rl.set_helper(Some(helper));

    loop {
        let prompt = if agent.is_some() { "colony> " } else { "shell> " };

        let readline = rl.readline(prompt);

        match readline {
            Ok(line) => {
                let line = line.trim();

                if line.is_empty() {
                    continue;
                }

                // Add to history
                rl.add_history_entry(line)
                    .map_err(|e| miette::miette!("history error: {}", e))?;

                // Handle exit
                if line == "exit" || line == "quit" {
                    println!("Goodbye!");
                    break;
                }

                // Handle llm command (model management)
                if line.starts_with("llm ") {
                    handle_llm_command(line, &mut agent).await;
                    continue;
                }

                // Route to agent or shell
                if let Some(ref mut agent) = agent {
                    if NaturalLanguageDetector::is_natural_language(line) {
                        // Route to agent
                        match agent.process(line).await {
                            Ok(_response) => {
                                println!(); // Newline after streaming
                            }
                            Err(e) => {
                                eprintln!("Agent error: {}", e);
                            }
                        }
                        continue;
                    }
                }

                // Try parsing and executing shell command
                match execute_shell_command(line, &mut executor).await {
                    Ok(()) => {
                        // Display any reactive tuples that were emitted
                        display_tuple_activity(&executor);
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }

            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }

            Err(ReadlineError::Eof) => {
                println!("exit");
                break;
            }

            Err(err) => {
                eprintln!("Error: {}", err);
                break;
            }
        }
    }

    Ok(())
}

/// Execute a list with control flow propagation
async fn execute_list_with_control(list: &colony_parser::CommandList, executor: &mut Executor) -> (i32, ControlFlow) {
    use colony_parser::ListOp;

    // Execute first statement
    let (mut last_exit_code, mut control) = execute_statement_with_control(&*list.first, executor).await;
    executor.context_mut().set_last_exit_status(last_exit_code);

    // If we got break/continue, propagate it immediately
    if control != ControlFlow::Normal {
        return (last_exit_code, control);
    }

    // Execute rest of list based on operators
    for (op, stmt) in &list.rest {
        let should_execute = match op {
            ListOp::Semi => true,
            ListOp::And => last_exit_code == 0,
            ListOp::Or => last_exit_code != 0,
            ListOp::Background => {
                eprintln!("Background jobs (&) not yet supported");
                false
            }
        };

        if should_execute {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit_code = code;
            executor.context_mut().set_last_exit_status(last_exit_code);
            control = flow;

            // Propagate break/continue
            if control != ControlFlow::Normal {
                return (last_exit_code, control);
            }
        }
    }

    (last_exit_code, control)
}

/// Execute if with control flow propagation
async fn execute_if_with_control(if_stmt: &colony_parser::IfStatement, executor: &mut Executor) -> (i32, ControlFlow) {
    use colony_parser::Condition;

    // Evaluate the condition
    let (condition_exit, _) = match &if_stmt.condition {
        Condition::Command(cmd) => execute_statement_with_control(cmd, executor).await,
        Condition::Expression(_expr) => {
            eprintln!("Expression conditions not yet supported");
            (1, ControlFlow::Normal)
        }
    };

    let condition_result = condition_exit == 0;

    // Execute appropriate branch
    if condition_result {
        // Execute then body
        let mut last_exit = 0;
        for stmt in &if_stmt.then_body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;
            if flow != ControlFlow::Normal {
                return (last_exit, flow);  // Propagate break/continue
            }
        }
        (last_exit, ControlFlow::Normal)
    } else {
        // Check elif branches
        for elif in &if_stmt.elif_branches {
            let (elif_exit, _) = match &elif.condition {
                Condition::Command(cmd) => execute_statement_with_control(cmd, executor).await,
                Condition::Expression(_) => (1, ControlFlow::Normal),
            };

            if elif_exit == 0 {
                let mut last_exit = 0;
                for stmt in &elif.body {
                    let (code, flow) = execute_statement_with_control(stmt, executor).await;
                    last_exit = code;
                    if flow != ControlFlow::Normal {
                        return (last_exit, flow);
                    }
                }
                return (last_exit, ControlFlow::Normal);
            }
        }

        // Execute else body if present
        if let Some(else_body) = &if_stmt.else_body {
            let mut last_exit = 0;
            for stmt in else_body {
                let (code, flow) = execute_statement_with_control(stmt, executor).await;
                last_exit = code;
                if flow != ControlFlow::Normal {
                    return (last_exit, flow);
                }
            }
            (last_exit, ControlFlow::Normal)
        } else {
            (0, ControlFlow::Normal)
        }
    }
}

/// Execute for loop with control flow returns
async fn execute_for_loop_with_control(for_stmt: &colony_parser::ForStatement, executor: &mut Executor) -> i32 {
    use colony_expand::Expander;

    // Expand the word list (with command substitution support)
    let mut expander = Expander::with_executor(executor.context().clone(), |stmt, ctx| {
        colony_exec::execute_and_capture(stmt, ctx)
    });
    let mut values = Vec::new();

    for word in &for_stmt.words {
        match expander.expand_word(word) {
            Ok(fields) => {
                values.extend(fields);
            }
            Err(e) => {
                eprintln!("expansion error: {}", e);
                return 1;
            }
        }
    }

    // Iterate over values
    let mut last_exit = 0;
    for field in values {
        // Set loop variable
        executor.context_mut().set_var(for_stmt.var.clone(), field.text);

        // Execute body
        for stmt in &for_stmt.body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;

            match flow {
                ControlFlow::Break => return last_exit,
                ControlFlow::Continue => break,  // Skip to next iteration
                _ => {}
            }
        }
    }

    last_exit
}

/// Execute an if statement (old version for compatibility)
async fn execute_if_statement(if_stmt: &colony_parser::IfStatement, executor: &mut Executor) -> i32 {
    use colony_parser::Condition;

    // Evaluate the condition
    let condition_result = match &if_stmt.condition {
        Condition::Command(cmd) => {
            // Execute command and check exit code
            execute_statement_for_code(cmd, executor).await == 0
        }
        Condition::Expression(_expr) => {
            // Expression conditions not yet implemented
            eprintln!("Expression conditions not yet supported");
            false
        }
    };

    // Execute appropriate branch
    if condition_result {
        // Execute then body
        let mut last_exit = 0;
        for stmt in &if_stmt.then_body {
            last_exit = execute_statement_for_code(stmt, executor).await;
        }
        last_exit
    } else {
        // Check elif branches
        for elif in &if_stmt.elif_branches {
            let elif_result = match &elif.condition {
                Condition::Command(cmd) => {
                    execute_statement_for_code(cmd, executor).await == 0
                }
                Condition::Expression(_) => false,
            };

            if elif_result {
                let mut last_exit = 0;
                for stmt in &elif.body {
                    last_exit = execute_statement_for_code(stmt, executor).await;
                }
                return last_exit;
            }
        }

        // Execute else body if present
        if let Some(else_body) = &if_stmt.else_body {
            let mut last_exit = 0;
            for stmt in else_body {
                last_exit = execute_statement_for_code(stmt, executor).await;
            }
            last_exit
        } else {
            0  // No else, return success
        }
    }
}

/// Evaluate a [[ ]] conditional expression
fn evaluate_conditional(expr: &colony_parser::CondExpr, executor: &Executor) -> bool {
    use colony_parser::{CondExpr, CondBinaryOp, CondUnaryOp};
    
    match expr {
        CondExpr::And(left, right) => {
            evaluate_conditional(left, executor) && evaluate_conditional(right, executor)
        }
        CondExpr::Or(left, right) => {
            evaluate_conditional(left, executor) || evaluate_conditional(right, executor)
        }
        CondExpr::Not(inner) => {
            !evaluate_conditional(inner, executor)
        }
        CondExpr::Binary { left, op, right } => {
            let left_expanded = expand_cond_operand(left, executor);
            let right_expanded = expand_cond_operand(right, executor);
            
            match op {
                CondBinaryOp::Eq => {
                    glob_match(&right_expanded, &left_expanded)
                }
                CondBinaryOp::Ne => {
                    !glob_match(&right_expanded, &left_expanded)
                }
                CondBinaryOp::Match => {
                    match regex::Regex::new(&right_expanded) {
                        Ok(re) => re.is_match(&left_expanded),
                        Err(_) => false,
                    }
                }
                CondBinaryOp::Lt => left_expanded < right_expanded,
                CondBinaryOp::Gt => left_expanded > right_expanded,
                CondBinaryOp::IntEq => {
                    left_expanded.parse::<i64>().ok() == right_expanded.parse::<i64>().ok()
                }
                CondBinaryOp::IntNe => {
                    left_expanded.parse::<i64>().ok() != right_expanded.parse::<i64>().ok()
                }
                CondBinaryOp::IntLt => {
                    match (left_expanded.parse::<i64>(), right_expanded.parse::<i64>()) {
                        (Ok(l), Ok(r)) => l < r,
                        _ => false,
                    }
                }
                CondBinaryOp::IntLe => {
                    match (left_expanded.parse::<i64>(), right_expanded.parse::<i64>()) {
                        (Ok(l), Ok(r)) => l <= r,
                        _ => false,
                    }
                }
                CondBinaryOp::IntGt => {
                    match (left_expanded.parse::<i64>(), right_expanded.parse::<i64>()) {
                        (Ok(l), Ok(r)) => l > r,
                        _ => false,
                    }
                }
                CondBinaryOp::IntGe => {
                    match (left_expanded.parse::<i64>(), right_expanded.parse::<i64>()) {
                        (Ok(l), Ok(r)) => l >= r,
                        _ => false,
                    }
                }
                CondBinaryOp::NewerThan => {
                    use std::fs;
                    match (fs::metadata(&left_expanded), fs::metadata(&right_expanded)) {
                        (Ok(m1), Ok(m2)) => m1.modified().ok() > m2.modified().ok(),
                        _ => false,
                    }
                }
                CondBinaryOp::OlderThan => {
                    use std::fs;
                    match (fs::metadata(&left_expanded), fs::metadata(&right_expanded)) {
                        (Ok(m1), Ok(m2)) => m1.modified().ok() < m2.modified().ok(),
                        _ => false,
                    }
                }
                CondBinaryOp::SameFile => {
                    use std::os::unix::fs::MetadataExt;
                    use std::fs;
                    match (fs::metadata(&left_expanded), fs::metadata(&right_expanded)) {
                        (Ok(m1), Ok(m2)) => m1.dev() == m2.dev() && m1.ino() == m2.ino(),
                        _ => false,
                    }
                }
            }
        }
        CondExpr::Unary { op, arg } => {
            let arg_expanded = expand_cond_operand(arg, executor);
            let path = std::path::Path::new(&arg_expanded);
            
            match op {
                CondUnaryOp::NonZeroLen => !arg_expanded.is_empty(),
                CondUnaryOp::ZeroLen => arg_expanded.is_empty(),
                CondUnaryOp::IsFile => path.is_file(),
                CondUnaryOp::IsDir => path.is_dir(),
                CondUnaryOp::Exists => path.exists(),
                CondUnaryOp::Readable => path.exists() && std::fs::File::open(path).is_ok(),
                CondUnaryOp::Writable => {
                    use std::fs::OpenOptions;
                    path.exists() && OpenOptions::new().write(true).open(path).is_ok()
                }
                CondUnaryOp::Executable => {
                    use std::os::unix::fs::PermissionsExt;
                    path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
                }
                CondUnaryOp::NonEmpty => {
                    path.metadata().map(|m| m.len() > 0).unwrap_or(false)
                }
                CondUnaryOp::IsSymlink => path.is_symlink(),
                CondUnaryOp::IsPipe => {
                    use std::os::unix::fs::FileTypeExt;
                    path.metadata().map(|m| m.file_type().is_fifo()).unwrap_or(false)
                }
                CondUnaryOp::IsSocket => {
                    use std::os::unix::fs::FileTypeExt;
                    path.metadata().map(|m| m.file_type().is_socket()).unwrap_or(false)
                }
                CondUnaryOp::IsBlock => {
                    use std::os::unix::fs::FileTypeExt;
                    path.metadata().map(|m| m.file_type().is_block_device()).unwrap_or(false)
                }
                CondUnaryOp::IsChar => {
                    use std::os::unix::fs::FileTypeExt;
                    path.metadata().map(|m| m.file_type().is_char_device()).unwrap_or(false)
                }
                CondUnaryOp::IsTerminal => {
                    arg_expanded.parse::<i32>().map(|fd| unsafe { libc::isatty(fd) } != 0).unwrap_or(false)
                }
                CondUnaryOp::OwnedByEuid => {
                    use std::os::unix::fs::MetadataExt;
                    path.metadata().map(|m| m.uid() == unsafe { libc::geteuid() }).unwrap_or(false)
                }
                CondUnaryOp::OwnedByEgid => {
                    use std::os::unix::fs::MetadataExt;
                    path.metadata().map(|m| m.gid() == unsafe { libc::getegid() }).unwrap_or(false)
                }
                CondUnaryOp::ModifiedSinceRead => {
                    use std::os::unix::fs::MetadataExt;
                    path.metadata().map(|m| m.mtime() > m.atime()).unwrap_or(false)
                }
                CondUnaryOp::VarIsSet => {
                    executor.context().get_var_or_env(&arg_expanded).is_some()
                }
            }
        }
        CondExpr::String(s) => {
            let expanded = expand_cond_operand(s, executor);
            !expanded.is_empty()
        }
    }
}

fn expand_cond_operand(s: &str, executor: &Executor) -> String {
    if s.starts_with('$') {
        let var_name = s.trim_start_matches('$').trim_start_matches('{').trim_end_matches('}');
        executor.context().get_var_or_env(var_name).unwrap_or_default()
    } else if s.starts_with('"') && s.ends_with('"') {
        s[1..s.len()-1].to_string()
    } else if s.starts_with('\'') && s.ends_with('\'') {
        s[1..s.len()-1].to_string()
    } else {
        s.to_string()
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();
    
    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                if pattern_chars.peek().is_none() {
                    return true;
                }
                let remaining_pattern: String = pattern_chars.collect();
                let text_remaining: Vec<char> = text_chars.collect();
                for i in 0..=text_remaining.len() {
                    if glob_match(&remaining_pattern, &text_remaining[i..].iter().collect::<String>()) {
                        return true;
                    }
                }
                return false;
            }
            '?' => {
                if text_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                if text_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }
    
    text_chars.next().is_none()
}

fn evaluate_arithmetic_command(expr: &str, executor: &Executor) -> i64 {
    let expanded = expand_arithmetic_vars(expr, executor);
    evaluate_arithmetic_expr(&expanded)
}

fn expand_arithmetic_vars(expr: &str, executor: &Executor) -> String {
    let mut result = String::new();
    let mut chars = expr.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut var_name = String::new();
            if chars.peek() == Some(&'{') {
                chars.next();
                while let Some(&c) = chars.peek() {
                    if c == '}' {
                        chars.next();
                        break;
                    }
                    var_name.push(chars.next().unwrap());
                }
            } else {
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        var_name.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
            }
            let val = executor.context().get_var_or_env(&var_name).unwrap_or_default();
            result.push_str(&val);
        } else if c.is_alphabetic() || c == '_' {
            let mut var_name = String::from(c);
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    var_name.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if chars.peek() == Some(&'[') {
                chars.next();
                let mut index = String::new();
                let mut bracket_depth = 1;
                while let Some(&c) = chars.peek() {
                    if c == '[' {
                        bracket_depth += 1;
                    } else if c == ']' {
                        bracket_depth -= 1;
                        if bracket_depth == 0 {
                            chars.next();
                            break;
                        }
                    }
                    index.push(chars.next().unwrap());
                }
                let expanded_index = expand_arithmetic_vars(&index, executor);
                let idx: usize = evaluate_arithmetic_expr(&expanded_index) as usize;
                
                if let Some(colony_expand::Variable::Array(arr)) = executor.context().get_variable(&var_name) {
                    if let Some(val) = arr.get(idx) {
                        result.push_str(val);
                    } else {
                        result.push('0');
                    }
                } else {
                    result.push('0');
                }
            } else {
                if let Some(val) = executor.context().get_var_or_env(&var_name) {
                    result.push_str(&val);
                } else {
                    result.push_str(&var_name);
                }
            }
        } else {
            result.push(c);
        }
    }
    
    result
}

fn evaluate_arithmetic_expr(expr: &str) -> i64 {
    let expr = expr.trim();
    
    if let Some(idx) = expr.find("||") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left != 0 || right != 0 { 1 } else { 0 };
    }
    if let Some(idx) = expr.find("&&") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left != 0 && right != 0 { 1 } else { 0 };
    }
    
    if let Some(idx) = expr.find(">=") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left >= right { 1 } else { 0 };
    }
    if let Some(idx) = expr.find("<=") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left <= right { 1 } else { 0 };
    }
    if let Some(idx) = expr.find("==") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left == right { 1 } else { 0 };
    }
    if let Some(idx) = expr.find("!=") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left != right { 1 } else { 0 };
    }
    if let Some(idx) = expr.rfind('>') {
        if idx > 0 && !expr[..idx].ends_with('>') && !expr[..idx].ends_with('|') && !expr[..idx].ends_with('=') {
            let left = evaluate_arithmetic_expr(&expr[..idx]);
            let right = evaluate_arithmetic_expr(&expr[idx+1..]);
            return if left > right { 1 } else { 0 };
        }
    }
    if let Some(idx) = expr.rfind('<') {
        if idx > 0 && !expr[..idx].ends_with('<') && !expr[..idx].ends_with('|') && !expr[..idx].ends_with('=') {
            let left = evaluate_arithmetic_expr(&expr[..idx]);
            let right = evaluate_arithmetic_expr(&expr[idx+1..]);
            return if left < right { 1 } else { 0 };
        }
    }
    
    if let Some(idx) = expr.rfind('+') {
        if idx > 0 {
            let left = evaluate_arithmetic_expr(&expr[..idx]);
            let right = evaluate_arithmetic_expr(&expr[idx+1..]);
            return left + right;
        }
    }
    if let Some(idx) = expr.rfind('-') {
        if idx > 0 {
            let left = evaluate_arithmetic_expr(&expr[..idx]);
            let right = evaluate_arithmetic_expr(&expr[idx+1..]);
            return left - right;
        }
    }
    if let Some(idx) = expr.rfind('*') {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+1..]);
        return left * right;
    }
    if let Some(idx) = expr.rfind('/') {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+1..]);
        if right != 0 {
            return left / right;
        }
        return 0;
    }
    if let Some(idx) = expr.rfind('%') {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+1..]);
        if right != 0 {
            return left % right;
        }
        return 0;
    }
    
    if expr.starts_with('(') && expr.ends_with(')') {
        return evaluate_arithmetic_expr(&expr[1..expr.len()-1]);
    }
    
    if let Some(stripped) = expr.strip_prefix('-') {
        return -evaluate_arithmetic_expr(stripped);
    }
    
    expr.parse().unwrap_or(0)
}

fn evaluate_cstyle_arith_expr(expr: &str, executor: &mut Executor) -> i64 {
    let expr = expr.trim();
    if expr.is_empty() {
        return 0;
    }
    
    let exprs: Vec<&str> = expr.split(',').collect();
    if exprs.len() > 1 {
        let mut result = 0;
        for e in exprs {
            result = evaluate_cstyle_arith_single(e.trim(), executor);
        }
        return result;
    }
    
    evaluate_cstyle_arith_single(expr, executor)
}

fn evaluate_cstyle_arith_single(expr: &str, executor: &mut Executor) -> i64 {
    let expr = expr.trim();
    if expr.is_empty() {
        return 0;
    }
    
    if expr.starts_with("++") {
        let var = &expr[2..].trim();
        let current = get_var_as_i64(var, executor);
        let new_val = current + 1;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    if expr.starts_with("--") {
        let var = &expr[2..].trim();
        let current = get_var_as_i64(var, executor);
        let new_val = current - 1;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    
    if expr.ends_with("++") {
        let var = expr[..expr.len()-2].trim();
        let current = get_var_as_i64(var, executor);
        executor.context_mut().set_var(var.to_string(), (current + 1).to_string());
        return current;
    }
    if expr.ends_with("--") {
        let var = expr[..expr.len()-2].trim();
        let current = get_var_as_i64(var, executor);
        executor.context_mut().set_var(var.to_string(), (current - 1).to_string());
        return current;
    }
    
    if let Some(idx) = expr.find("+=") {
        let var = expr[..idx].trim();
        let val_expr = expr[idx+2..].trim();
        let current = get_var_as_i64(var, executor);
        let delta = evaluate_cstyle_arith_value(val_expr, executor);
        let new_val = current + delta;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    if let Some(idx) = expr.find("-=") {
        let var = expr[..idx].trim();
        let val_expr = expr[idx+2..].trim();
        let current = get_var_as_i64(var, executor);
        let delta = evaluate_cstyle_arith_value(val_expr, executor);
        let new_val = current - delta;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    if let Some(idx) = expr.find("*=") {
        let var = expr[..idx].trim();
        let val_expr = expr[idx+2..].trim();
        let current = get_var_as_i64(var, executor);
        let factor = evaluate_cstyle_arith_value(val_expr, executor);
        let new_val = current * factor;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    if let Some(idx) = expr.find("/=") {
        let var = expr[..idx].trim();
        let val_expr = expr[idx+2..].trim();
        let current = get_var_as_i64(var, executor);
        let divisor = evaluate_cstyle_arith_value(val_expr, executor);
        if divisor != 0 {
            let new_val = current / divisor;
            executor.context_mut().set_var(var.to_string(), new_val.to_string());
            return new_val;
        }
        return 0;
    }
    
    if let Some(idx) = expr.find('=') {
        if idx > 0 && !matches!(expr.chars().nth(idx.saturating_sub(1)), Some('!') | Some('<') | Some('>') | Some('=')) 
            && !matches!(expr.chars().nth(idx+1), Some('=')) {
            let var = expr[..idx].trim();
            let val_expr = expr[idx+1..].trim();
            let new_val = evaluate_cstyle_arith_value(val_expr, executor);
            executor.context_mut().set_var(var.to_string(), new_val.to_string());
            return new_val;
        }
    }
    
    evaluate_cstyle_arith_value(expr, executor)
}

fn evaluate_cstyle_arith_value(expr: &str, executor: &Executor) -> i64 {
    let expanded = expand_arithmetic_vars(expr, executor);
    evaluate_arithmetic_expr(&expanded)
}

fn get_var_as_i64(var: &str, executor: &Executor) -> i64 {
    executor.context().get_var_or_env(var)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

async fn execute_cstyle_for_loop(
    cstyle_for: &colony_parser::CStyleForStatement,
    executor: &mut Executor,
) -> i32 {
    if let Some(ref init) = cstyle_for.init {
        evaluate_cstyle_arith_expr(init, executor);
    }
    
    let mut last_exit = 0;
    
    loop {
        if let Some(ref cond) = cstyle_for.condition {
            let cond_result = evaluate_cstyle_arith_value(cond, executor);
            if cond_result == 0 {
                break;
            }
        }
        
        for stmt in &cstyle_for.body {
            if let Statement::SimpleCommand(cmd) = stmt {
                if let Some(name) = cmd.name.as_literal() {
                    match name {
                        "break" => return last_exit,
                        "continue" => {
                            if let Some(ref update) = cstyle_for.update {
                                evaluate_cstyle_arith_expr(update, executor);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            
            last_exit = execute_statement_for_code(stmt, executor).await;
        }
        
        if let Some(ref update) = cstyle_for.update {
            evaluate_cstyle_arith_expr(update, executor);
        }
    }
    
    last_exit
}

async fn execute_cstyle_for_loop_with_control(
    cstyle_for: &colony_parser::CStyleForStatement,
    executor: &mut Executor,
) -> i32 {
    if let Some(ref init) = cstyle_for.init {
        evaluate_cstyle_arith_expr(init, executor);
    }
    
    let mut last_exit = 0;
    
    loop {
        if let Some(ref cond) = cstyle_for.condition {
            let cond_result = evaluate_cstyle_arith_value(cond, executor);
            if cond_result == 0 {
                break;
            }
        }
        
        let mut should_break = false;
        let mut should_continue = false;
        
        for stmt in &cstyle_for.body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;
            
            match flow {
                ControlFlow::Break => {
                    should_break = true;
                    break;
                }
                ControlFlow::Continue => {
                    should_continue = true;
                    break;
                }
                ControlFlow::Return | ControlFlow::Exit => {
                    should_break = true;
                    break;
                }
                ControlFlow::Normal => {}
            }
        }
        
        if should_break {
            break;
        }
        
        if let Some(ref update) = cstyle_for.update {
            evaluate_cstyle_arith_expr(update, executor);
        }
        
        if should_continue {
            continue;
        }
    }
    
    last_exit
}

/// Execute a for loop with break/continue support
async fn execute_for_loop(for_stmt: &colony_parser::ForStatement, executor: &mut Executor) -> i32 {
    use colony_expand::Expander;

    // Expand the word list (with command substitution support)
    let mut expander = Expander::with_executor(executor.context().clone(), |stmt, ctx| {
        colony_exec::execute_and_capture(stmt, ctx)
    });
    let mut values = Vec::new();

    for word in &for_stmt.words {
        match expander.expand_word(word) {
            Ok(fields) => {
                // Fields are the expanded words - just use them directly
                values.extend(fields);
            }
            Err(e) => {
                eprintln!("expansion error: {}", e);
                return 1;
            }
        }
    }

    // Iterate over values
    let mut last_exit = 0;
    for field in values {
        // Set loop variable (Field has .text)
        executor.context_mut().set_var(for_stmt.var.clone(), field.text);

        // Execute body
        for stmt in &for_stmt.body {
            // Check for break/continue
            if let Statement::SimpleCommand(cmd) = stmt {
                if let Some(name) = cmd.name.as_literal() {
                    match name {
                        "break" => return last_exit, // Exit loop immediately
                        "continue" => break,          // Skip to next iteration
                        _ => {}
                    }
                }
            }

            // Execute statement
            last_exit = execute_statement_for_code(stmt, executor).await;
        }
    }

    last_exit
}

/// Helper: Execute any statement and return (exit_code, control_flow)
fn execute_statement_with_control<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = (i32, ControlFlow)> + 'a>> {
    Box::pin(async move {
    match stmt {
        Statement::SimpleCommand(cmd) => {
            // Check for break/continue
            if let Some(literal) = cmd.name.as_literal() {
                match literal {
                    "break" => return (0, ControlFlow::Break),
                    "continue" => return (0, ControlFlow::Continue),
                    _ => {}
                }
            }

            match executor.execute_simple_command(cmd).await {
                Ok(status) => (status.code(), ControlFlow::Normal),
                Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                Err(e) => {
                    eprintln!("execution error: {}", e);
                    (1, ControlFlow::Normal)
                }
            }
        }
        Statement::Pipeline(pipeline) => {
            if pipeline.elements.len() == 1 {
                return execute_statement_with_control(&pipeline.elements[0].command, executor).await;
            }
            match executor.execute_pipeline(&pipeline).await {
                Ok(status) => (status.code(), ControlFlow::Normal),
                Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                Err(e) => {
                    eprintln!("execution error: {}", e);
                    (1, ControlFlow::Normal)
                }
            }
        }
        Statement::List(list) => {
            execute_list_with_control(list, executor).await
        }
        Statement::For(for_stmt) => {
            (execute_for_loop_with_control(for_stmt, executor).await, ControlFlow::Normal)
        }
        Statement::CStyleFor(cstyle_for) => {
            (execute_cstyle_for_loop_with_control(cstyle_for, executor).await, ControlFlow::Normal)
        }
        Statement::If(if_stmt) => {
            execute_if_with_control(if_stmt, executor).await
        }
        Statement::While(_) | Statement::Until(_) | Statement::Case(_) => {
            match executor.execute_statement_recursive(stmt).await {
                Ok(code) => (code, ControlFlow::Normal),
                Err(ExecutionError::Break) => (0, ControlFlow::Break),
                Err(ExecutionError::Continue) => (0, ControlFlow::Continue),
                Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                Err(e) => {
                    eprintln!("{}", e);
                    (1, ControlFlow::Normal)
                }
            }
        }
        Statement::BraceGroup(brace_group) => {
            use colony_exec::builtin_redirect::BuiltinRedirectGuard;
            let _redirect_guard = match BuiltinRedirectGuard::apply(&brace_group.redirects, executor.context_mut()) {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("colony: redirect error: {}", e);
                    return (1, ControlFlow::Normal);
                }
            };
            let mut last_exit = 0;
            for stmt in &brace_group.body {
                let (exit_code, flow) = execute_statement_with_control(stmt, executor).await;
                last_exit = exit_code;
                match flow {
                    ControlFlow::Normal => {}
                    other => return (last_exit, other),
                }
            }
            (last_exit, ControlFlow::Normal)
        }
        Statement::Subshell(subshell) => {
            use colony_exec::builtin_redirect::BuiltinRedirectGuard;
            let _redirect_guard = match BuiltinRedirectGuard::apply(&subshell.redirects, executor.context_mut()) {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("colony: redirect error: {}", e);
                    return (1, ControlFlow::Normal);
                }
            };
            let mut last_exit = 0;
            for stmt in &subshell.body {
                let (exit_code, flow) = execute_statement_with_control(stmt, executor).await;
                last_exit = exit_code;
                match flow {
                    ControlFlow::Normal => {}
                    ControlFlow::Return | ControlFlow::Break | ControlFlow::Continue => {
                        return (last_exit, flow);
                    }
                    ControlFlow::Exit => return (last_exit, ControlFlow::Exit),
                }
            }
            (last_exit, ControlFlow::Normal)
        }
        Statement::FunctionDef(func_def) => {
            // Store function definition
            executor.context_mut().define_function(func_def.name.clone(), func_def.clone());
            (0, ControlFlow::Normal)
        }
        Statement::Return(ret_stmt) => {
            // Return statement - propagate as control flow
            let exit_code = if let Some(ref _expr) = ret_stmt.value {
                // TODO: Evaluate expression
                0
            } else {
                0
            };
            (exit_code, ControlFlow::Return)
        }
        Statement::VarDecl(_) | Statement::ProcDef(_) => {
            eprintln!("This declaration not yet supported");
            (1, ControlFlow::Normal)
        }
        Statement::Conditional(cond_cmd) => {
            let result = evaluate_conditional(&cond_cmd.expr, executor);
            (if result { 0 } else { 1 }, ControlFlow::Normal)
        }
        Statement::Arithmetic(arith_cmd) => {
            let result = evaluate_arithmetic_command(&arith_cmd.expr, executor);
            (if result != 0 { 0 } else { 1 }, ControlFlow::Normal)
        }
        Statement::Empty => (0, ControlFlow::Normal),
    }
    })
}

/// Helper for backwards compatibility: Execute and return only exit code
fn execute_statement_for_code<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + 'a>> {
    Box::pin(async move {
        let (code, _) = execute_statement_with_control(stmt, executor).await;
        executor.context_mut().set_last_exit_status(code);
        code
    })
}

/// Old implementation - kept for reference but now uses new version
#[allow(dead_code)]
fn _old_execute_statement_for_code<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + 'a>> {
    Box::pin(async move {
    match stmt {
        Statement::SimpleCommand(cmd) => {
            match executor.execute_simple_command(cmd).await {
                Ok(status) => status.code(),
                Err(e) => {
                    eprintln!("execution error: {}", e);
                    1
                }
            }
        }
        Statement::Pipeline(pipeline) => {
            // Check if it's a single-element pipeline with a simple command
            if pipeline.elements.len() == 1 {
                if let Statement::SimpleCommand(ref cmd) = *pipeline.elements[0].command {
                    match executor.execute_simple_command(cmd).await {
                        Ok(status) => status.code(),
                        Err(e) => {
                            eprintln!("execution error: {}", e);
                            1
                        }
                    }
                } else {
                    // Single-element but not simple command - fall through to pipeline
                    match executor.execute_pipeline(&pipeline).await {
                        Ok(status) => status.code(),
                        Err(e) => {
                            eprintln!("execution error: {}", e);
                            1
                        }
                    }
                }
            } else {
                // Real pipeline with multiple commands
                match executor.execute_pipeline(&pipeline).await {
                    Ok(status) => status.code(),
                    Err(e) => {
                        eprintln!("execution error: {}", e);
                        1
                    }
                }
            }
        }
        Statement::List(list) => {
            use colony_parser::ListOp;

            // Execute first statement
            let mut last_exit_code = execute_statement_for_code(&*list.first, executor).await;

            // Execute rest of list based on operators
            for (op, stmt) in &list.rest {
                let should_execute = match op {
                    ListOp::Semi => true,              // ; always executes
                    ListOp::And => last_exit_code == 0, // && executes if prev succeeded
                    ListOp::Or => last_exit_code != 0,  // || executes if prev failed
                    ListOp::Background => {
                        eprintln!("Background jobs (&) not yet supported");
                        false
                    }
                };

                if should_execute {
                    last_exit_code = execute_statement_for_code(stmt, executor).await;
                }
            }

            last_exit_code
        }
        Statement::For(for_stmt) => {
            execute_for_loop(for_stmt, executor).await
        }
        Statement::CStyleFor(cstyle_for) => {
            execute_cstyle_for_loop(cstyle_for, executor).await
        }
        Statement::If(if_stmt) => {
            execute_if_statement(if_stmt, executor).await
        }
        Statement::While(_) | Statement::Until(_) | Statement::Case(_) => {
            eprintln!("This control flow not yet supported");
            1
        }
        Statement::BraceGroup(brace_group) => {
            use colony_exec::builtin_redirect::BuiltinRedirectGuard;
            let _redirect_guard = match BuiltinRedirectGuard::apply(&brace_group.redirects, executor.context_mut()) {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("colony: redirect error: {}", e);
                    return 1;
                }
            };
            let mut last_exit = 0;
            for stmt in &brace_group.body {
                last_exit = Box::pin(execute_statement_for_code(stmt, executor)).await;
            }
            last_exit
        }
        Statement::Subshell(subshell) => {
            use colony_exec::builtin_redirect::BuiltinRedirectGuard;
            let _redirect_guard = match BuiltinRedirectGuard::apply(&subshell.redirects, executor.context_mut()) {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("colony: redirect error: {}", e);
                    return 1;
                }
            };
            let mut last_exit = 0;
            for stmt in &subshell.body {
                last_exit = Box::pin(execute_statement_for_code(stmt, executor)).await;
            }
            last_exit
        }
        Statement::FunctionDef(func_def) => {
            // Store function definition
            executor.context_mut().define_function(func_def.name.clone(), func_def.clone());
            0
        }
        Statement::Return(_ret_stmt) => {
            // Return statement outside function - just return 0
            0
        }
        Statement::VarDecl(_) | Statement::ProcDef(_) => {
            eprintln!("This declaration not yet supported");
            1
        }
        Statement::Conditional(cond_cmd) => {
            if evaluate_conditional(&cond_cmd.expr, executor) { 0 } else { 1 }
        }
        Statement::Arithmetic(arith_cmd) => {
            if evaluate_arithmetic_command(&arith_cmd.expr, executor) != 0 { 0 } else { 1 }
        }
        Statement::Empty => 0,
    }
    })
}

/// Execute a shell command and return its exit code (for -c mode)
async fn execute_command_with_status(line: &str, executor: &mut Executor) -> i32 {
    use colony_parser::pest_api::parse;

    let program = match parse(line) {
        Ok(program) => program,
        Err(e) => {
            eprintln!("parse error: {}", e);
            return 2;
        }
    };

    let mut last_exit = 0;
    for stmt in &program.statements {
        let (code, flow) = execute_statement_with_control(stmt, executor).await;
        last_exit = code;
        if flow == ControlFlow::Exit {
            return code;
        }
    }
    last_exit
}

async fn execute_shell_command(line: &str, executor: &mut Executor) -> Result<()> {
    let stmt = parse_statement(line)
        .map_err(|e| miette::miette!("parse error: {}", e))?;

    let exit_code = execute_statement_for_code(&stmt, executor).await;
    executor.context_mut().set_last_exit_status(exit_code);
    
    if exit_code != 0 {
        eprintln!("Command exited with code {}", exit_code);
    }

    Ok(())
}

/// Handle `llm` command for model management
///
/// Subcommands:
/// - `llm status` - Show current provider and model
/// - `llm list` - List available providers
/// - `llm use <provider>:<model>` - Switch provider/model
async fn handle_llm_command(line: &str, agent: &mut Option<Agent>) {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 2 {
        eprintln!("Usage: llm <status|list|use>");
        return;
    }

    let subcommand = parts[1];

    match subcommand {
        "status" => {
            if let Some(ref agent) = agent {
                let (provider, model) = agent.current_provider();
                println!("Current provider: {:?}", provider);
                println!("Current model: {}", model);
            } else {
                println!("Agent not initialized");
            }
        }

        "list" => {
            use colony_agent::ModelRouter;

            println!("Available providers:");
            let providers = ModelRouter::list_available_providers().await;

            for info in providers {
                let status = if info.available { "✓" } else { "✗" };
                println!(
                    "  {} {:?}: {}",
                    status, info.provider_type, info.model
                );
                if let Some(reason) = info.reason {
                    println!("      {}", reason);
                }
            }
        }

        "use" => {
            if parts.len() < 3 {
                eprintln!("Usage: llm use <provider>:<model>");
                eprintln!("Example: llm use anthropic:claude-3-5-sonnet-20241022");
                eprintln!("Example: llm use openai:gpt-4-turbo");
                eprintln!("Example: llm use ollama:llama3");
                return;
            }

            // Parse provider:model
            let spec = parts[2];
            let spec_parts: Vec<&str> = spec.split(':').collect();

            if spec_parts.len() != 2 {
                eprintln!("Invalid format. Use: provider:model");
                return;
            }

            let provider_str = spec_parts[0];
            let model_name = spec_parts[1];

            let provider_type = match colony_agent::ProviderType::from_str(provider_str) {
                Some(p) => p,
                None => {
                    eprintln!("Unknown provider: {}", provider_str);
                    eprintln!("Available: anthropic, openai, ollama");
                    return;
                }
            };

            if let Some(ref mut agent) = agent {
                match agent.switch_model(provider_type, model_name).await {
                    Ok(()) => {
                        println!("Switched to {:?}: {}", provider_type, model_name);
                    }
                    Err(e) => {
                        eprintln!("Failed to switch model: {}", e);
                    }
                }
            } else {
                eprintln!("Agent not initialized - cannot switch model");
            }
        }

        _ => {
            eprintln!("Unknown subcommand: {}", subcommand);
            eprintln!("Available: status, list, use");
        }
    }
}

/// Display reactive tuple activity from the tuple space
///
/// Reads and displays env_change and dir_change tuples to show
/// the reactive environment in action (Rules 42-47)
fn display_tuple_activity(executor: &Executor) {
    let tuple_space = executor.tuple_space();

    // Check for env_change tuples: ("env_change", var_name, old_value, new_value)
    let env_pattern = Tuple::pattern(&[
        Value::Str("env_change".to_string()),
        Value::Wildcard,
        Value::Wildcard,
        Value::Wildcard,
    ]);

    while let Some(tuple) = tuple_space.inp(env_pattern.clone()) {
        if let (Some(Value::Str(var)), Some(Value::Str(old)), Some(new)) =
            (tuple.fields.get(1), tuple.fields.get(2), tuple.fields.get(3))
        {
            match new {
                Value::Null => {
                    println!("🔄 Environment: unset {}", var);
                }
                Value::Str(new_val) => {
                    if old.is_empty() {
                        println!("🔄 Environment: {} = {}", var, new_val);
                    } else {
                        println!("🔄 Environment: {} = {} (was: {})", var, new_val, old);
                    }
                }
                _ => {}
            }
        }
    }

    // Check for dir_change tuples: ("dir_change", old_dir, new_dir)
    let dir_pattern = Tuple::pattern(&[
        Value::Str("dir_change".to_string()),
        Value::Wildcard,
        Value::Wildcard,
    ]);

    while let Some(tuple) = tuple_space.inp(dir_pattern.clone()) {
        if let (Some(Value::Str(_old)), Some(Value::Str(new))) =
            (tuple.fields.get(1), tuple.fields.get(2))
        {
            println!("🔄 Directory: {}", new);
        }
    }
}
