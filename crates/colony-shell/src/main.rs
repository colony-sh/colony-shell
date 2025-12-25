//! Colony Shell - POSIX-compliant shell for AI-native terminal

mod tutorial;

use colony_agent::{Agent, AgentConfig, NaturalLanguageDetector};
use colony_config::{discover_config, ColonyConfig, TrustManager, TrustStatus};
use colony_exec::{Executor, ExecutionError};
use colony_expand::{ExpansionContext, TrapSignal, TrapAction};
use colony_parser::pest_api::parse_statement;
use colony_parser::Statement;
use colony_tree_sitter::parse;
use colony_tuple::{Tuple, Value};
use miette::Result;
use std::io::{self, Write};
use std::path::PathBuf;

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

    let use_caching = true;
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

/// Shell startup options (bash-compatible)
#[derive(Debug, Default)]
struct ShellOptions {
    /// Force login shell behavior (--login, -l)
    login: bool,
    /// Don't read /etc/profile or personal profile files (--noprofile)
    noprofile: bool,
    /// Don't read ~/.bashrc (--norc)
    norc: bool,
    /// Use FILE instead of ~/.bashrc (--rcfile FILE, --init-file FILE)
    rcfile: Option<PathBuf>,
    /// Command to execute (-c)
    command: Option<String>,
    /// Script file to execute
    script: Option<PathBuf>,
    /// Positional parameters (arguments after -c command or script)
    positional_args: Vec<String>,
    /// Verbose mode - show which files are sourced (--verbose)
    verbose: bool,
    /// Tutorial mode - X-ray into script execution (--tutorial, -T)
    tutorial: bool,
    /// Teach mode - explain concepts on first occurrence (--teach)
    teach: bool,
    /// Trace depth level 2=mechanics, 3=internals (--depth)
    depth: u8,
}

