//! Shell builtin commands
//!
//! Builtins are commands that run within the shell process itself,
//! not as external programs. They can modify shell state (environment,
//! working directory, variables) which external commands cannot do.
//!
//! Essential builtins (POSIX):
//! - cd: Change working directory
//! - export: Set environment variables
//! - unset: Remove environment variables
//! - exit: Terminate the shell
//! - pwd: Print working directory
//! - echo: Print arguments
//! - type: Check command type
//! - alias/unalias: Manage command aliases
//!
//! Rule #14: All public APIs must have doc comments with examples

use crate::{ExecutionError, JobTable, Result};
use colony_channels::ChannelManager;
use colony_expand::{ExpansionContext, Expander, Variable};
use colony_parser::{SimpleCommand, Word};
use colony_tuple::{Tuple, TupleSpace, Value};
use std::env;
use std::path::PathBuf;

/// Result of executing a builtin
#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinResult {
    /// Builtin executed successfully with exit code
    Success(i32),
    /// Builtin failed with exit code
    Failure(i32),
    /// Builtin requested shell exit
    Exit(i32),
}

impl BuiltinResult {
    pub fn exit_code(&self) -> i32 {
        match self {
            BuiltinResult::Success(code) | BuiltinResult::Failure(code) | BuiltinResult::Exit(code) => *code,
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, BuiltinResult::Success(0))
    }

    pub fn is_exit(&self) -> bool {
        matches!(self, BuiltinResult::Exit(_))
    }
}

pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "cd" | "export" | "unset" | "exit" | "pwd" | "echo" | "type"
        | "alias" | "unalias" | "." | "source"
        | "fg" | "bg" | "jobs" | "wait"
        | "[" | "test"
        | ":"
        | "context"
        | "declare"
        | "true" | "false"
        | "read"
        | "history"
        | "recho"
        | "set"
        | "printf"
    )
}

/// Execute a builtin command
///
/// Returns None if the command is not a builtin.
/// Returns Some(BuiltinResult) if it is a builtin.
///
/// Builtins with state changes emit tuples to the tuple space for reactive updates.
pub fn execute_builtin(
    cmd: &SimpleCommand,
    context: &mut ExpansionContext,
    jobs: &JobTable,
    tuple_space: &TupleSpace,
    channels: &mut ChannelManager,
) -> Option<Result<BuiltinResult>> {
    // Expand command name (with command substitution support)
    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
        crate::execute_and_capture(stmt, ctx)
    });
    let name_fields = match expander.expand_word(&cmd.name) {
        Ok(fields) => fields,
        Err(e) => return Some(Err(e.into())),
    };

    if name_fields.is_empty() {
        return None;
    }

    let name = &name_fields[0].text;

    if !is_builtin(name) {
        return None;
    }

    // Expand arguments
    let mut args = Vec::new();
    for (i, arg_word) in cmd.args.iter().enumerate() {
        let fields = match expander.expand_word(arg_word) {
            Ok(fields) => {
                fields
            }
            Err(e) => return Some(Err(e.into())),
        };
        for field in fields {
            args.push(field.text);
        }
    }

    // Dispatch to specific builtin
    let result = match name.as_str() {
        "cd" => builtin_cd(&args, context, tuple_space),
        "export" => builtin_export(&args, context, tuple_space),
        "unset" => builtin_unset(&args, context, tuple_space),
        "exit" => builtin_exit(&args),
        "pwd" => builtin_pwd(&args),
        "echo" => builtin_echo(&args),
        "type" => builtin_type(&args),
        "alias" => builtin_alias(&args, context),
        "unalias" => builtin_unalias(&args, context),
        "." | "source" => builtin_source(&args, context),
        "fg" => builtin_fg(&args, jobs),
        "bg" => builtin_bg(&args, jobs),
        "jobs" => builtin_jobs(&args, jobs),
        "wait" => builtin_wait(&args, jobs),
        "[" | "test" => builtin_test(&args),
        ":" => builtin_noop(&args),
        "context" => builtin_context(&args, channels),
        "declare" => builtin_declare(&args, context),
        "true" => builtin_true(),
        "false" => builtin_false(),
        "read" => builtin_read(&args, context),
        "recho" => builtin_recho(&args),
        "set" => builtin_set(&args, context),
        "printf" => builtin_printf(&args, context),
        _ => unreachable!("is_builtin returned true but no handler"),
    };

    Some(result)
}

/// cd - Change working directory
///
/// Usage: cd [dir]
/// - No args: cd to HOME
/// - One arg: cd to specified directory
///
/// **Reactive:** Emits ("dir_change", old_dir, new_dir) tuple (Rule 46)
fn builtin_cd(args: &[String], context: &mut ExpansionContext, tuple_space: &TupleSpace) -> Result<BuiltinResult> {
    // Capture old directory for reactive tuple
    let old_dir = env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "/".to_string());

    let target = if args.is_empty() {
        // cd with no args goes to HOME
        context
            .get_env("HOME")
            .ok_or_else(|| ExecutionError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "HOME not set",
            )))?
    } else {
        &args[0]
    };

    // Change directory
    env::set_current_dir(target).map_err(|e| {
        ExecutionError::IoError(std::io::Error::new(
            e.kind(),
            format!("cd: {}: {}", target, e),
        ))
    })?;

    // Get new directory
    let new_dir = env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| target.to_string());

    // Update PWD environment variable
    context.set_env("PWD", &new_dir);

    // REACTIVE: Emit dir_change tuple (Rule 46)
    tuple_space.out(Tuple::from(vec![
        Value::Str("dir_change".to_string()),
        Value::Str(old_dir),
        Value::Str(new_dir.clone()),
    ]));

    Ok(BuiltinResult::Success(0))
}

