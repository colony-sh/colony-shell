//! Colony Shell - POSIX-compliant shell for AI-native terminal

use colony_agent::{Agent, AgentConfig, NaturalLanguageDetector};
use colony_config::{discover_config, ColonyConfig, ConfigLocation, TrustManager, TrustStatus};
use colony_exec::Executor;
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

    // Check for -c flag (execute command and exit)
    if args.len() >= 3 && args[1] == "-c" {
        // Execute inline command
        let command = &args[2];

        // Initialize executor (no agent needed for -c mode)
        let context = ExpansionContext::new();
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
                Err(e) => {
                    eprintln!("execution error: {}", e);
                    (1, ControlFlow::Normal)
                }
            }
        }
        Statement::Pipeline(pipeline) => {
            // Check for simple command in single-element pipeline
            if pipeline.elements.len() == 1 {
                if let Statement::SimpleCommand(ref cmd) = *pipeline.elements[0].command {
                    if let Some(literal) = cmd.name.as_literal() {
                        match literal {
                            "break" => return (0, ControlFlow::Break),
                            "continue" => return (0, ControlFlow::Continue),
                            _ => {}
                        }
                    }

                    match executor.execute_simple_command(cmd).await {
                        Ok(status) => (status.code(), ControlFlow::Normal),
                        Err(e) => {
                            eprintln!("execution error: {}", e);
                            (1, ControlFlow::Normal)
                        }
                    }
                } else {
                    match executor.execute_pipeline(&pipeline).await {
                        Ok(status) => (status.code(), ControlFlow::Normal),
                        Err(e) => {
                            eprintln!("execution error: {}", e);
                            (1, ControlFlow::Normal)
                        }
                    }
                }
            } else {
                match executor.execute_pipeline(&pipeline).await {
                    Ok(status) => (status.code(), ControlFlow::Normal),
                    Err(e) => {
                        eprintln!("execution error: {}", e);
                        (1, ControlFlow::Normal)
                    }
                }
            }
        }
        Statement::List(list) => {
            execute_list_with_control(list, executor).await
        }
        Statement::For(for_stmt) => {
            (execute_for_loop_with_control(for_stmt, executor).await, ControlFlow::Normal)
        }
        Statement::If(if_stmt) => {
            execute_if_with_control(if_stmt, executor).await
        }
        Statement::While(_) | Statement::Until(_) | Statement::Case(_) => {
            eprintln!("This control flow not yet supported");
            (1, ControlFlow::Normal)
        }
        Statement::BraceGroup(_) | Statement::Subshell(_) => {
            eprintln!("Compound commands not yet supported");
            (1, ControlFlow::Normal)
        }
        Statement::VarDecl(_) | Statement::FunctionDef(_) | Statement::ProcDef(_) | Statement::Return(_) => {
            eprintln!("Declarations not yet supported");
            (1, ControlFlow::Normal)
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
        Statement::If(if_stmt) => {
            execute_if_statement(if_stmt, executor).await
        }
        Statement::While(_) | Statement::Until(_) | Statement::Case(_) => {
            eprintln!("This control flow not yet supported");
            1
        }
        Statement::BraceGroup(_) | Statement::Subshell(_) => {
            eprintln!("Compound commands not yet supported");
            1
        }
        Statement::VarDecl(_) | Statement::FunctionDef(_) | Statement::ProcDef(_) | Statement::Return(_) => {
            eprintln!("Declarations not yet supported");
            1
        }
        Statement::Empty => 0,
    }
    })
}

/// Execute a shell command and return its exit code (for -c mode)
async fn execute_command_with_status(line: &str, executor: &mut Executor) -> i32 {
    // Parse with pest - use parse() not parse_statement() to handle multiple statements
    use colony_parser::pest_api::parse;

    let program = match parse(line) {
        Ok(program) => program,
        Err(e) => {
            eprintln!("parse error: {}", e);
            return 2; // Parse error exit code
        }
    };

    // Execute all statements in sequence
    let mut last_exit = 0;
    for stmt in &program.statements {
        last_exit = execute_statement_for_code(stmt, executor).await;
    }
    last_exit
}

async fn execute_shell_command(line: &str, executor: &mut Executor) -> Result<()> {
    // Parse with pest (pure PEG - no lexer needed!)
    let stmt = parse_statement(line)
        .map_err(|e| miette::miette!("parse error: {}", e))?;

    // Execute the parsed command
    use colony_parser::Statement;
    match stmt {
        Statement::SimpleCommand(cmd) => {
            let status = executor.execute_simple_command(&cmd).await
                .map_err(|e| miette::miette!("execution error: {}", e))?;

            if !status.is_success() {
                eprintln!("Command exited with code {}", status.code());
            }
        }
        Statement::Pipeline(pipeline) => {
            let status = executor.execute_pipeline(&pipeline).await
                .map_err(|e| miette::miette!("execution error: {}", e))?;

            if !status.is_success() {
                eprintln!("Pipeline exited with code {}", status.code());
            }
        }
        Statement::List(list) => {
            // List with single element
            if list.rest.is_empty() {
                match *list.first {
                    Statement::SimpleCommand(cmd) => {
                        let status = executor.execute_simple_command(&cmd).await
                            .map_err(|e| miette::miette!("execution error: {}", e))?;

                        if !status.is_success() {
                            eprintln!("Command exited with code {}", status.code());
                        }
                    }
                    Statement::Pipeline(ref pipeline) => {
                        // Check if it's a single-element pipeline with a simple command
                        if pipeline.elements.len() == 1 {
                            if let Statement::SimpleCommand(ref cmd) = *pipeline.elements[0].command {
                                // Execute as simple command to check for builtins
                                let status = executor.execute_simple_command(cmd).await
                                    .map_err(|e| miette::miette!("execution error: {}", e))?;

                                if !status.is_success() {
                                    eprintln!("Command exited with code {}", status.code());
                                }
                            } else {
                                println!("(Pipeline element not a simple command)");
                            }
                        } else {
                            // Real pipeline with multiple commands
                            let status = executor.execute_pipeline(&pipeline).await
                                .map_err(|e| miette::miette!("execution error: {}", e))?;

                            if !status.is_success() {
                                eprintln!("Pipeline exited with code {}", status.code());
                            }
                        }
                    }
                    _ => {
                        println!("(This command type not yet supported)");
                    }
                }
            } else {
                println!("(Command lists with && || not yet supported)");
            }
        }
        Statement::If(_) | Statement::While(_) | Statement::Until(_) | Statement::For(_) | Statement::Case(_) => {
            println!("(Control flow not yet supported)");
        }
        Statement::BraceGroup(_) | Statement::Subshell(_) => {
            println!("(Compound commands not yet supported)");
        }
        Statement::VarDecl(_) | Statement::FunctionDef(_) | Statement::ProcDef(_) | Statement::Return(_) => {
            println!("(Declarations not yet supported)");
        }
        Statement::Empty => {
            // Empty statement - do nothing
        }
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
