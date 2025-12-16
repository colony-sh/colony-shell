//! Pipeline execution
//!
//! Executes commands connected by pipes: cmd1 | cmd2 | cmd3
//! All commands run concurrently with stdout→stdin connections

use crate::path::resolve_command;
use crate::process::ExitStatus;
use crate::{ExecutionError, Result};
use colony_expand::{ExpansionContext, Expander};
use colony_parser::{Pipeline, PipelineElement, Statement};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{close, dup2, execve, fork, pipe, ForkResult, Pid};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, RawFd};

/// Execute a pipeline
///
/// All commands run concurrently with pipes connecting them.
/// Returns the exit status of the last command in the pipeline.
pub async fn execute_pipeline(
    pipeline: &Pipeline,
    context: &mut ExpansionContext,
) -> Result<ExitStatus> {
    let pipeline_clone = pipeline.clone();
    let context_clone = context.clone();

    // Use spawn_blocking since we use fork/exec
    tokio::task::spawn_blocking(move || execute_pipeline_blocking(&pipeline_clone, &context_clone))
        .await
        .map_err(|e| ExecutionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?
}

/// Execute pipeline synchronously (blocking)
fn execute_pipeline_blocking(pipeline: &Pipeline, context: &ExpansionContext) -> Result<ExitStatus> {
    let num_commands = pipeline.elements.len();

    if num_commands == 0 {
        return Ok(ExitStatus::success());
    }

    if num_commands == 1 {
        // Single command - no pipes needed
        return execute_single_pipeline_element(&pipeline.elements[0], context);
    }

    // Create pipes: N commands need N-1 pipes
    let mut pipes: Vec<(RawFd, RawFd)> = Vec::new();
    for _ in 0..(num_commands - 1) {
        let pipe_fds = pipe().map_err(ExecutionError::ForkError)?;
        let read_end = pipe_fds.0.as_raw_fd();
        let write_end = pipe_fds.1.as_raw_fd();
        pipes.push((read_end, write_end));
        // Leak the OwnedFds so they don't get closed automatically
        std::mem::forget(pipe_fds.0);
        std::mem::forget(pipe_fds.1);
    }

    // Spawn all processes
    let mut pids: Vec<Pid> = Vec::new();

    for (i, element) in pipeline.elements.iter().enumerate() {
        match unsafe { fork() }.map_err(ExecutionError::ForkError)? {
            ForkResult::Parent { child } => {
                pids.push(child);
            }
            ForkResult::Child => {
                // Setup pipes for this command
                setup_pipeline_fds(i, num_commands, &pipes);

                // Close all pipe ends in child
                for (read_fd, write_fd) in &pipes {
                    let _ = close(*read_fd);
                    let _ = close(*write_fd);
                }

                // Execute the command
                execute_pipeline_element_in_child(element, context);
                // execute_pipeline_element_in_child never returns
            }
        }
    }

    // Parent: close all pipe ends
    for (read_fd, write_fd) in pipes {
        let _ = close(read_fd);
        let _ = close(write_fd);
    }

    // Wait for all children and collect exit statuses
    let mut last_status = ExitStatus::success();
    for pid in pids {
        match waitpid(pid, None) {
            Ok(WaitStatus::Exited(_pid, code)) => {
                last_status = ExitStatus::new(code);
            }
            Ok(WaitStatus::Signaled(_pid, signal, _core_dumped)) => {
                last_status = ExitStatus::new(128 + signal as i32);
            }
            _ => {}
        }
    }

    // Return the exit status of the last command
    Ok(last_status)
}

/// Setup file descriptors for a command in the pipeline
fn setup_pipeline_fds(index: usize, num_commands: usize, pipes: &[(RawFd, RawFd)]) {
    // First command: stdin from parent, stdout to pipe[0]
    // Middle commands: stdin from pipe[i-1], stdout to pipe[i]
    // Last command: stdin from pipe[N-2], stdout to parent

    if index > 0 {
        // Not the first command - read from previous pipe
        let (read_fd, _) = pipes[index - 1];
        dup2(read_fd, 0).unwrap(); // stdin = read_fd
    }

    if index < num_commands - 1 {
        // Not the last command - write to next pipe
        let (_, write_fd) = pipes[index];
        dup2(write_fd, 1).unwrap(); // stdout = write_fd
    }
}

/// Execute a pipeline element in a child process
///
/// This function never returns - it either execs or exits
fn execute_pipeline_element_in_child(element: &PipelineElement, context: &ExpansionContext) -> ! {
    // Extract the simple command
    let simple_cmd = match &*element.command {
        Statement::SimpleCommand(cmd) => cmd,
        _ => {
            eprintln!("colony-exec: pipeline element is not a simple command");
            std::process::exit(1);
        }
    };

    // Expand and resolve command
    let mut expander = Expander::new(context.clone());

    let name_fields = match expander.expand_word(&simple_cmd.name) {
        Ok(fields) if !fields.is_empty() => fields,
        _ => {
            eprintln!("colony-exec: command expansion failed");
            std::process::exit(127);
        }
    };
    let command_name = &name_fields[0].text;

    // Expand arguments
    let mut args = vec![command_name.clone()];
    for arg_word in &simple_cmd.args {
        if let Ok(fields) = expander.expand_word(arg_word) {
            for field in fields {
                args.push(field.text);
            }
        }
    }

    // Resolve command path
    let path_env = context.get_env("PATH");
    let program_path = match resolve_command(command_name, path_env) {
        Ok(path) => path,
        Err(_) => {
            eprintln!("colony-exec: {}: command not found", command_name);
            std::process::exit(127);
        }
    };

    // Build environment
    let env: Vec<(String, String)> = std::env::vars().collect();

    // Convert to CStrings
    let c_program = CString::new(program_path.as_os_str().as_bytes()).unwrap();
    let c_args: Vec<CString> = args
        .iter()
        .map(|s| CString::new(s.as_bytes()).unwrap())
        .collect();
    let c_env: Vec<CString> = env
        .iter()
        .map(|(k, v)| CString::new(format!("{}={}", k, v).as_bytes()).unwrap())
        .collect();

    // Exec
    let _ = execve(&c_program, &c_args, &c_env);

    // If we get here, exec failed
    eprintln!("colony-exec: exec failed");
    std::process::exit(127);
}

/// Execute a single pipeline element (no pipes needed)
fn execute_single_pipeline_element(
    element: &PipelineElement,
    context: &ExpansionContext,
) -> Result<ExitStatus> {
    // For now, just handle simple commands
    let simple_cmd = match &*element.command {
        Statement::SimpleCommand(cmd) => cmd,
        _ => return Err(ExecutionError::CommandNotFound("<complex>".to_string())),
    };

    // Use the existing command execution logic
    let mut context_mut = context.clone();
    let result = tokio::runtime::Handle::current().block_on(async {
        crate::command::execute_command(simple_cmd, &mut context_mut).await
    });

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::{PipeOp, Span, Word};

    fn make_simple_cmd_statement(name: &str, args: Vec<&str>) -> Statement {
        Statement::SimpleCommand(colony_parser::SimpleCommand {
            name: Word::literal(name, Span::DUMMY),
            args: args
                .into_iter()
                .map(|s| Word::literal(s, Span::DUMMY))
                .collect(),
            redirects: vec![],
            env_assignments: vec![],
            span: Span::DUMMY,
        })
    }

    fn make_pipeline(commands: Vec<(&str, Vec<&str>)>) -> Pipeline {
        let mut elements = Vec::new();

        for (i, (name, args)) in commands.into_iter().enumerate() {
            let is_last = i == elements.len();
            elements.push(PipelineElement {
                command: Box::new(make_simple_cmd_statement(name, args)),
                pipe_op: if is_last { None } else { Some(PipeOp::Pipe) },
                negated: false,
            });
        }

        Pipeline {
            elements,
            span: Span::DUMMY,
        }
    }

    #[tokio::test]
    async fn test_simple_pipeline() {
        let context = ExpansionContext::new();

        // echo hello | cat
        let pipeline = make_pipeline(vec![("echo", vec!["hello"]), ("cat", vec![])]);

        let result = execute_pipeline(&pipeline, &mut context.clone()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_success());
    }

    #[tokio::test]
    async fn test_single_command_pipeline() {
        let context = ExpansionContext::new();

        // Just: echo hello
        let pipeline = Pipeline {
            elements: vec![PipelineElement {
                command: Box::new(make_simple_cmd_statement("echo", vec!["hello"])),
                pipe_op: None,
                negated: false,
            }],
            span: Span::DUMMY,
        };

        let result = execute_pipeline(&pipeline, &mut context.clone()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_success());
    }
}