/// export - Set environment variables
///
/// Usage: export VAR=value [VAR2=value2 ...]
///
/// **Reactive:** Emits ("env_change", var, old, new) tuple (Rule 43)
fn builtin_export(args: &[String], context: &mut ExpansionContext, tuple_space: &TupleSpace) -> Result<BuiltinResult> {
    if args.is_empty() {
        // No args: print all exported variables
        for (key, value) in env::vars() {
            println!("export {}={}", key, value);
        }
        return Ok(BuiltinResult::Success(0));
    }

    // Parse VAR=value assignments
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let (name, value) = arg.split_at(eq_pos);
            let value = &value[1..]; // Skip the '='

            // Capture old value for reactive tuple
            let old_value = context.get_env(name).unwrap_or("").to_string();

            context.set_env(name, value);
            env::set_var(name, value);

            // REACTIVE: Emit env_change tuple (Rule 43)
            tuple_space.out(Tuple::from(vec![
                Value::Str("env_change".to_string()),
                Value::Str(name.to_string()),
                Value::Str(old_value),
                Value::Str(value.to_string()),
            ]));
        } else {
            // Just export existing variable
            if let Some(value) = context.get_env(arg) {
                env::set_var(arg, value);
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// unset - Remove environment variables
///
/// Usage: unset VAR [VAR2 ...]
///
/// **Reactive:** Emits ("env_change", var, old, null) tuple (Rule 43)
fn builtin_unset(args: &[String], context: &mut ExpansionContext, tuple_space: &TupleSpace) -> Result<BuiltinResult> {
    for var_name in args {
        // Capture old value for reactive tuple (check both shell vars and env)
        let old_value = context.get_var_or_env(var_name).unwrap_or_default();

        // Unset from shell variables (includes arrays)
        context.unset_variable(var_name);

        // Also unset from environment
        context.unset_env(var_name);
        env::remove_var(var_name);

        // REACTIVE: Emit env_change tuple with Null for deleted (Rule 43)
        tuple_space.out(Tuple::from(vec![
            Value::Str("env_change".to_string()),
            Value::Str(var_name.to_string()),
            Value::Str(old_value),
            Value::Null,
        ]));
    }

    Ok(BuiltinResult::Success(0))
}

/// exit - Exit the shell
///
/// Usage: exit [n]
/// - No args: exit with status 0
/// - One arg: exit with specified status
fn builtin_exit(args: &[String]) -> Result<BuiltinResult> {
    let code = if args.is_empty() {
        0
    } else {
        args[0].parse::<i32>().unwrap_or(255)
    };

    Ok(BuiltinResult::Exit(code))
}

/// pwd - Print working directory
///
/// Usage: pwd
fn builtin_pwd(_args: &[String]) -> Result<BuiltinResult> {
    let cwd = env::current_dir().map_err(|e| {
        ExecutionError::IoError(std::io::Error::new(e.kind(), format!("pwd: {}", e)))
    })?;

    println!("{}", cwd.display());
    Ok(BuiltinResult::Success(0))
}

/// echo - Print arguments
///
/// Usage: echo [args...]
/// Prints arguments separated by spaces, followed by newline
fn builtin_echo(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        println!();
    } else {
        println!("{}", args.join(" "));
    }

    Ok(BuiltinResult::Success(0))
}

/// recho - Really echo args (test helper)
///
/// Usage: recho [args...]
/// Prints each argument on its own line as `argv[N] = <arg>` with control
/// characters made visible. Used for testing word splitting and expansion.
///
/// Control character display:
/// - 0x00-0x1F: ^@ through ^_ (e.g., ^A for 0x01, ^I for tab)
/// - 0x7F: ^?
///
/// If no arguments are provided, prints nothing (not even "nothing").
/// This matches bash's recho behavior for testing $@ expansion.
fn builtin_recho(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        // No arguments - print nothing (matches bash recho behavior)
        return Ok(BuiltinResult::Success(0));
    }

    for (i, arg) in args.iter().enumerate() {
        print!("argv[{}] = <", i + 1);
        for byte in arg.bytes() {
            if byte < 0x20 {
                // Control character 0x00-0x1F -> ^@ through ^_
                print!("^{}", (byte + 64) as char);
            } else if byte == 0x7F {
                // DEL character -> ^?
                print!("^?");
            } else {
                // Normal character
                print!("{}", byte as char);
            }
        }
        println!(">");
    }

    Ok(BuiltinResult::Success(0))
}

fn builtin_set(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    context.set_positional_params(args.to_vec());
    Ok(BuiltinResult::Success(0))
}

