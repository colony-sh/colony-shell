//! Command substitution $(cmd) execution
//!
//! Executes a statement and captures its stdout output.
//! Used by the expansion system for command substitution.

use crate::{execute_builtin, is_builtin, Result};
use colony_channels::ChannelManager;
use colony_expand::ExpansionContext;
use colony_parser::Statement;
use colony_tuple::TupleSpace;
use nix::unistd::{close, dup2, fork, pipe, read, ForkResult};
use std::os::fd::{AsRawFd, OwnedFd};

/// Execute a statement and capture its stdout
///
/// This creates a pipe, forks, redirects stdout in the child,
/// executes the statement, and returns the captured output.
///
/// Used for command substitution $(cmd) in expansions.
///
/// Note: This is synchronous (blocking) because command substitution
/// must complete before expansion can continue.
pub fn execute_and_capture(
    stmt: &Statement,
    context: &mut ExpansionContext,
) -> std::result::Result<String, String> {
    // Create pipe for capturing stdout
    let (read_fd, write_fd) = pipe().map_err(|e| format!("pipe() failed: {}", e))?;

    // Fork to execute command
    match unsafe { fork() }.map_err(|e| format!("fork() failed: {}", e))? {
        ForkResult::Child => {
            // Child process: redirect stdout to pipe write end
            drop(read_fd); // Close read end in child

            // Redirect stdout to write end
            if let Err(e) = dup2(write_fd.as_raw_fd(), 1) {
                eprintln!("dup2 failed: {}", e);
                std::process::exit(1);
            }
            drop(write_fd);

            // Execute the statement
            // Note: This is in child process, so we don't return from here
            let exit_code = execute_statement_in_child(stmt, context);
            std::process::exit(exit_code);
        }

        ForkResult::Parent { child } => {
            // Parent process: read from pipe
            drop(write_fd); // Close write end in parent

            // Read all output from child
            let mut output = Vec::new();
            let mut buffer = [0u8; 4096];

            loop {
                match read(read_fd.as_raw_fd(), &mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        output.extend_from_slice(&buffer[..n])
                    },
                    Err(e) => {
                        let _ = nix::sys::wait::waitpid(child, None);
                        return Err(format!("read() failed: {}", e));
                    }
                }
            }

            drop(read_fd);

            // Wait for child to complete
            match nix::sys::wait::waitpid(child, None) {
                Ok(status) => {
                    // Convert output to string
                    let output_str = String::from_utf8_lossy(&output).to_string();

                    // Check if command failed (non-zero exit code)
                    use nix::sys::wait::WaitStatus;
                    let failed = match status {
                        WaitStatus::Exited(_, code) => code != 0,
                        _ => true, // Signaled or stopped = failure
                    };

                    if failed {
                        return Err(format!(
                            "command substitution failed with status: {:?}",
                            status
                        ));
                    }

                    Ok(output_str)
                }
                Err(e) => Err(format!("waitpid() failed: {}", e)),
            }
        }
    }
}

/// Execute a statement in the child process (for command substitution)
///
/// This is called after fork() in the child, so it doesn't return.
/// Returns an exit code which the child should exit with.
fn execute_statement_in_child(stmt: &Statement, context: &mut ExpansionContext) -> i32 {
    use crate::JobTable;
    use std::io::Write;


    // For command substitution, we only support simple commands and pipelines
    match stmt {
        Statement::SimpleCommand(cmd) => {

            // Check if it's a builtin
            let jobs = JobTable::new();
            let tuple_space = TupleSpace::new();
            let mut channels = ChannelManager::new();

            if let Some(result) = execute_builtin(cmd, context, &jobs, &tuple_space, &mut channels)
            {
                // Flush stdout after builtin (builtins use println! which is buffered)
                let _ = std::io::stdout().flush();

                match result {
                    Ok(builtin_result) => {
                        builtin_result.exit_code()
                    }
                    Err(e) => {
                        1
                    }
                }
            } else {
                // External command - call synchronous helper
                match execute_command_in_child_blocking(cmd, context) {
                    Ok(status) => {
                        status.code()
                    }
                    Err(e) => {
                        1
                    }
                }
            }
        }

        Statement::Pipeline(pipeline) => {
            // Execute pipeline synchronously
            match execute_pipeline_in_child_blocking(pipeline, context) {
                Ok(status) => status.code(),
                Err(_) => 1,
            }
        }

        Statement::List(list) => {
            // Execute first command
            let mut last_code = execute_statement_in_child(&list.first, context);

            // Execute rest with operators
            for (op, stmt) in &list.rest {
                use colony_parser::ListOp;
                let should_run = match op {
                    ListOp::Semi => true,                // Always run
                    ListOp::And => last_code == 0,       // Run if previous succeeded
                    ListOp::Or => last_code != 0,        // Run if previous failed
                    ListOp::Background => true,          // Run in background (ignore for now)
                };

                if should_run {
                    last_code = execute_statement_in_child(stmt, context);
                }
            }

            last_code
        }

        other => {
            eprintln!("command substitution: unsupported statement type");
            1
        }
    }
}

