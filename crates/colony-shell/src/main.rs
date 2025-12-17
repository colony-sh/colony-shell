//! Colony Shell - POSIX-compliant shell for AI-native terminal

use colony_agent::{Agent, AgentConfig, NaturalLanguageDetector};
use colony_exec::Executor;
use colony_expand::ExpansionContext;
use colony_parser::pest_api::parse_statement;
use colony_parser::Statement;
use colony_tree_sitter::parse;
use colony_tuple::{Tuple, Value};
use miette::Result;
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

    // Initialize agent with auto-detected provider
    let agent = match Agent::with_config(AgentConfig {
        system_prompt: "You are Colony Shell's AI assistant. You help users with \
                        shell commands, system administration, and programming tasks."
            .to_string(),
        use_caching: true,
        max_tokens: 4096,
        temperature: None,
    })
    .await
    {
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

/// Helper: Execute any statement and return exit code
fn execute_statement_for_code<'a>(
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
        Statement::If(_) | Statement::While(_) | Statement::Until(_) | Statement::For(_) | Statement::Case(_) => {
            eprintln!("Control flow not yet supported");
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
    // Parse with pest
    let stmt = match parse_statement(line) {
        Ok(stmt) => stmt,
        Err(e) => {
            eprintln!("parse error: {}", e);
            return 2; // Parse error exit code
        }
    };

    // Execute using helper
    execute_statement_for_code(&stmt, executor).await
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