/// type - Check command type
///
/// Usage: type command [command...]
fn builtin_type(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("type: missing argument");
        return Ok(BuiltinResult::Failure(1));
    }

    for cmd in args {
        if is_builtin(cmd) {
            println!("{} is a shell builtin", cmd);
        } else {
            // Check if it's in PATH
            match crate::path::resolve_command(cmd, env::var("PATH").ok().as_deref()) {
                Ok(path) => println!("{} is {}", cmd, path.display()),
                Err(_) => {
                    eprintln!("{}: not found", cmd);
                    return Ok(BuiltinResult::Failure(1));
                }
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// alias - Manage command aliases
///
/// Usage:
/// - alias              - List all aliases
/// - alias name         - Show specific alias
/// - alias name='value' - Define alias
///
/// POSIX: Aliases substitute the command name before execution
fn builtin_alias(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() {
        // No args: list all aliases
        let mut aliases: Vec<_> = context.aliases().iter().collect();
        aliases.sort_by_key(|(name, _)| *name);

        for (name, value) in aliases {
            println!("alias {}='{}'", name, value);
        }
        return Ok(BuiltinResult::Success(0));
    }

    // Process each argument
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            // Define alias: name=value
            let (name, value) = arg.split_at(eq_pos);
            let value = &value[1..]; // Skip the '='

            // Remove surrounding quotes if present
            let value = value.trim_matches('\'').trim_matches('"');

            context.set_alias(name.to_string(), value.to_string());
        } else {
            // Show specific alias
            if let Some(value) = context.get_alias(arg) {
                println!("alias {}='{}'", arg, value);
            } else {
                eprintln!("alias: {}: not found", arg);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// unalias - Remove command aliases
///
/// Usage: unalias name [name...]
///
/// Removes the specified aliases from the alias table.
fn builtin_unalias(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("unalias: usage: unalias name [name ...]");
        return Ok(BuiltinResult::Failure(1));
    }

    for name in args {
        if context.get_alias(name).is_some() {
            context.unset_alias(name);
        } else {
            eprintln!("unalias: {}: not found", name);
            return Ok(BuiltinResult::Failure(1));
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// source/. - Execute commands from file
///
/// Usage: source filename
/// TODO: Implement file reading and execution
fn builtin_source(_args: &[String], _context: &mut ExpansionContext) -> Result<BuiltinResult> {
    eprintln!("source: not yet implemented");
    Ok(BuiltinResult::Failure(1))
}

/// fg - Bring job to foreground
///
/// Usage: fg [%job_id]
/// TODO: Implement terminal control and SIGCONT
fn builtin_fg(_args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    let job_list = jobs.list_jobs();

    if job_list.is_empty() {
        eprintln!("fg: no current job");
        return Ok(BuiltinResult::Failure(1));
    }

    // For now, just list what would be foregrounded
    eprintln!("fg: would foreground job (not yet implemented)");
    Ok(BuiltinResult::Failure(1))
}

/// bg - Continue job in background
///
/// Usage: bg [%job_id]
/// TODO: Implement SIGCONT to stopped jobs
fn builtin_bg(_args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    let job_list = jobs.list_jobs();

    if job_list.is_empty() {
        eprintln!("bg: no current job");
        return Ok(BuiltinResult::Failure(1));
    }

    eprintln!("bg: would background job (not yet implemented)");
    Ok(BuiltinResult::Failure(1))
}

/// jobs - List all jobs
///
/// Usage: jobs
fn builtin_jobs(_args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    let job_list = jobs.list_jobs();

    if job_list.is_empty() {
        return Ok(BuiltinResult::Success(0));
    }

    for job in job_list {
        let status_str = match job.status {
            crate::JobStatus::Running => "Running",
            crate::JobStatus::Background => "Running",
            crate::JobStatus::Stopped => "Stopped",
            crate::JobStatus::Done(status) => {
                if status.is_success() {
                    "Done"
                } else {
                    "Exit"
                }
            }
        };

        println!("[{}]  {}  {}", job.id, status_str, job.command);
    }

    Ok(BuiltinResult::Success(0))
}

/// wait - Wait for job to complete
///
/// Usage: wait [%job_id]
/// TODO: Implement blocking wait on specific job
fn builtin_wait(_args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    // For now, just acknowledge
    let job_list = jobs.list_jobs();

    if job_list.is_empty() {
        return Ok(BuiltinResult::Success(0));
    }

    eprintln!("wait: would wait for jobs (not yet implemented)");
    Ok(BuiltinResult::Success(0))
}

/// [ or test - Evaluate conditional expressions
///
/// Usage: [ expression ] or test expression
/// Returns 0 (true) or 1 (false)
///
/// Supported tests:
/// - [ string ] - true if non-empty
/// - [ string = string ] - true if equal
/// - [ string != string ] - true if not equal
/// - [ -n string ] - true if non-empty
/// - [ -z string ] - true if empty
/// - [ -f file ] - true if file exists
/// - [ -d dir ] - true if directory exists
fn builtin_test(args: &[String]) -> Result<BuiltinResult> {
    // Remove trailing ] if present (for [ command)
    let args = if args.last().map(|s| s.as_str()) == Some("]") {
        &args[..args.len() - 1]
    } else {
        args
    };

    if args.is_empty() {
        // Empty test is false
        return Ok(BuiltinResult::Failure(1));
    }

    // Handle unary operators
    if args.len() == 2 {
        let op = &args[0];
        let arg = &args[1];

        let result = match op.as_str() {
            "-n" => !arg.is_empty(),  // non-zero length
            "-z" => arg.is_empty(),    // zero length
            "-f" => std::path::Path::new(arg).is_file(),
            "-d" => std::path::Path::new(arg).is_dir(),
            "-e" => std::path::Path::new(arg).exists(),
            _ => {
                // Not a unary operator - maybe it's just a string test
                !args[0].is_empty()
            }
        };

        return Ok(if result {
            BuiltinResult::Success(0)
        } else {
            BuiltinResult::Failure(1)
        });
    }

    // Handle binary operators
    if args.len() == 3 {
        let left = &args[0];
        let op = &args[1];
        let right = &args[2];

        let result = match op.as_str() {
            "=" | "==" => left == right,
            "!=" => left != right,
            "-eq" => left.parse::<i64>().ok() == right.parse::<i64>().ok(),
            "-ne" => left.parse::<i64>().ok() != right.parse::<i64>().ok(),
            "-lt" => {
                if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                    l < r
                } else {
                    false
                }
            }
            "-le" => {
                if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                    l <= r
                } else {
                    false
                }
            }
            "-gt" => {
                if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                    l > r
                } else {
                    false
                }
            }
            "-ge" => {
                if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                    l >= r
                } else {
                    false
                }
            }
            _ => {
                eprintln!("test: unknown operator: {}", op);
                return Ok(BuiltinResult::Failure(2));
            }
        };

        return Ok(if result {
            BuiltinResult::Success(0)
        } else {
            BuiltinResult::Failure(1)
        });
    }

    // Single argument: test if non-empty
    if args.len() == 1 {
        return Ok(if !args[0].is_empty() {
            BuiltinResult::Success(0)
        } else {
            BuiltinResult::Failure(1)
        });
    }

    // Complex expressions not supported
    eprintln!("test: complex expressions not yet supported");
    Ok(BuiltinResult::Failure(2))
}

/// : - POSIX null utility
///
/// Usage: :
/// Does nothing, returns success (0).
/// Used for variable assignments and as a no-op command.
fn builtin_noop(_args: &[String]) -> Result<BuiltinResult> {
    Ok(BuiltinResult::Success(0))
}

/// true - Return success
///
/// Usage: true
/// Always returns exit code 0 (success).
/// Used in conditionals and implicit return value testing.
fn builtin_true() -> Result<BuiltinResult> {
    Ok(BuiltinResult::Success(0))
}

/// false - Return failure
///
/// Usage: false
/// Always returns exit code 1 (failure).
/// Used in conditionals and implicit return value testing.
fn builtin_false() -> Result<BuiltinResult> {
    Ok(BuiltinResult::Failure(1))
}

/// context - Manage context channel (fd 3)
///
/// Usage:
/// - context add <text>     - Append text to context channel
/// - context show           - Display all context entries
/// - context show --last N  - Show last N entries
/// - context clear          - Clear context channel
///
/// **F024:** Extended channels system - writes to stdcontext (fd 3)
fn builtin_context(args: &[String], channels: &mut ChannelManager) -> Result<BuiltinResult> {
    use colony_channels::Channel;

    if args.is_empty() {
        eprintln!("context: missing subcommand");
        eprintln!("Usage: context <add|show|clear> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    let subcommand = &args[0];

    match subcommand.as_str() {
        "add" => {
            if args.len() < 2 {
                eprintln!("context add: missing text");
                return Ok(BuiltinResult::Failure(1));
            }

            // Join all remaining args as the text to add
            let text = args[1..].join(" ");

            // Get mutable reference to context channel
            if let Some(ctx_channel) = channels.context() {
                // Use ContextChannel's add_text method
                ctx_channel.add_text(&text);
                println!("Added to context: {}", text);
                Ok(BuiltinResult::Success(0))
            } else {
                eprintln!("context: channel not available");
                Ok(BuiltinResult::Failure(1))
            }
        }

        "show" => {
            // Check for --last N option
            let count_limit = if args.len() >= 3 && args[1] == "--last" {
                args[2].parse::<usize>().ok()
            } else {
                None
            };

            // Get context channel
            if let Some(ctx_channel) = channels.context() {
                let entries = if let Some(limit) = count_limit {
                    ctx_channel.last(limit)
                } else {
                    ctx_channel.lines()
                };

                if entries.is_empty() {
                    println!("(context is empty)");
                } else {
                    for entry in entries {
                        println!("{}", entry);
                    }
                }

                Ok(BuiltinResult::Success(0))
            } else {
                eprintln!("context: channel not available");
                Ok(BuiltinResult::Failure(1))
            }
        }

        "clear" => {
            if let Some(ctx_channel) = channels.context() {
                // Call clear through Channel trait
                ctx_channel.clear();
                println!("Context cleared");
                Ok(BuiltinResult::Success(0))
            } else {
                eprintln!("context: channel not available");
                Ok(BuiltinResult::Failure(1))
            }
        }

        _ => {
            eprintln!("context: unknown subcommand: {}", subcommand);
            eprintln!("Usage: context <add|show|clear> [args...]");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

fn builtin_declare(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use std::collections::HashMap;
    
    let mut is_assoc = false;
    let mut reassembled_args: Vec<String> = Vec::new();
    let mut in_parens = false;
    let mut current_arg = String::new();
    
    for arg in args {
        if arg == "-A" {
            is_assoc = true;
            continue;
        } else if arg.starts_with('-') {
            continue;
        }
        
        if in_parens {
            current_arg.push(' ');
            current_arg.push_str(arg);
            if arg.ends_with(')') {
                in_parens = false;
                reassembled_args.push(current_arg.clone());
                current_arg.clear();
            }
        } else if arg.contains("=(") && !arg.ends_with(')') {
            in_parens = true;
            current_arg = arg.clone();
        } else {
            reassembled_args.push(arg.clone());
        }
    }
    
    for var_spec in reassembled_args {
        if let Some(eq_pos) = var_spec.find('=') {
            let name = &var_spec[..eq_pos];
            let value_part = &var_spec[eq_pos + 1..];
            
            if is_assoc && value_part.starts_with('(') && value_part.ends_with(')') {
                let inner = &value_part[1..value_part.len()-1];
                let mut map = HashMap::new();
                for item in inner.split_whitespace() {
                    if item.starts_with('[') {
                        if let Some(end) = item.find("]=") {
                            let key = &item[1..end];
                            let val = &item[end+2..];
                            map.insert(key.to_string(), val.to_string());
                        }
                    }
                }
                context.set_variable(name.to_string(), Variable::AssocArray(map));
            } else if is_assoc {
                context.set_variable(name.to_string(), Variable::AssocArray(HashMap::new()));
            } else {
                context.set_var(name.to_string(), value_part.to_string());
            }
        } else if is_assoc {
            context.set_variable(var_spec.clone(), Variable::AssocArray(HashMap::new()));
        }
    }
    
    Ok(BuiltinResult::Success(0))
}

fn builtin_read(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use std::io::{self, BufRead, Read};
    
    let mut raw_mode = false;
    let mut var_names: Vec<&str> = Vec::new();
    
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-r" {
            raw_mode = true;
        } else if arg.starts_with('-') {
            i += 1;
            continue;
        } else {
            var_names.push(arg);
        }
        i += 1;
    }
    
    if var_names.is_empty() {
        var_names.push("REPLY");
    }
    
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let mut line = String::new();
    
    if raw_mode {
        match stdin_lock.read_line(&mut line) {
            Ok(0) => {
                for name in &var_names {
                    context.set_var(name.to_string(), String::new());
                }
                return Ok(BuiltinResult::Success(1));
            }
            Ok(_) => {}
            Err(e) => return Err(ExecutionError::IoError(e)),
        }
    } else {
        loop {
            let mut chunk = String::new();
            match stdin_lock.read_line(&mut chunk) {
                Ok(0) => {
                    if line.is_empty() {
                        for name in &var_names {
                            context.set_var(name.to_string(), String::new());
                        }
                        return Ok(BuiltinResult::Success(1));
                    }
                    break;
                }
                Ok(_) => {
                    if chunk.ends_with('\n') {
                        chunk.pop();
                    }
                    if chunk.ends_with('\\') {
                        chunk.pop();
                        line.push_str(&chunk);
                    } else {
                        line.push_str(&chunk);
                        break;
                    }
                }
                Err(e) => return Err(ExecutionError::IoError(e)),
            }
        }
    }
    
    if line.ends_with('\n') {
        line.pop();
    }
    if line.ends_with('\r') {
        line.pop();
    }
    
    let ifs = context.ifs();
    let ifs_chars: Vec<char> = ifs.chars().collect();
    let ifs_whitespace: Vec<char> = ifs_chars.iter()
        .filter(|c| c.is_whitespace())
        .copied()
        .collect();
    
    if var_names.len() == 1 && var_names[0] == "REPLY" {
        context.set_var("REPLY".to_string(), line);
        return Ok(BuiltinResult::Success(0));
    }
    
    let trimmed = line.trim_start_matches(|c: char| ifs_whitespace.contains(&c));
    
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = trimmed.chars().collect();
    let mut j = 0;
    
    while j < chars.len() {
        let c = chars[j];
        if ifs_chars.contains(&c) {
            if !current.is_empty() || !ifs_whitespace.contains(&c) {
                fields.push(current);
                current = String::new();
            }
            while j + 1 < chars.len() && ifs_whitespace.contains(&chars[j + 1]) {
                j += 1;
            }
        } else {
            current.push(c);
        }
        j += 1;
    }
    if !current.is_empty() {
        fields.push(current);
    }
    
    for (idx, name) in var_names.iter().enumerate() {
        if idx < fields.len() {
            if idx == var_names.len() - 1 && idx + 1 < fields.len() {
                let rest: Vec<&str> = fields[idx..].iter().map(|s| s.as_str()).collect();
                context.set_var(name.to_string(), rest.join(" "));
            } else {
                context.set_var(name.to_string(), fields[idx].clone());
            }
        } else {
            context.set_var(name.to_string(), String::new());
        }
    }
    
    Ok(BuiltinResult::Success(0))
}

/// printf - Format and print data
///
/// Usage: printf [-v varname] format [arguments...]
///
/// Supports:
/// - Format specifiers: %s, %d, %i, %o, %x, %X, %b, %q, %%
/// - Width and precision: %10s, %-10s, %.5s, %10.5s
/// - Escape sequences: \n, \t, \r, \\, \", \', \0nnn, \xHH
/// - -v varname: Store result in variable instead of printing
fn builtin_printf(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("printf: missing format string");
        return Ok(BuiltinResult::Failure(1));
    }

    let mut store_var: Option<&str> = None;
    let mut arg_idx = 0;

    // Parse options
    while arg_idx < args.len() {
        if args[arg_idx] == "-v" {
            if arg_idx + 1 >= args.len() {
                eprintln!("printf: -v requires variable name");
                return Ok(BuiltinResult::Failure(1));
            }
            store_var = Some(&args[arg_idx + 1]);
            arg_idx += 2;
        } else if args[arg_idx].starts_with('-') && args[arg_idx] != "-" {
            // Skip unknown options
            arg_idx += 1;
        } else {
            break;
        }
    }

    if arg_idx >= args.len() {
        eprintln!("printf: missing format string");
        return Ok(BuiltinResult::Failure(1));
    }

    let format = &args[arg_idx];
    let format_args = &args[arg_idx + 1..];
    
    let output = format_printf(format, format_args)?;

    if let Some(var_name) = store_var {
        context.set_var(var_name.to_string(), output);
    } else {
        print!("{}", output);
    }

    Ok(BuiltinResult::Success(0))
}

/// Format a printf format string with arguments
fn format_printf(format: &str, args: &[String]) -> Result<String> {
    let mut output = String::new();
    let mut arg_idx = 0;
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' {
            // Escape sequence
            i += 1;
            if i < chars.len() {
                let escaped = match chars[i] {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    'a' => '\x07', // bell
                    'b' => '\x08', // backspace
                    'f' => '\x0c', // form feed
                    'v' => '\x0b', // vertical tab
                    '\\' => '\\',
                    '\'' => '\'',
                    '"' => '"',
                    '0' => {
                        // Octal escape \0nnn
                        let mut octal = String::new();
                        i += 1;
                        while i < chars.len() && chars[i].is_ascii_digit() && octal.len() < 3 {
                            octal.push(chars[i]);
                            i += 1;
                        }
                        i -= 1;
                        if octal.is_empty() {
                            '\0'
                        } else {
                            let val = u8::from_str_radix(&octal, 8).unwrap_or(0);
                            val as char
                        }
                    }
                    'x' => {
                        // Hex escape \xHH
                        let mut hex = String::new();
                        i += 1;
                        while i < chars.len() && chars[i].is_ascii_hexdigit() && hex.len() < 2 {
                            hex.push(chars[i]);
                            i += 1;
                        }
                        i -= 1;
                        if hex.is_empty() {
                            'x'
                        } else {
                            let val = u8::from_str_radix(&hex, 16).unwrap_or(0);
                            val as char
                        }
                    }
                    c => c,
                };
                output.push(escaped);
            }
        } else if chars[i] == '%' {
            // Format specifier
            i += 1;
            if i >= chars.len() {
                output.push('%');
                continue;
            }

            if chars[i] == '%' {
                output.push('%');
                i += 1;
                continue;
            }

            // Parse flags
            let mut left_justify = false;
            let mut zero_pad = false;
            let mut plus_sign = false;
            let mut space_sign = false;
            let mut alternate = false;

            while i < chars.len() {
                match chars[i] {
                    '-' => left_justify = true,
                    '0' => zero_pad = true,
                    '+' => plus_sign = true,
                    ' ' => space_sign = true,
                    '#' => alternate = true,
                    _ => break,
                }
                i += 1;
            }

            // Parse width
            let mut width = 0usize;
            while i < chars.len() && chars[i].is_ascii_digit() {
                width = width * 10 + (chars[i] as usize - '0' as usize);
                i += 1;
            }

            // Parse precision
            let mut precision: Option<usize> = None;
            if i < chars.len() && chars[i] == '.' {
                i += 1;
                let mut prec = 0usize;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    prec = prec * 10 + (chars[i] as usize - '0' as usize);
                    i += 1;
                }
                precision = Some(prec);
            }

            if i >= chars.len() {
                output.push('%');
                continue;
            }

            let spec = chars[i];
            let arg = if arg_idx < args.len() {
                let a = args[arg_idx].clone();
                arg_idx += 1;
                a
            } else {
                String::new()
            };

            let formatted = match spec {
                's' => {
                    let mut s = arg;
                    if let Some(prec) = precision {
                        if s.len() > prec {
                            s = s[..prec].to_string();
                        }
                    }
                    if width > 0 {
                        if left_justify {
                            format!("{:<width$}", s, width = width)
                        } else {
                            format!("{:>width$}", s, width = width)
                        }
                    } else {
                        s
                    }
                }
                'd' | 'i' => {
                    let num: i64 = arg.parse().unwrap_or(0);
                    format_number(num, 10, false, width, precision, left_justify, zero_pad, plus_sign, space_sign)
                }
                'u' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    format_unsigned(num, 10, false, width, left_justify, zero_pad)
                }
                'o' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    let prefix = if alternate && num != 0 { "0" } else { "" };
                    format_unsigned_with_prefix(num, 8, false, width, left_justify, zero_pad, prefix)
                }
                'x' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    let prefix = if alternate && num != 0 { "0x" } else { "" };
                    format_unsigned_with_prefix(num, 16, false, width, left_justify, zero_pad, prefix)
                }
                'X' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    let prefix = if alternate && num != 0 { "0X" } else { "" };
                    format_unsigned_with_prefix(num, 16, true, width, left_justify, zero_pad, prefix)
                }
                'c' => {
                    // First character
                    arg.chars().next().map(|c| c.to_string()).unwrap_or_default()
                }
                'b' => {
                    // %b expands escape sequences in the argument
                    expand_escapes(&arg)
                }
                'q' => {
                    // %q quotes the argument for shell reuse
                    shell_quote(&arg)
                }
                _ => {
                    // Unknown specifier - output literally
                    format!("%{}", spec)
                }
            };
            output.push_str(&formatted);
        } else {
            output.push(chars[i]);
        }
        i += 1;
    }

    Ok(output)
}

