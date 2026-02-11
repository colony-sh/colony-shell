//! High-level command execution

use crate::path::resolve_command;
use crate::process::{spawn_process, ExitStatus};
use crate::{ExecutionError, Result};
use colony_expand::{Expander, ExpansionContext};
use colony_parser::SimpleCommand;

fn build_env_with_exported_functions(context: &ExpansionContext) -> Vec<(String, String)> {
    crate::build_env(context)
}

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
        .map_err(|e| ExecutionError::IoError(std::io::Error::other(e)))?
}

/// Execute a command synchronously (blocking)
fn execute_command_blocking(
    cmd: &SimpleCommand,
    context: &mut ExpansionContext,
) -> Result<ExitStatus> {
    // Create expander with command substitution support
    let mut expander = Expander::with_executor_and_expr(
        context.clone(),
        |stmt, ctx| crate::execute_and_capture(stmt, ctx),
        |expr, ctx| crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
    );

    // 1. Expand command name
    let name_fields = expander.expand_word(&cmd.name)?;
    if name_fields.is_empty() {
        return Err(ExecutionError::CommandNotFound("<empty>".to_string()));
    }
    let mut command_name = name_fields[0].text.clone();
    let extra_args_from_name: Vec<String> =
        name_fields[1..].iter().map(|f| f.text.clone()).collect();

    // 1b. Check for alias expansion
    if let Some(alias_value) = context.get_alias(&command_name) {
        // Split alias value into words (simple word splitting on whitespace)
        let alias_words: Vec<&str> = alias_value.split_whitespace().collect();

        if !alias_words.is_empty() {
            // First word becomes the new command name
            command_name = alias_words[0].to_string();

            // Remaining words are prepended to args
            // We'll build a new args list: [new_name, alias_args..., original_args...]
            let mut args = vec![command_name.clone()]; // argv[0]

            // Add alias arguments
            for alias_arg in &alias_words[1..] {
                args.push(alias_arg.to_string());
            }

            // Add extra args from "$@" expansion
            args.extend(extra_args_from_name.clone());

            // Add original command arguments
            for arg_word in &cmd.args {
                let fields = expander.expand_word(arg_word)?;
                for field in fields {
                    args.push(field.text);
                }
            }

            // Continue to path resolution with expanded alias
            crate::xtrace::trace_simple_command(context, &command_name, &args[1..]);
            let path_env = context.get_env("PATH");
            let program_path = resolve_command(&command_name, path_env)?;

            let mut env = build_env_with_exported_functions(context);
            for assignment in &cmd.env_assignments {
                if assignment.subscript.is_none() {
                    if let colony_parser::AssignmentValue::Word(word) = &assignment.value {
                        let value = expander.expand_word_for_assignment(word)?;
                        env.retain(|(k, _)| k != &assignment.name);
                        env.push((assignment.name.clone(), value));
                    }
                }
            }
            let handle = spawn_process(&program_path, &args, &env, &cmd.redirects, context)?;

            // Register child process in secure lineage tracking
            if let Some(registry_arc) = context.secure_registry() {
                registry_arc.lock().register_child(std::process::id(), handle.pid());
            }

            return handle.wait();
        }
    }

    // 2. No alias - expand arguments normally
    let mut args = vec![command_name.clone()]; // argv[0] is the program name
    args.extend(extra_args_from_name);
    for arg_word in &cmd.args {
        let fields = expander.expand_word(arg_word)?;
        for field in fields {
            args.push(field.text);
        }
    }

    // 3. Resolve command path
    crate::xtrace::trace_simple_command(context, &command_name, &args[1..]);
    let path_env = context.get_env("PATH");
    let program_path = resolve_command(&command_name, path_env)?;

    let mut env = build_env_with_exported_functions(context);

    for assignment in &cmd.env_assignments {
        if assignment.subscript.is_none() {
            if let colony_parser::AssignmentValue::Word(word) = &assignment.value {
                let value = expander.expand_word_for_assignment(word)?;
                env.retain(|(k, _)| k != &assignment.name);
                env.push((assignment.name.clone(), value));
            }
        }
    }

    // 5. Spawn process with redirects
    let handle = spawn_process(&program_path, &args, &env, &cmd.redirects, context)?;

    // Register child process in secure lineage tracking
    if let Some(registry_arc) = context.secure_registry() {
        registry_arc.lock().register_child(std::process::id(), handle.pid());
    }

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
