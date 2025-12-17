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
use colony_expand::{ExpansionContext, Expander};
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

/// Check if a command name is a builtin
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "cd" | "export" | "unset" | "exit" | "pwd" | "echo" | "type"
        | "alias" | "unalias" | "." | "source"
        | "fg" | "bg" | "jobs" | "wait"
        | "[" | "test"  // POSIX test command
        | "context"     // Extended channel system (F024)
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
    for arg_word in &cmd.args {
        let fields = match expander.expand_word(arg_word) {
            Ok(fields) => fields,
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
        "context" => builtin_context(&args, channels),
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
        // Capture old value for reactive tuple
        let old_value = context.get_env(var_name).unwrap_or("").to_string();

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
/// Usage: alias [name=value ...]
/// TODO: Implement alias storage
fn builtin_alias(_args: &[String], _context: &mut ExpansionContext) -> Result<BuiltinResult> {
    eprintln!("alias: not yet implemented");
    Ok(BuiltinResult::Failure(1))
}

/// unalias - Remove command aliases
///
/// Usage: unalias name [name...]
/// TODO: Implement alias storage
fn builtin_unalias(_args: &[String], _context: &mut ExpansionContext) -> Result<BuiltinResult> {
    eprintln!("unalias: not yet implemented");
    Ok(BuiltinResult::Failure(1))
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