fn format_number(
    num: i64,
    _radix: u32,
    _uppercase: bool,
    width: usize,
    _precision: Option<usize>,
    left_justify: bool,
    zero_pad: bool,
    plus_sign: bool,
    space_sign: bool,
) -> String {
    let sign = if num < 0 {
        "-"
    } else if plus_sign {
        "+"
    } else if space_sign {
        " "
    } else {
        ""
    };
    
    let abs_num = num.abs();
    let num_str = abs_num.to_string();
    
    if width > 0 {
        let content_len = sign.len() + num_str.len();
        if content_len < width {
            let padding = width - content_len;
            if zero_pad && !left_justify {
                format!("{}{:0>width$}", sign, num_str, width = num_str.len() + padding)
            } else if left_justify {
                format!("{}{}{}", sign, num_str, " ".repeat(padding))
            } else {
                format!("{}{}{}", " ".repeat(padding), sign, num_str)
            }
        } else {
            format!("{}{}", sign, num_str)
        }
    } else {
        format!("{}{}", sign, num_str)
    }
}

fn format_unsigned(
    num: u64,
    radix: u32,
    uppercase: bool,
    width: usize,
    left_justify: bool,
    zero_pad: bool,
) -> String {
    format_unsigned_with_prefix(num, radix, uppercase, width, left_justify, zero_pad, "")
}