impl ShellOptions {
    /// Parse command-line arguments
    fn parse() -> Self {
        let raw_args: Vec<String> = std::env::args().collect();
        let mut opts = ShellOptions::default();
        opts.depth = 2;
        
        // Check if invoked as login shell (argv[0] starts with '-')
        if let Some(argv0) = raw_args.first() {
            if let Some(basename) = std::path::Path::new(argv0).file_name() {
                if basename.to_str().map(|s| s.starts_with('-')).unwrap_or(false) {
                    opts.login = true;
                }
            }
        }
        
        let mut i = 1;
        while i < raw_args.len() {
            let arg = &raw_args[i];
            match arg.as_str() {
                "--login" | "-l" => opts.login = true,
                "--noprofile" => opts.noprofile = true,
                "--norc" => opts.norc = true,
                "--verbose" => opts.verbose = true,
                "--tutorial" | "-T" => opts.tutorial = true,
                "--teach" => opts.teach = true,
                "--depth" => {
                    i += 1;
                    if i < raw_args.len() {
                        opts.depth = raw_args[i].parse().unwrap_or(2);
                    }
                }
                "--rcfile" | "--init-file" => {
                    i += 1;
                    if i < raw_args.len() {
                        opts.rcfile = Some(PathBuf::from(&raw_args[i]));
                    } else {
                        eprintln!("colony: {}: option requires an argument", arg);
                        std::process::exit(2);
                    }
                }
                "-c" => {
                    i += 1;
                    if i < raw_args.len() {
                        opts.command = Some(raw_args[i].clone());
                        // Rest are positional args
                        if i + 1 < raw_args.len() {
                            opts.positional_args = raw_args[i + 1..].to_vec();
                        }
                        break;
                    } else {
                        eprintln!("colony: -c: option requires an argument");
                        std::process::exit(2);
                    }
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                "--version" | "-V" => {
                    println!("Colony Shell v{}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                arg if arg.starts_with('-') => {
                    // Handle combined short options like -lc
                    let chars: Vec<char> = arg[1..].chars().collect();
                    let mut j = 0;
                    while j < chars.len() {
                        match chars[j] {
                            'l' => opts.login = true,
                            'c' => {
                                // -c needs an argument
                                if j + 1 < chars.len() {
                                    // Rest of this arg is the command
                                    opts.command = Some(chars[j+1..].iter().collect());
                                } else {
                                    // Next arg is the command
                                    i += 1;
                                    if i < raw_args.len() {
                                        opts.command = Some(raw_args[i].clone());
                                        if i + 1 < raw_args.len() {
                                            opts.positional_args = raw_args[i + 1..].to_vec();
                                        }
                                    } else {
                                        eprintln!("colony: -c: option requires an argument");
                                        std::process::exit(2);
                                    }
                                }
                                break;
                            }
                            _ => {
                                eprintln!("colony: -{}: invalid option", chars[j]);
                                std::process::exit(2);
                            }
                        }
                        j += 1;
                    }
                    if opts.command.is_some() {
                        break;
                    }
                }
                _ => {
                    // First non-option is script file
                    opts.script = Some(PathBuf::from(arg));
                    if i + 1 < raw_args.len() {
                        opts.positional_args = raw_args[i + 1..].to_vec();
                    }
                    break;
                }
            }
            i += 1;
        }
        
        opts
    }
    
    fn is_interactive(&self) -> bool {
        self.command.is_none() && self.script.is_none() && unsafe { libc::isatty(0) != 0 }
    }
}

fn print_help() {
    println!("Colony Shell v{}", env!("CARGO_PKG_VERSION"));
    println!("POSIX-compliant shell with AI agent integration");
    println!();
    println!("Usage: colony [options] [script [arguments]]");
    println!("       colony [options] -c command [arguments]");
    println!();
    println!("Options:");
    println!("  -c STRING         Execute commands from STRING");
    println!("  -l, --login       Force login shell behavior");
    println!("  --noprofile       Don't read /etc/profile or ~/.profile");
    println!("  --norc            Don't read ~/.bashrc");
    println!("  --rcfile FILE     Use FILE instead of ~/.bashrc");
    println!("  --init-file FILE  Same as --rcfile");
    println!("  --verbose         Show which startup files are sourced");
    println!("  -T, --tutorial    Run in tutorial mode (Script X-Ray)");
    println!("  --teach           Enable teach mode (explain concepts)");
    println!("  --depth LEVEL     Trace depth: 2=mechanics, 3=internals");
    println!("  -h, --help        Show this help message");
    println!("  -V, --version     Show version information");
    println!();
    println!("Startup files (login shell):");
    println!("  1. /etc/profile");
    println!("  2. ~/.bash_profile, ~/.bash_login, or ~/.profile (first found)");
    println!();
    println!("Startup files (interactive non-login):");
    println!("  1. /etc/bash.bashrc (if exists)");
    println!("  2. ~/.bashrc");
    println!();
    println!("Tutorial mode controls:");
    println!("  [space]  Pause/resume playback");
    println!("  [enter]  Single step");
    println!("  [d]      Toggle depth level");
    println!("  [t]      Toggle teach mode");
    println!("  [m]      Mark current line");
    println!("  [q]      Quit");
    println!();
    println!("Environment variables:");
    println!("  BASH_ENV  File to source for non-interactive shells");
    println!("  ENV       Same as BASH_ENV (POSIX)");
}

/// Expand tilde in a path (~ to home directory)
fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(&path[2..]);
        }
    } else if path == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(path)
}

/// Source a startup file if it exists
async fn maybe_source_file(path: &str, executor: &mut Executor, verbose: bool) -> bool {
    let expanded = expand_tilde(path);
    
    if expanded.is_file() {
        if verbose {
            eprintln!("colony: sourcing {}", expanded.display());
        }
        match executor.execute_source_file(&expanded, vec![]).await {
            Ok(_) => true,
            Err(e) => {
                eprintln!("colony: error sourcing {}: {}", expanded.display(), e);
                false
            }
        }
    } else {
        false
    }
}

fn load_exported_functions(context: &mut ExpansionContext) {
    const BASHFUNC_PREFIX: &str = "BASH_FUNC_";
    const BASHFUNC_SUFFIX: &str = "%%";
    
    for (key, value) in std::env::vars() {
        if key.starts_with(BASHFUNC_PREFIX) && key.ends_with(BASHFUNC_SUFFIX) {
            let name_start = BASHFUNC_PREFIX.len();
            let name_end = key.len() - BASHFUNC_SUFFIX.len();
            let func_name = &key[name_start..name_end];
            
            if !value.starts_with("() {") {
                continue;
            }
            
            let func_source = format!("{} {}", func_name, value);
            
            if let Ok(stmt) = parse_statement(&func_source) {
                if let Statement::FunctionDef(func) = stmt {
                    context.define_function(func.name.clone(), func);
                    context.export_function(&func_name);
                }
            }
        }
    }
}

