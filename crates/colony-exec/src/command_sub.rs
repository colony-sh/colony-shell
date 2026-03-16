//! Command substitution $(cmd) execution
//!
//! Executes a statement and captures its stdout output.
//! Used by the expansion system for command substitution.

use crate::{execute_builtin, Result};
use colony_channels::ChannelManager;
use colony_expand::ExpansionContext;
use colony_parser::Statement;
use colony_tuple::TupleSpace;
use nix::unistd::{dup2, fork, pipe, read, ForkResult};
use std::os::fd::{AsRawFd, OwnedFd};

pub fn execute_statement_for_process_sub(stmt: &Statement, context: &mut ExpansionContext) -> i32 {
    execute_statement_in_child(stmt, context)
}

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
                    Ok(n) => output.extend_from_slice(&buffer[..n]),
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

                    // Update $? with the exit status
                    use nix::sys::wait::WaitStatus;
                    let exit_code = match status {
                        WaitStatus::Exited(_, code) => code,
                        WaitStatus::Signaled(_, sig, _) => 128 + sig as i32,
                        _ => 1,
                    };
                    context.set_last_exit_status(exit_code);

                    Ok(output_str)
                }
                Err(e) => Err(format!("waitpid() failed: {}", e)),
            }
        }
    }
}

/// Result type for execute_statement_in_child_with_return - tracks return vs normal exit
enum ChildExitResult {
    /// Normal exit with code
    Exit(i32),
    /// Return statement encountered with code (should exit subshell)
    Return(i32),
}

/// Execute a statement in the child process (for command substitution)
///
/// This is called after fork() in the child, so it doesn't return.
/// Returns an exit code which the child should exit with.
fn execute_statement_in_child(stmt: &Statement, context: &mut ExpansionContext) -> i32 {
    match execute_statement_in_child_with_return(stmt, context) {
        ChildExitResult::Exit(code) => code,
        ChildExitResult::Return(code) => code,
    }
}