fn format_unsigned_with_prefix(
    num: u64,
    radix: u32,
    uppercase: bool,
    width: usize,
    left_justify: bool,
    zero_pad: bool,
    prefix: &str,
) -> String {
    let num_str = match radix {
        8 => format!("{:o}", num),
        16 => {
            if uppercase {
                format!("{:X}", num)
            } else {
                format!("{:x}", num)
            }
        }
        _ => num.to_string(),
    };
    
    let full = format!("{}{}", prefix, num_str);
    
    if width > 0 && full.len() < width {
        let padding = width - full.len();
        if zero_pad && !left_justify {
            format!("{}{:0>width$}", prefix, num_str, width = num_str.len() + padding)
        } else if left_justify {
            format!("{}{}", full, " ".repeat(padding))
        } else {
            format!("{}{}", " ".repeat(padding), full)
        }
    } else {
        full
    }
}

fn expand_escapes(s: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 1;
            match chars[i] {
                'n' => output.push('\n'),
                't' => output.push('\t'),
                'r' => output.push('\r'),
                'a' => output.push('\x07'),
                'b' => output.push('\x08'),
                'f' => output.push('\x0c'),
                'v' => output.push('\x0b'),
                '\\' => output.push('\\'),
                '\'' => output.push('\''),
                '"' => output.push('"'),
                '0' => {
                    // Octal
                    let mut octal = String::new();
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() && octal.len() < 3 {
                        octal.push(chars[i]);
                        i += 1;
                    }
                    i -= 1;
                    let val = u8::from_str_radix(&octal, 8).unwrap_or(0);
                    output.push(val as char);
                }
                'x' => {
                    // Hex
                    let mut hex = String::new();
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_hexdigit() && hex.len() < 2 {
                        hex.push(chars[i]);
                        i += 1;
                    }
                    i -= 1;
                    if !hex.is_empty() {
                        let val = u8::from_str_radix(&hex, 16).unwrap_or(0);
                        output.push(val as char);
                    }
                }
                'c' => {
                    // Stop processing - %b specific
                    break;
                }
                c => output.push(c),
            }
        } else {
            output.push(chars[i]);
        }
        i += 1;
    }

    output
}

