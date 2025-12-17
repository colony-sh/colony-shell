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

use crate::{ExecutionError, Result};
use colony_expand::{ExpansionContext, Expander};
use colony_parser::{SimpleCommand, Word};
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
        "cd" | "export" | "unset" | "exit" | "pwd" | "echo" | "type" | "alias" | "unalias" | "." | "source"
    )
}

/// Execute a builtin command
///
/// Returns None if the command is not a builtin.
/// Returns Some(BuiltinResult) if it is a builtin.
pub fn execute_builtin(
    cmd: &SimpleCommand,
    context: &mut ExpansionContext,
) -> Option<Result<BuiltinResult>> {
    // Expand command name
    let mut expander = Expander::new(context.clone());
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
        "cd" => builtin_cd(&args, context),
        "export" => builtin_export(&args, context),
        "unset" => builtin_unset(&args, context),
        "exit" => builtin_exit(&args),
        "pwd" => builtin_pwd(&args),
        "echo" => builtin_echo(&args),
        "type" => builtin_type(&args),
        "alias" => builtin_alias(&args, context),
        "unalias" => builtin_unalias(&args, context),
        "." | "source" => builtin_source(&args, context),
        _ => unreachable!("is_builtin returned true but no handler"),
    };

    Some(result)
}

/// cd - Change working directory
///
/// Usage: cd [dir]
/// - No args: cd to HOME
/// - One arg: cd to specified directory
fn builtin_cd(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
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

    // Update PWD environment variable
    if let Ok(pwd) = env::current_dir() {
        if let Some(pwd_str) = pwd.to_str() {
            context.set_env("PWD", pwd_str);
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// export - Set environment variables
///
/// Usage: export VAR=value [VAR2=value2 ...]
fn builtin_export(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
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

            context.set_env(name, value);
            env::set_var(name, value);
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
fn builtin_unset(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    for var_name in args {
        context.unset_env(var_name);
        env::remove_var(var_name);
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
        let cmd = make_simple_command("pwd", vec![]);

        let result = execute_builtin(&cmd, &mut context);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_builtin_echo() {
        let mut context = ExpansionContext::new();
        let cmd = make_simple_command("echo", vec!["hello", "world"]);

        let result = execute_builtin(&cmd, &mut context);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_builtin_exit() {
        let mut context = ExpansionContext::new();
        let cmd = make_simple_command("exit", vec!["42"]);

        let result = execute_builtin(&cmd, &mut context);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert!(builtin_result.is_exit());
        assert_eq!(builtin_result.exit_code(), 42);
    }

    #[test]
    fn test_builtin_export() {
        let mut context = ExpansionContext::new();
        let cmd = make_simple_command("export", vec!["TEST_VAR=hello"]);

        let result = execute_builtin(&cmd, &mut context);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);

        // Verify variable was set
        assert_eq!(context.get_env("TEST_VAR"), Some("hello"));
    }

    #[test]
    fn test_builtin_unset() {
        let mut context = ExpansionContext::new();
        context.set_env("TEST_VAR", "value");

        let cmd = make_simple_command("unset", vec!["TEST_VAR"]);

        let result = execute_builtin(&cmd, &mut context);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);

        // Verify variable was removed
        assert_eq!(context.get_env("TEST_VAR"), None);
    }

    #[test]
    fn test_builtin_cd() {
        let mut context = ExpansionContext::new();

        // Save current dir
        let original_dir = env::current_dir().unwrap();

        // cd to /tmp
        let cmd = make_simple_command("cd", vec!["/tmp"]);
        let result = execute_builtin(&cmd, &mut context);
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
        let cmd = make_simple_command("type", vec!["cd", "echo"]);

        let result = execute_builtin(&cmd, &mut context);
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_non_builtin() {
        let mut context = ExpansionContext::new();
        let cmd = make_simple_command("ls", vec![]);

        let result = execute_builtin(&cmd, &mut context);
        assert!(result.is_none()); // ls is not a builtin
    }
}