/// Execute a statement tracking return vs normal exit
fn execute_statement_in_child_with_return(
    stmt: &Statement,
    context: &mut ExpansionContext,
) -> ChildExitResult {
    use crate::JobTable;
    use colony_expand::Expander;
    use std::io::Write;

    match stmt {
        Statement::Empty => ChildExitResult::Exit(context.last_exit_status()),

        Statement::Return(ret_stmt) => {
            let code = if let Some(ref expr) = ret_stmt.value {
                use colony_parser::Expression;
                match expr {
                    Expression::Int(n, _) => *n as i32,
                    _ => 0,
                }
            } else {
                context.last_exit_status()
            };
            ChildExitResult::Return(code)
        }

        Statement::SimpleCommand(cmd) => {
            let mut expander = Expander::with_executor_and_expr(
                context.clone(),
                execute_and_capture,
                |expr, ctx| crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
            );

            let name_fields = match expander.expand_word(&cmd.name) {
                Ok(fields) if !fields.is_empty() => fields,
                _ => return ChildExitResult::Exit(1),
            };
            let command_name = &name_fields[0].text;
            let extra_args_from_name: Vec<String> =
                name_fields[1..].iter().map(|f| f.text.clone()).collect();

            if let Some(func_def) = context.get_function(command_name).cloned() {
                let mut args = extra_args_from_name.clone();
                for arg_word in &cmd.args {
                    if let Ok(fields) = expander.expand_word(arg_word) {
                        for field in fields {
                            args.push(field.text);
                        }
                    }
                }

                context.set_positional_params(args);

                let mut last_exit = 0;
                for body_stmt in &func_def.body {
                    match execute_statement_in_child_with_return(body_stmt, context) {
                        ChildExitResult::Exit(code) => {
                            last_exit = code;
                            context.set_last_exit_status(last_exit);
                        }
                        ChildExitResult::Return(code) => {
                            // Return from function - just exit with code, don't propagate
                            return ChildExitResult::Exit(code);
                        }
                    }
                }
                return ChildExitResult::Exit(last_exit);
            }

            let jobs = JobTable::new();
            let tuple_space = TupleSpace::new();
            let session_id = std::process::id().to_string();
            let mut channels = ChannelManager::new();

            if let Err(e) = crate::apply_redirects(&cmd.redirects, context) {
                eprintln!("redirect error: {}", e);
                return ChildExitResult::Exit(1);
            }

            if let Some(result) = execute_builtin(
                cmd,
                context,
                &jobs,
                &tuple_space,
                &session_id,
                &mut channels,
                None,
                None,
            ) {
                let _ = std::io::stdout().flush();

                match result {
                    Ok(builtin_result) => ChildExitResult::Exit(builtin_result.exit_code()),
                    Err(crate::ExecutionError::Return(code)) => ChildExitResult::Return(code),
                    Err(_e) => ChildExitResult::Exit(1),
                }
            } else {
                match execute_command_in_child_blocking(cmd, context) {
                    Ok(status) => ChildExitResult::Exit(status.code()),
                    Err(_e) => ChildExitResult::Exit(1),
                }
            }
        }

        Statement::Pipeline(pipeline) => {
            match execute_pipeline_in_child_blocking(pipeline, context) {
                Ok(status) => ChildExitResult::Exit(status.code()),
                Err(crate::ExecutionError::Return(code)) => ChildExitResult::Return(code),
                Err(_) => ChildExitResult::Exit(1),
            }
        }

        Statement::List(list) => {
            let first_result = execute_statement_in_child_with_return(&list.first, context);
            let mut last_code = match first_result {
                ChildExitResult::Exit(code) => code,
                ChildExitResult::Return(code) => return ChildExitResult::Return(code),
            };

            for (op, stmt) in &list.rest {
                use colony_parser::ListOp;
                let should_run = match op {
                    ListOp::Semi => true,
                    ListOp::And => last_code == 0,
                    ListOp::Or => last_code != 0,
                    ListOp::Background => true,
                };

                if should_run {
                    match execute_statement_in_child_with_return(stmt, context) {
                        ChildExitResult::Exit(code) => {
                            last_code = code;
                        }
                        ChildExitResult::Return(code) => {
                            return ChildExitResult::Return(code);
                        }
                    }
                }
            }

            ChildExitResult::Exit(last_code)
        }

        Statement::BraceGroup(brace_group) => {
            let mut last_exit = 0;
            for stmt in &brace_group.body {
                match execute_statement_in_child_with_return(stmt, context) {
                    ChildExitResult::Exit(code) => {
                        last_exit = code;
                        context.set_last_exit_status(last_exit);
                    }
                    ChildExitResult::Return(code) => {
                        return ChildExitResult::Return(code);
                    }
                }
            }
            ChildExitResult::Exit(last_exit)
        }

        Statement::Try(try_stmt) => {
            let mut last_exit = context.last_exit_status();
            for stmt in &try_stmt.body {
                match execute_statement_in_child_with_return(stmt, context) {
                    ChildExitResult::Exit(code) => {
                        last_exit = code;
                        context.set_last_exit_status(last_exit);
                    }
                    ChildExitResult::Return(code) => {
                        return ChildExitResult::Return(code);
                    }
                }
            }

            if let Some(finally_body) = &try_stmt.finally_body {
                for stmt in finally_body {
                    match execute_statement_in_child_with_return(stmt, context) {
                        ChildExitResult::Exit(code) => {
                            last_exit = code;
                            context.set_last_exit_status(last_exit);
                        }
                        ChildExitResult::Return(code) => {
                            return ChildExitResult::Return(code);
                        }
                    }
                }
            }

            ChildExitResult::Exit(last_exit)
        }

        _other => ChildExitResult::Exit(1),
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
    let mut expander =
        Expander::with_executor_and_expr(context.clone(), execute_and_capture, |expr, ctx| {
            crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string())
        });

    // Expand command name
    let name_fields = expander.expand_word(&cmd.name)?;
    if name_fields.is_empty() {
        return Err(crate::ExecutionError::CommandNotFound(
            "<empty>".to_string(),
        ));
    }
    let command_name = &name_fields[0].text;
    let extra_args_from_name: Vec<String> =
        name_fields[1..].iter().map(|f| f.text.clone()).collect();

    // Expand arguments
    let mut args = vec![command_name.clone()];
    args.extend(extra_args_from_name);
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

    let mut env = crate::build_env(context);

    for assignment in &cmd.env_assignments {
        if assignment.subscript.is_none() {
            if let colony_parser::AssignmentValue::Word(word) = &assignment.value {
                if let Ok(value) = expander.expand_word_for_assignment(word) {
                    env.retain(|(k, _)| k != &assignment.name);
                    env.push((assignment.name.clone(), value));
                }
            }
        }
    }

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
        .map_err(|e| crate::ExecutionError::IoError(std::io::Error::other(e)))?;

    // Never reached
    unreachable!()
}