/// Run startup files based on shell mode (login/interactive)
async fn run_startup_files(opts: &ShellOptions, executor: &mut Executor) {
    let is_login = opts.login;
    let is_interactive = opts.is_interactive();
    let verbose = opts.verbose;
    
    if verbose {
        eprintln!("colony: login={}, interactive={}", is_login, is_interactive);
    }
    
    // Login shell startup files
    if is_login && !opts.noprofile {
        // 1. System-wide profile
        maybe_source_file("/etc/profile", executor, verbose).await;
        
        // 2. User profile - try in order, use first found
        if !maybe_source_file("~/.bash_profile", executor, verbose).await {
            if !maybe_source_file("~/.bash_login", executor, verbose).await {
                maybe_source_file("~/.profile", executor, verbose).await;
            }
        }
    }
    
    // Interactive non-login shell startup files
    if is_interactive && !is_login && !opts.norc {
        if let Some(ref rcfile) = opts.rcfile {
            // Custom rcfile specified
            if verbose {
                eprintln!("colony: sourcing {}", rcfile.display());
            }
            if rcfile.is_file() {
                if let Err(e) = executor.execute_source_file(rcfile, vec![]).await {
                    eprintln!("colony: error sourcing {}: {}", rcfile.display(), e);
                }
            } else {
                eprintln!("colony: {}: No such file or directory", rcfile.display());
            }
        } else {
            // Default rc files
            // Some systems have /etc/bash.bashrc
            maybe_source_file("/etc/bash.bashrc", executor, verbose).await;
            maybe_source_file("~/.bashrc", executor, verbose).await;
        }
    }
    
    // Non-interactive shell (script or -c command)
    if !is_interactive {
        // Only process BASH_ENV or ENV if set
        if let Ok(env_file) = std::env::var("BASH_ENV") {
            if !env_file.is_empty() {
                maybe_source_file(&env_file, executor, verbose).await;
            }
        } else if let Ok(env_file) = std::env::var("ENV") {
            if !env_file.is_empty() {
                maybe_source_file(&env_file, executor, verbose).await;
            }
        }
    }
    
    // Set marker to prevent re-sourcing
    executor.context_mut().set_var("COLONY_STARTUP_SOURCED".to_string(), "1".to_string());
}

#[tokio::main]
async fn main() -> Result<()> {
    // Ignore SIGPIPE so writes to broken pipes return EPIPE instead of killing process
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command-line arguments
    let opts = ShellOptions::parse();

    // Handle -c command mode
    if let Some(ref command) = opts.command {
        let mut context = ExpansionContext::new();
        load_exported_functions(&mut context);
        if !opts.positional_args.is_empty() {
            context.set_positional_params(opts.positional_args.clone());
        }

        let mut executor = Executor::new(context);
        
        // Source BASH_ENV for non-interactive shells
        run_startup_files(&opts, &mut executor).await;

        // Execute the command and exit with its status
        let exit_code = execute_command_with_status(command, &mut executor).await;
        std::process::exit(exit_code);
    }
    
    // Handle script execution mode
    if let Some(ref script) = opts.script {
        if opts.tutorial {
            use tutorial::{TutorialConfig, TutorialRunner};
            use tutorial::tracer::TraceDepth;
            
            let depth = match opts.depth {
                3 => TraceDepth::Internals,
                _ => TraceDepth::Mechanics,
            };
            
            let config = TutorialConfig {
                depth,
                teach_mode: opts.teach,
                pause_on_complex: true,
            };
            
            let mut runner = match TutorialRunner::new(script, config) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("colony: tutorial error: {}", e);
                    std::process::exit(1);
                }
            };
            
            match runner.run() {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("colony: tutorial error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        
        let mut context = ExpansionContext::new();
        load_exported_functions(&mut context);
        context.set_shell_name(script.display().to_string());
        context.set_positional_params(opts.positional_args.clone());

        let mut executor = Executor::new(context);
        
        // Source BASH_ENV for non-interactive shells
        run_startup_files(&opts, &mut executor).await;

        // Execute the script
        match executor.execute_source_file(script, vec![]).await {
            Ok(status) => std::process::exit(status.code()),
            Err(e) => {
                eprintln!("colony: {}: {}", script.display(), e);
                std::process::exit(127);
            }
        }
    }

    // Interactive REPL mode
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
    let mut context = ExpansionContext::new();
    load_exported_functions(&mut context);
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
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        s[1..s.len()-1].to_string()
    } else if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        expand_double_quoted(&s[1..s.len()-1], executor)
    } else if s.starts_with('$') {
        expand_variable_ref(s, executor)
    } else {
        s.to_string()
    }
}

