//! High-level command execution

use crate::path::resolve_command;
use crate::process::{spawn_process, ExitStatus};
use crate::{ExecutionError, Result};
use colony_expand::{ExpansionContext, Expander};
use colony_parser::SimpleCommand;

/// Execute a simple command
///
/// This is the high-level entry point that:
/// 1. Expands the command name and arguments
/// 2. Resolves the command path
/// 3. Spawns the process
/// 4. Waits for completion
///
/// Uses spawn_blocking internally since fork/exec is blocking
pub async fn execute_command(
    cmd: &SimpleCommand,
    context: &mut ExpansionContext,
) -> Result<ExitStatus> {
    // Capture needed data before moving into spawn_blocking
    let cmd_clone = cmd.clone();
    let mut context_clone = context.clone();

    // Use spawn_blocking for fork/exec (rule #10)
    tokio::task::spawn_blocking(move || execute_command_blocking(&cmd_clone, &mut context_clone))
        .await
        .map_err(|e| ExecutionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?
}

/// Execute a command synchronously (blocking)
fn execute_command_blocking(
    cmd: &SimpleCommand,
    context: &mut ExpansionContext,
) -> Result<ExitStatus> {
    let mut expander = Expander::new(context.clone());

    // 1. Expand command name
    let name_fields = expander.expand_word(&cmd.name)?;
    if name_fields.is_empty() {
        return Err(ExecutionError::CommandNotFound("<empty>".to_string()));
    }
    let command_name = &name_fields[0].text;

    // 2. Expand arguments
    let mut args = vec![command_name.clone()]; // argv[0] is the program name
    for arg_word in &cmd.args {
        let fields = expander.expand_word(arg_word)?;
        for field in fields {
            args.push(field.text);
        }
    }

    // 3. Resolve command path
    let path_env = context.get_env("PATH");
    let program_path = resolve_command(command_name, path_env)?;

    // 4. Build environment
    // For now, just pass through current environment
    // TODO: Handle cmd.env_assignments
    let env: Vec<(String, String)> = std::env::vars().collect();

    // 5. Spawn process with redirects
    let handle = spawn_process(&program_path, &args, &env, &cmd.redirects, context)?;

    // 6. Wait for completion
    handle.wait()
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::{Span, Word};

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

    #[tokio::test]
    async fn test_execute_echo() {
        let mut context = ExpansionContext::new();
        let cmd = make_simple_command("echo", vec!["hello", "world"]);

        let result = execute_command(&cmd, &mut context).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_success());
    }

    #[tokio::test]
    async fn test_execute_true() {
        let mut context = ExpansionContext::new();
        let cmd = make_simple_command("true", vec![]);

        let result = execute_command(&cmd, &mut context).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().code(), 0);
    }

    #[tokio::test]
    async fn test_execute_false() {
        let mut context = ExpansionContext::new();
        let cmd = make_simple_command("false", vec![]);

        let result = execute_command(&cmd, &mut context).await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_success());
    }

    #[tokio::test]
    async fn test_command_not_found() {
        let mut context = ExpansionContext::new();
        let cmd = make_simple_command("nonexistent_command_12345", vec![]);

        let result = execute_command(&cmd, &mut context).await;
        assert!(result.is_err());
    }
}