fn execute_pipeline_in_child_blocking(
    pipeline: &colony_parser::Pipeline,
    context: &mut ExpansionContext,
) -> Result<crate::ExitStatus> {
    use nix::unistd::{dup2, fork, pipe, ForkResult};

    use std::os::fd::AsRawFd;

    let num_cmds = pipeline.elements.len();

    if num_cmds == 0 {
        return Ok(crate::ExitStatus::success());
    }

    if num_cmds == 1 {
        return execute_single_element_blocking(&pipeline.elements[0], context);
    }

    let mut prev_read_fd: Option<OwnedFd> = None;
    let mut child_pids = Vec::new();

    for (i, element) in pipeline.elements.iter().enumerate() {
        let is_last = i == num_cmds - 1;

        let (read_fd, write_fd) = if !is_last {
            let (r, w) =
                pipe().map_err(|e| crate::ExecutionError::IoError(std::io::Error::other(e)))?;
            (Some(r), Some(w))
        } else {
            (None, None)
        };

        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                if let Some(prev_fd) = prev_read_fd {
                    dup2(prev_fd.as_raw_fd(), 0).expect("dup2 stdin failed");
                    drop(prev_fd);
                }

                if let Some(ref wfd) = write_fd {
                    dup2(wfd.as_raw_fd(), 1).expect("dup2 stdout failed");
                }
                drop(write_fd);
                drop(read_fd);

                let exit_code = execute_statement_in_child(&element.command, context);
                std::process::exit(exit_code);
            }
            Ok(ForkResult::Parent { child }) => {
                child_pids.push(child);

                drop(prev_read_fd);
                drop(write_fd);

                prev_read_fd = read_fd;
            }
            Err(e) => {
                return Err(crate::ExecutionError::IoError(std::io::Error::other(e)));
            }
        }
    }

    drop(prev_read_fd);

    let mut last_status = crate::ExitStatus::success();
    for pid in child_pids {
        match nix::sys::wait::waitpid(pid, None) {
            Ok(nix::sys::wait::WaitStatus::Exited(_, code)) => {
                last_status = crate::ExitStatus::new(code);
            }
            Ok(_) => {
                last_status = crate::ExitStatus::new(1);
            }
            Err(_) => {}
        }
    }

    Ok(last_status)
}