fn expand_double_quoted(content: &str, executor: &Executor) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut var_str = String::from("$");
            if chars.peek() == Some(&'{') {
                var_str.push(chars.next().unwrap());
                while let Some(&ch) = chars.peek() {
                    var_str.push(chars.next().unwrap());
                    if ch == '}' {
                        break;
                    }
                }
            } else if chars.peek() == Some(&'(') {
                var_str.push(chars.next().unwrap());
                let mut depth = 1;
                while depth > 0 {
                    if let Some(ch) = chars.next() {
                        var_str.push(ch);
                        if ch == '(' { depth += 1; }
                        if ch == ')' { depth -= 1; }
                    } else {
                        break;
                    }
                }
            } else {
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '[' || ch == ']' {
                        var_str.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
            }
            result.push_str(&expand_variable_ref(&var_str, executor));
        } else if c == '\\' {
            if let Some(escaped) = chars.next() {
                match escaped {
                    '$' | '"' | '\\' | '`' => result.push(escaped),
                    _ => {
                        result.push('\\');
                        result.push(escaped);
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    
    result
}

fn expand_variable_ref(s: &str, executor: &Executor) -> String {
    use colony_expand::Variable;
    
    if !s.starts_with('$') {
        return s.to_string();
    }
    
    let content = if s.starts_with("${") && s.ends_with('}') {
        &s[2..s.len()-1]
    } else {
        &s[1..]
    };
    
    if let Some(bracket_pos) = content.find('[') {
        if let Some(end_pos) = content.find(']') {
            let var_name = &content[..bracket_pos];
            let index_str = &content[bracket_pos+1..end_pos];
            
            if let Some(Variable::Array(arr)) = executor.context().get_variable(var_name) {
                if let Ok(index) = index_str.parse::<usize>() {
                    return arr.get(index).cloned().unwrap_or_default();
                } else if index_str == "@" || index_str == "*" {
                    return arr.join(" ");
                }
            }
            return String::new();
        }
    }
    
    executor.context().get_var_or_env(content).unwrap_or_default()
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

fn evaluate_arithmetic_command(expr: &str, executor: &mut Executor) -> i64 {
    match colony_expand::expand_arithmetic(expr, executor.context_mut()) {
        Ok(result) => result.parse().unwrap_or(0),
        Err(_) => 0,
    }
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

            let result = match executor.execute_simple_command(cmd).await {
                Ok(status) => (status.code(), ControlFlow::Normal),
                Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                Err(e) => {
                    eprintln!("execution error: {}", e);
                    (1, ControlFlow::Normal)
                }
            };
            if result.0 != 0 && result.1 == ControlFlow::Normal {
                execute_err_trap(executor).await;
            }
            result
        }
        Statement::Pipeline(pipeline) => {
            if pipeline.elements.len() == 1 {
                let element = &pipeline.elements[0];
                let (mut code, flow) = execute_statement_with_control(&element.command, executor).await;
                if element.negated {
                    code = if code == 0 { 1 } else { 0 };
                }
                executor.context_mut().set_last_exit_status(code);
                return (code, flow);
            }
            let result = match executor.execute_pipeline(&pipeline).await {
                Ok(status) => (status.code(), ControlFlow::Normal),
                Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                Err(e) => {
                    eprintln!("execution error: {}", e);
                    (1, ControlFlow::Normal)
                }
            };
            if result.0 != 0 && result.1 == ControlFlow::Normal {
                execute_err_trap(executor).await;
            }
            result
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
            let mut subshell_executor = Executor::new(executor.context().clone());
            let _redirect_guard = match BuiltinRedirectGuard::apply(&subshell.redirects, subshell_executor.context_mut()) {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("colony: redirect error: {}", e);
                    return (1, ControlFlow::Normal);
                }
            };
            let mut last_exit = 0;
            for stmt in &subshell.body {
                let (exit_code, flow) = execute_statement_with_control(stmt, &mut subshell_executor).await;
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
        Statement::Coproc(coproc) => {
            match execute_coproc(coproc, executor).await {
                Ok(exit_code) => (exit_code, ControlFlow::Normal),
                Err(e) => {
                    eprintln!("colony: coproc error: {}", e);
                    (1, ControlFlow::Normal)
                }
            }
        }
        Statement::Empty => (executor.context().last_exit_status(), ControlFlow::Normal),
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
            let element = &pipeline.elements[0];
            let is_negated = pipeline.elements.last().map(|e| e.negated).unwrap_or(false);
            
            if pipeline.elements.len() == 1 {
                let mut code = if let Statement::SimpleCommand(ref cmd) = *element.command {
                    match executor.execute_simple_command(cmd).await {
                        Ok(status) => status.code(),
                        Err(e) => {
                            eprintln!("execution error: {}", e);
                            1
                        }
                    }
                } else {
                    match executor.execute_pipeline(&pipeline).await {
                        Ok(status) => status.code(),
                        Err(e) => {
                            eprintln!("execution error: {}", e);
                            1
                        }
                    }
                };
                if is_negated {
                    code = if code == 0 { 1 } else { 0 };
                }
                code
            } else {
                let mut code = match executor.execute_pipeline(&pipeline).await {
                    Ok(status) => status.code(),
                    Err(e) => {
                        eprintln!("execution error: {}", e);
                        1
                    }
                };
                if is_negated {
                    code = if code == 0 { 1 } else { 0 };
                }
                code
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
        Statement::Coproc(coproc) => {
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    handle.block_on(async {
                        execute_coproc(coproc, executor).await.unwrap_or(1)
                    })
                }
                Err(_) => {
                    eprintln!("colony: coproc requires async runtime");
                    1
                }
            }
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
        executor.context_mut().set_last_exit_status(code);
        if flow == ControlFlow::Exit {
            if let Some(trap_code) = execute_exit_trap(executor, code).await {
                return trap_code;
            }
            return code;
        }
    }
    
    if let Some(trap_code) = execute_exit_trap(executor, last_exit).await {
        return trap_code;
    }
    last_exit
}

async fn execute_exit_trap(executor: &mut Executor, _exit_code: i32) -> Option<i32> {
    use colony_parser::pest_api::parse;
    
    if let Some(TrapAction::Command(cmd)) = executor.context().get_trap(&TrapSignal::Exit).cloned() {
        if let Ok(program) = parse(&cmd) {
            for stmt in &program.statements {
                let (code, flow) = execute_statement_with_control(&stmt, executor).await;
                if flow == ControlFlow::Exit {
                    return Some(code);
                }
            }
        }
    }
    None
}

async fn execute_err_trap(executor: &mut Executor) {
    use colony_parser::pest_api::parse;
    
    if let Some(TrapAction::Command(cmd)) = executor.context().get_trap(&TrapSignal::Err).cloned() {
        if let Ok(program) = parse(&cmd) {
            for stmt in &program.statements {
                let _ = execute_statement_for_code(&stmt, executor).await;
            }
        }
    }
}

/// Serialize a Statement to a shell command string for child process execution
fn statement_to_shell_string(stmt: &colony_parser::Statement) -> String {
    use colony_parser::{Statement, Word, WordPart, VariableRef};
    
    fn varref_to_string(v: &VariableRef) -> String {
        let mut result = String::from("$");
        if v.modifier.is_some() || v.subscript.is_some() || v.prefix.is_some() {
            result.push('{');
        }
        result.push_str(&v.name);
        if v.modifier.is_some() || v.subscript.is_some() || v.prefix.is_some() {
            result.push('}');
        }
        result
    }
    
    fn word_to_string(word: &Word) -> String {
        let mut result = String::new();
        for part in &word.parts {
            match part {
                WordPart::Literal(s) => {
                    for c in s.chars() {
                        if " \t\n\"'$`\\!&|;<>(){}[]".contains(c) {
                            result.push('\\');
                        }
                        result.push(c);
                    }
                }
                WordPart::SingleQuoted(s) => {
                    result.push('\'');
                    result.push_str(&s.replace('\'', "'\\''"));
                    result.push('\'');
                }
                WordPart::DoubleQuoted(parts) => {
                    result.push('"');
                    for p in parts {
                        match p {
                            WordPart::Literal(s) => {
                                for c in s.chars() {
                                    if "\"$`\\".contains(c) {
                                        result.push('\\');
                                    }
                                    result.push(c);
                                }
                            }
                            WordPart::Variable(v) => {
                                result.push_str(&varref_to_string(v));
                            }
                            _ => result.push_str(&format!("{:?}", p)),
                        }
                    }
                    result.push('"');
                }
                WordPart::Variable(v) => {
                    result.push_str(&varref_to_string(v));
                }
                WordPart::Tilde(user) => {
                    result.push('~');
                    if let Some(u) = user {
                        result.push_str(u);
                    }
                }
                WordPart::CommandSubRaw(s) => {
                    result.push_str("$(");
                    result.push_str(s);
                    result.push(')');
                }
                WordPart::ArithRaw(s) => {
                    result.push_str("$((");
                    result.push_str(s);
                    result.push_str("))");
                }
                _ => result.push_str(&format!("{:?}", part)),
            }
        }
        result
    }
    
    fn simple_cmd_to_string(cmd: &colony_parser::SimpleCommand) -> String {
        let mut parts = Vec::new();
        parts.push(word_to_string(&cmd.name));
        for arg in &cmd.args {
            parts.push(word_to_string(arg));
        }
        parts.join(" ")
    }
    
    fn statements_to_string(stmts: &[Statement]) -> String {
        stmts.iter()
            .map(statement_to_shell_string)
            .collect::<Vec<_>>()
            .join("; ")
    }
    
    fn condition_to_string(cond: &colony_parser::Condition) -> String {
        match cond {
            colony_parser::Condition::Command(stmt) => statement_to_shell_string(stmt),
            colony_parser::Condition::Expression(expr) => format!("{:?}", expr),
        }
    }
    
    match stmt {
        Statement::SimpleCommand(cmd) => simple_cmd_to_string(cmd),
        Statement::Pipeline(pipe) => {
            pipe.elements.iter()
                .map(|elem| statement_to_shell_string(&elem.command))
                .collect::<Vec<_>>()
                .join(" | ")
        }
        Statement::List(list) => {
            let mut result = statement_to_shell_string(&list.first);
            for (op, cmd) in &list.rest {
                let op_str = match op {
                    colony_parser::ListOp::And => "&&",
                    colony_parser::ListOp::Or => "||",
                    colony_parser::ListOp::Semi => ";",
                    colony_parser::ListOp::Background => "&",
                };
                result.push_str(&format!(" {} {}", op_str, statement_to_shell_string(cmd)));
            }
            result
        }
        Statement::BraceGroup(bg) => {
            format!("{{ {}; }}", statements_to_string(&bg.body))
        }
        Statement::Subshell(sub) => {
            format!("( {} )", statements_to_string(&sub.body))
        }
        Statement::If(if_stmt) => {
            let mut result = format!("if {}; then {}", 
                condition_to_string(&if_stmt.condition),
                statements_to_string(&if_stmt.then_body));
            for elif in &if_stmt.elif_branches {
                result.push_str(&format!("; elif {}; then {}",
                    condition_to_string(&elif.condition),
                    statements_to_string(&elif.body)));
            }
            if let Some(else_body) = &if_stmt.else_body {
                result.push_str(&format!("; else {}", statements_to_string(else_body)));
            }
            result.push_str("; fi");
            result
        }
        Statement::While(while_stmt) => {
            format!("while {}; do {}; done",
                condition_to_string(&while_stmt.condition),
                statements_to_string(&while_stmt.body))
        }
        Statement::Until(until_stmt) => {
            format!("until {}; do {}; done",
                condition_to_string(&until_stmt.condition),
                statements_to_string(&until_stmt.body))
        }
        Statement::For(for_stmt) => {
            let words: Vec<String> = for_stmt.words.iter()
                .map(word_to_string)
                .collect();
            format!("for {} in {}; do {}; done",
                for_stmt.var,
                words.join(" "),
                statements_to_string(&for_stmt.body))
        }
        Statement::Case(case_stmt) => {
            let word = word_to_string(&case_stmt.value);
            let mut result = format!("case {} in ", word);
            for arm in &case_stmt.arms {
                let patterns: Vec<String> = arm.patterns.iter()
                    .map(word_to_string)
                    .collect();
                result.push_str(&format!("{}) {};; ", patterns.join("|"), statements_to_string(&arm.body)));
            }
            result.push_str("esac");
            result
        }
        Statement::Empty => String::new(),
        other => format!("{:?}", other),
    }
}

async fn execute_coproc(coproc: &colony_parser::CoprocCommand, executor: &mut Executor) -> Result<i32, String> {
    use nix::unistd::{fork, ForkResult, pipe, dup2, close, execvp};
    use std::os::fd::AsRawFd;
    use std::ffi::CString;
    
    let coproc_name = coproc.name.clone().unwrap_or_else(|| "COPROC".to_string());
    
    let stdin_pipe = pipe().map_err(|e| format!("pipe error: {}", e))?;
    let stdout_pipe = pipe().map_err(|e| format!("pipe error: {}", e))?;
    
    // Determine if we need to run through shell or can exec directly
    let is_simple_external = matches!(coproc.command.as_ref(), colony_parser::Statement::SimpleCommand(cmd) 
        if cmd.name.as_literal().is_some() && !colony_exec::is_builtin(cmd.name.as_literal().unwrap()));
    
    // For compound commands or builtins, serialize to shell string and run through colony
    let shell_cmd = if !is_simple_external {
        Some(statement_to_shell_string(coproc.command.as_ref()))
    } else {
        None
    };
    
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            close(stdin_pipe.1.as_raw_fd()).ok();
            close(stdout_pipe.0.as_raw_fd()).ok();
            
            dup2(stdin_pipe.0.as_raw_fd(), 0).ok();
            dup2(stdout_pipe.1.as_raw_fd(), 1).ok();
            
            close(stdin_pipe.0.as_raw_fd()).ok();
            close(stdout_pipe.1.as_raw_fd()).ok();
            
            if let Some(cmd_string) = shell_cmd {
                // Run compound command or builtin through colony shell
                let exe_path = std::env::current_exe()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "colony".to_string());
                
                let c_exe = CString::new(exe_path.as_str()).unwrap();
                let c_flag = CString::new("-c").unwrap();
                let c_cmd = CString::new(cmd_string.as_str()).unwrap();
                
                let args: Vec<&std::ffi::CStr> = vec![c_exe.as_c_str(), c_flag.as_c_str(), c_cmd.as_c_str()];
                let _ = execvp(&c_exe, &args);
                std::process::exit(127);
            } else if let colony_parser::Statement::SimpleCommand(cmd) = coproc.command.as_ref() {
                // Simple external command - exec directly for efficiency
                let cmd_name = cmd.name.as_literal().unwrap_or("true");
                let c_cmd = CString::new(cmd_name).unwrap();
                let mut c_args: Vec<CString> = vec![c_cmd.clone()];
                for arg in &cmd.args {
                    if let Some(lit) = arg.as_literal() {
                        c_args.push(CString::new(lit).unwrap());
                    }
                }
                let c_args_ref: Vec<&std::ffi::CStr> = c_args.iter().map(|s| s.as_c_str()).collect();
                let _ = execvp(&c_cmd, &c_args_ref);
                std::process::exit(127);
            } else {
                // Should not reach here due to is_simple_external logic
                std::process::exit(1);
            }
        }
        Ok(ForkResult::Parent { child }) => {
            drop(stdin_pipe.0);
            drop(stdout_pipe.1);
            
            let read_fd = stdout_pipe.0.as_raw_fd();
            let write_fd = stdin_pipe.1.as_raw_fd();
            
            executor.context_mut().set_array_element(&coproc_name, 0, &read_fd.to_string());
            executor.context_mut().set_array_element(&coproc_name, 1, &write_fd.to_string());
            executor.context_mut().set_var(format!("{}_PID", coproc_name), child.as_raw().to_string());
            
            std::mem::forget(stdout_pipe.0);
            std::mem::forget(stdin_pipe.1);
            
            Ok(0)
        }
        Err(e) => Err(format!("fork error: {}", e)),
    }
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