fn shell_quote(s: &str) -> String {
    // If the string is simple, just add single quotes
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/') {
        return s.to_string();
    }
    
    // Use $'...' quoting for strings with special characters
    let mut output = String::from("$'");
    for c in s.chars() {
        match c {
            '\'' => output.push_str("\\'"),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\t' => output.push_str("\\t"),
            '\r' => output.push_str("\\r"),
            c if c.is_ascii_control() => {
                output.push_str(&format!("\\x{:02x}", c as u8));
            }
            c => output.push(c),
        }
    }
    output.push('\'');
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::Span;

    fn make_simple_command(name: &str, args: Vec<&str>) -> SimpleCommand {
        SimpleCommand {
            name: Word::literal(name, Span::DUMMY),
            args: args
                .into_iter()
                .map(|s| Word::literal(s, Span::DUMMY))
                .collect(),
            redirects: vec![],
            env_assignments: vec![],
            span: Span::DUMMY,
        }
    }

    #[test]
    fn test_is_builtin() {
        assert!(is_builtin("cd"));
        assert!(is_builtin("export"));
        assert!(is_builtin("echo"));
        assert!(is_builtin("pwd"));
        assert!(is_builtin("exit"));
        assert!(!is_builtin("ls"));
        assert!(!is_builtin("grep"));
    }

    #[test]
    fn test_builtin_pwd() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("pwd", vec![]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_builtin_echo() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("echo", vec!["hello", "world"]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_builtin_exit() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("exit", vec!["42"]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert!(builtin_result.is_exit());
        assert_eq!(builtin_result.exit_code(), 42);
    }

    #[test]
    fn test_builtin_export() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("export", vec!["TEST_VAR=hello"]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);

        // Verify variable was set
        assert_eq!(context.get_env("TEST_VAR"), Some("hello"));
    }

    #[test]
    fn test_builtin_unset() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        context.set_env("TEST_VAR", "value");

        let cmd = make_simple_command("unset", vec!["TEST_VAR"]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);

        // Verify variable was removed
        assert_eq!(context.get_env("TEST_VAR"), None);
    }

    #[test]
    fn test_builtin_cd() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        // Save current dir
        let original_dir = env::current_dir().unwrap();

        // cd to /tmp
        let cmd = make_simple_command("cd", vec!["/tmp"]);
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);

        // Verify we're in /tmp (resolve symlinks for macOS)
        let current = env::current_dir().unwrap().canonicalize().unwrap();
        let expected = PathBuf::from("/tmp").canonicalize().unwrap();
        assert_eq!(current, expected);

        // Restore original directory
        env::set_current_dir(original_dir).ok();
    }

    #[test]
    fn test_builtin_type() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("type", vec!["cd", "echo"]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_non_builtin() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("ls", vec![]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_none()); // ls is not a builtin
    }

    #[test]
    fn test_builtin_jobs_empty() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("jobs", vec![]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_builtin_jobs_with_job() {
        use nix::unistd::Pid;

        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        // Add a fake job
        jobs.add_job(
            Pid::from_raw(12345),
            vec![Pid::from_raw(12345)],
            "sleep 10".to_string(),
        );

        let cmd = make_simple_command("jobs", vec![]);
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_reactive_export() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        // Export a variable
        let cmd = make_simple_command("export", vec!["REACTIVE_TEST=initial"]);
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        // Check tuple was emitted
        let pattern = Tuple::pattern(&[
            Value::Str("env_change".to_string()),
            Value::Str("REACTIVE_TEST".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]);

        let tuple = tuple_space.inp(pattern);
        assert!(tuple.is_some());

        let t = tuple.unwrap();
        assert_eq!(t.fields[0], Value::Str("env_change".to_string()));
        assert_eq!(t.fields[1], Value::Str("REACTIVE_TEST".to_string()));
        assert_eq!(t.fields[2], Value::Str("".to_string())); // old value (was empty)
        assert_eq!(t.fields[3], Value::Str("initial".to_string())); // new value
    }

    #[test]
    fn test_reactive_cd() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        // Save original dir
        let original_dir = env::current_dir().unwrap();

        // cd to /tmp
        let cmd = make_simple_command("cd", vec!["/tmp"]);
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        // Check tuple was emitted
        let pattern = Tuple::pattern(&[
            Value::Str("dir_change".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]);

        let tuple = tuple_space.inp(pattern);
        assert!(tuple.is_some());

        let t = tuple.unwrap();
        assert_eq!(t.fields[0], Value::Str("dir_change".to_string()));
        // fields[1] is old dir, fields[2] is new dir (should be /tmp or /private/tmp)
        // Just verify it's a string value
        assert!(matches!(t.fields[2], Value::Str(_)));

        // Restore
        env::set_current_dir(original_dir).ok();
    }

    #[test]
    fn test_reactive_unset() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        // Set then unset
        context.set_env("TO_DELETE", "old_value");

        let cmd = make_simple_command("unset", vec!["TO_DELETE"]);
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        // Check tuple was emitted with Null
        let pattern = Tuple::pattern(&[
            Value::Str("env_change".to_string()),
            Value::Str("TO_DELETE".to_string()),
            Value::Wildcard,
            Value::Null,
        ]);

        let tuple = tuple_space.inp(pattern);
        assert!(tuple.is_some());
    }
}