fn execute_single_element_blocking(
    element: &colony_parser::PipelineElement,
    context: &mut ExpansionContext,
) -> Result<crate::ExitStatus> {
    use crate::JobTable;
    use colony_channels::ChannelManager;
    use colony_expand::Expander;
    use colony_tuple::TupleSpace;
    use std::io::Write;

    if let colony_parser::Statement::SimpleCommand(cmd) = &*element.command {
        let mut expander =
            Expander::with_executor_and_expr(context.clone(), execute_and_capture, |expr, ctx| {
                crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string())
            });

        let name_fields = match expander.expand_word(&cmd.name) {
            Ok(fields) if !fields.is_empty() => fields,
            _ => {
                return Err(crate::ExecutionError::CommandNotFound(
                    "<empty>".to_string(),
                ))
            }
        };
        let command_name = &name_fields[0].text;
        let extra_args_from_name: Vec<String> =
            name_fields[1..].iter().map(|f| f.text.clone()).collect();

        if let Some(func_def) = context.get_function(command_name).cloned() {
            let mut args = extra_args_from_name;
            for arg_word in &cmd.args {
                if let Ok(fields) = expander.expand_word(arg_word) {
                    for field in fields {
                        args.push(field.text);
                    }
                }
            }

            context.set_positional_params(args);

            let mut last_exit = 0;
            for body_stmt in &func_def.body {
                last_exit = execute_statement_in_child(body_stmt, context);
                context.set_last_exit_status(last_exit);
            }
            return Ok(crate::ExitStatus::new(last_exit));
        }

        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let session_id = std::process::id().to_string();
        let mut channels = ChannelManager::new();

        if let Err(e) = crate::apply_redirects(&cmd.redirects, context) {
            eprintln!("redirect error: {}", e);
            return Ok(crate::ExitStatus::new(1));
        }

        if let Some(result) = execute_builtin(
            cmd,
            context,
            &jobs,
            &tuple_space,
            &session_id,
            &mut channels,
            None,
            None,
        ) {
            let _ = std::io::stdout().flush();
            match result {
                Ok(builtin_result) => Ok(crate::ExitStatus::new(builtin_result.exit_code())),
                Err(_) => Ok(crate::ExitStatus::new(1)),
            }
        } else {
            execute_command_in_child_blocking(cmd, context)
        }
    } else {
        let code = execute_statement_in_child(&element.command, context);
        Ok(crate::ExitStatus::new(code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::pest_api;

    #[test]
    fn test_simple_command_substitution() {
        let mut context = ExpansionContext::new();

        let stmt = pest_api::parse_statement("echo hello").unwrap();
        let output = execute_and_capture(&stmt, &mut context).unwrap();

        assert!(output.contains("hello"));
    }

    #[test]
    fn test_pwd_substitution() {
        let mut context = ExpansionContext::new();

        let stmt = pest_api::parse_statement("pwd").unwrap();
        let result = execute_and_capture(&stmt, &mut context);

        if let Ok(output) = result {
            assert!(!output.trim().is_empty(), "pwd output should not be empty");
            assert!(
                output.trim().starts_with('/') || output.contains(':'),
                "pwd should return a path, got: {:?}",
                output
            );
        }
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
        assert!(
            output.contains("world"),
            "Output should contain 'world', got: {:?}",
            output
        );
    }

    #[test]
    fn test_multiline_command_substitution_semicolon() {
        let mut context = ExpansionContext::new();

        let stmt = pest_api::parse_statement("echo hello; echo world").unwrap();
        let output = execute_and_capture(&stmt, &mut context).unwrap();

        assert!(
            output.contains("hello"),
            "Output should contain 'hello', got: {:?}",
            output
        );
        assert!(
            output.contains("world"),
            "Output should contain 'world', got: {:?}",
            output
        );
    }

    #[test]
    fn test_multiline_command_substitution_newline() {
        let mut context = ExpansionContext::new();

        let program = pest_api::parse("echo hello\necho world").unwrap();

        let mut combined_output = String::new();
        for stmt in &program.statements {
            let output = execute_and_capture(stmt, &mut context).unwrap();
            combined_output.push_str(&output);
        }

        assert!(
            combined_output.contains("hello"),
            "Output should contain 'hello', got: {:?}",
            combined_output
        );
        assert!(
            combined_output.contains("world"),
            "Output should contain 'world', got: {:?}",
            combined_output
        );
    }

    #[test]
    fn test_function_in_command_substitution() {
        use colony_parser::{FunctionDef, Span};

        let mut context = ExpansionContext::new();

        let func_body = pest_api::parse_statement("echo hello_from_func").unwrap();
        let func_def = FunctionDef {
            name: "myfunc".to_string(),
            body: vec![func_body],
            span: Span::DUMMY,
        };
        context.define_function("myfunc".to_string(), func_def);

        let stmt = pest_api::parse_statement("myfunc").unwrap();
        let output = execute_and_capture(&stmt, &mut context).unwrap();

        assert!(
            output.contains("hello_from_func"),
            "Function output not captured. Got: {:?}",
            output
        );
    }
}