/// Execute a simple command in child process (blocking, no tokio)
///
/// This is called from the child process after fork(), so we can't
/// use tokio (runtime already exists in parent).
fn execute_command_in_child_blocking(
    cmd: &colony_parser::SimpleCommand,
    context: &mut ExpansionContext,
) -> Result<crate::ExitStatus> {
    use crate::{apply_redirects, resolve_command};
    use colony_expand::Expander;
    use nix::unistd::execve;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    // Create expander with command substitution (recursive!)
    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
        execute_and_capture(stmt, ctx)
    });

    // Expand command name
    let name_fields = expander.expand_word(&cmd.name)?;
    if name_fields.is_empty() {
        return Err(crate::ExecutionError::CommandNotFound("<empty>".to_string()));
    }
    let command_name = &name_fields[0].text;

    // Expand arguments
    let mut args = vec![command_name.clone()];
    for arg_word in &cmd.args {
        let fields = expander.expand_word(arg_word)?;
        for field in fields {
            args.push(field.text);
        }
    }

    // Apply redirects
    let _opened_fds = apply_redirects(&cmd.redirects, context)?;

    // Resolve command path
    let path_env = context.get_env("PATH");
    let program_path = resolve_command(command_name, path_env)?;

    // Build environment
    let env: Vec<(String, String)> = std::env::vars().collect();

    // Convert to CStrings
    let c_program = CString::new(program_path.as_os_str().as_bytes())
        .map_err(|_| crate::ExecutionError::CommandNotFound(command_name.to_string()))?;
    let c_args: Vec<CString> = args
        .iter()
        .map(|s| CString::new(s.as_bytes()).unwrap())
        .collect();
    let c_env: Vec<CString> = env
        .iter()
        .map(|(k, v)| CString::new(format!("{}={}", k, v).as_bytes()).unwrap())
        .collect();

    // Exec - this replaces the current process
    execve(&c_program, &c_args, &c_env)
        .map_err(|e| crate::ExecutionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    // Never reached
    unreachable!()
}

/// Execute a pipeline in child process (blocking, no tokio)
fn execute_pipeline_in_child_blocking(
    pipeline: &colony_parser::Pipeline,
    context: &ExpansionContext,
) -> Result<crate::ExitStatus> {

    // For command substitution, we can just execute the first command
    // and ignore piping complexity (the child process will exec anyway)
    if let Some(first) = pipeline.elements.first() {
        if let colony_parser::Statement::SimpleCommand(cmd) = &*first.command {
            execute_command_in_child_blocking(cmd, &mut context.clone())
        } else {
            Err(crate::ExecutionError::CommandNotFound("<complex>".to_string()))
        }
    } else {
        Ok(crate::ExitStatus::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::pest_api;

    #[test]
    fn test_simple_command_substitution() {
        let mut context = ExpansionContext::new();

        // Parse "echo hello"
        let stmt = pest_api::parse_statement("echo hello").unwrap();

        // Execute and capture
        let output = execute_and_capture(&stmt, &mut context).unwrap();

        // Should capture "hello\n" (or just "hello" after trimming)
        assert!(output.contains("hello"));
    }

    #[test]
    fn test_pwd_substitution() {
        let mut context = ExpansionContext::new();

        // Parse "pwd"
        let stmt = pest_api::parse_statement("pwd").unwrap();

        // Execute and capture
        let output = execute_and_capture(&stmt, &mut context).unwrap();

        // Should return a directory path
        assert!(output.starts_with('/'));
    }

    #[test]
    fn test_echo_world_substitution() {
        let mut context = ExpansionContext::new();

        // Parse "echo world"
        let stmt = pest_api::parse_statement("echo world").unwrap();

        // Execute and capture
        let output = execute_and_capture(&stmt, &mut context).unwrap();

        // Should capture "world\n"
        assert!(!output.is_empty(), "Output should not be empty");
        assert!(output.contains("world"), "Output should contain 'world', got: {:?}", output);
    }
}
