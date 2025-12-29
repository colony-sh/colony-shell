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

pub async fn execute_pipeline(
    pipeline: &Pipeline,
    context: &mut ExpansionContext,
) -> Result<ExitStatus> {
    let num_commands = pipeline.elements.len();
    let use_lastpipe = context.shopt_options().lastpipe && num_commands > 1;
    
    if use_lastpipe {
        execute_pipeline_with_lastpipe(pipeline, context).await
    } else {
        let pipeline_clone = pipeline.clone();
        let context_clone = context.clone();

        tokio::task::spawn_blocking(move || execute_pipeline_blocking(&pipeline_clone, &context_clone))
            .await
            .map_err(|e| ExecutionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?
    }
}

async fn execute_pipeline_with_lastpipe(
    pipeline: &Pipeline,
    context: &mut ExpansionContext,
) -> Result<ExitStatus> {
    let num_commands = pipeline.elements.len();
    let is_negated = pipeline.elements.first().map_or(false, |e| e.negated);
    
    let pipeline_clone = pipeline.clone();
    let context_clone = context.clone();
    
    let (pids, last_pipe_read_fd) = tokio::task::spawn_blocking(move || {
        execute_pipeline_head_blocking(&pipeline_clone, &context_clone)
    })
    .await
    .map_err(|e| ExecutionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))??;
    
    let last_element = &pipeline.elements[num_commands - 1];
    let saved_stdin = nix::unistd::dup(0).ok();
    
    if let Err(e) = dup2(last_pipe_read_fd, 0) {
        if let Some(fd) = saved_stdin {
            let _ = close(fd);
        }
        let _ = close(last_pipe_read_fd);
        return Err(ExecutionError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other, 
            format!("lastpipe: failed to redirect stdin: {}", e)
        )));
    }
    
    let _ = close(last_pipe_read_fd);
    
    let last_status = execute_single_pipeline_element_in_current_shell(last_element, context).await;
    
    if let Some(fd) = saved_stdin {
        let _ = dup2(fd, 0);
        let _ = close(fd);
    }
    
    tokio::task::spawn_blocking(move || {
        for pid in pids {
            let _ = waitpid(pid, None);
        }
    })
    .await
    .map_err(|e| ExecutionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    
    let status = last_status?;
    Ok(negate_exit_status(status, is_negated))
}

fn execute_pipeline_head_blocking(
    pipeline: &Pipeline,
    context: &ExpansionContext,
) -> Result<(Vec<Pid>, RawFd)> {
    let num_commands = pipeline.elements.len();
    
    let mut pipes: Vec<(RawFd, RawFd)> = Vec::new();
    for _ in 0..(num_commands - 1) {
        let pipe_fds = pipe().map_err(ExecutionError::ForkError)?;
        let read_end = pipe_fds.0.as_raw_fd();
        let write_end = pipe_fds.1.as_raw_fd();
        pipes.push((read_end, write_end));
        std::mem::forget(pipe_fds.0);
        std::mem::forget(pipe_fds.1);
    }
    
    let mut pids: Vec<Pid> = Vec::new();
    
    for (i, element) in pipeline.elements.iter().take(num_commands - 1).enumerate() {
        match unsafe { fork() }.map_err(ExecutionError::ForkError)? {
            ForkResult::Parent { child } => {
                pids.push(child);
            }
            ForkResult::Child => {
                setup_pipeline_fds(i, num_commands, &pipes);
                
                for (read_fd, write_fd) in &pipes {
                    let _ = close(*read_fd);
                    let _ = close(*write_fd);
                }
                
                execute_pipeline_element_in_child(element, context);
            }
        }
    }
    
    for (i, (read_fd, write_fd)) in pipes.iter().enumerate() {
        let _ = close(*write_fd);
        if i < pipes.len() - 1 {
            let _ = close(*read_fd);
        }
    }
    
    let last_pipe_read_fd = pipes[pipes.len() - 1].0;
    Ok((pids, last_pipe_read_fd))
}

async fn execute_single_pipeline_element_in_current_shell(
    element: &PipelineElement,
    context: &mut ExpansionContext,
) -> Result<ExitStatus> {
    match &*element.command {
        Statement::SimpleCommand(cmd) => {
            let mut executor = crate::Executor::new(context.clone());
            let status = executor.execute_simple_command(cmd).await?;
            *context = executor.into_context();
            Ok(status)
        }
        Statement::For(for_stmt) => {
            let mut executor = crate::Executor::new(context.clone());
            let code = executor.execute_for_loop_internal(for_stmt).await.unwrap_or(1);
            *context = executor.into_context();
            Ok(ExitStatus::new(code))
        }
        Statement::If(if_stmt) => {
            let mut executor = crate::Executor::new(context.clone());
            let code = executor.execute_if_internal(if_stmt).await.unwrap_or(1);
            *context = executor.into_context();
            Ok(ExitStatus::new(code))
        }
        Statement::While(while_stmt) => {
            let mut executor = crate::Executor::new(context.clone());
            let code = executor.execute_while_internal(while_stmt).await.unwrap_or(1);
            *context = executor.into_context();
            Ok(ExitStatus::new(code))
        }
        Statement::BraceGroup(brace) => {
            let mut executor = crate::Executor::new(context.clone());
            let mut last = 0;
            for stmt in &brace.body {
                last = executor.execute_statement_recursive(stmt).await.unwrap_or(1);
            }
            *context = executor.into_context();
            Ok(ExitStatus::new(last))
        }
        Statement::Subshell(subshell) => {
            let mut subshell_context = context.clone();
            subshell_context.clear_non_ignored_traps();
            let mut executor = crate::Executor::new(subshell_context);
            let mut last = 0;
            for stmt in &subshell.body {
                last = executor.execute_statement_recursive(stmt).await.unwrap_or(1);
            }
            executor.run_exit_trap().await;
            Ok(ExitStatus::new(last))
        }
        _ => Err(ExecutionError::CommandNotFound("<complex>".to_string())),
    }
}

/// Execute pipeline synchronously (blocking)
fn execute_pipeline_blocking(pipeline: &Pipeline, context: &ExpansionContext) -> Result<ExitStatus> {
    let num_commands = pipeline.elements.len();

    if num_commands == 0 {
        return Ok(ExitStatus::success());
    }

    let is_negated = pipeline.elements.first().map_or(false, |e| e.negated);
    let pipefail = context.shell_options().pipefail;

    if num_commands == 1 {
        let element = &pipeline.elements[0];
        let status = execute_single_pipeline_element(element, context)?;
        return Ok(negate_exit_status(status, element.negated));
    }

    // Create pipes: N commands need N-1 pipes
    let mut pipes: Vec<(RawFd, RawFd)> = Vec::new();
    for _ in 0..(num_commands - 1) {
        let pipe_fds = pipe().map_err(ExecutionError::ForkError)?;
        let read_end = pipe_fds.0.as_raw_fd();
        let write_end = pipe_fds.1.as_raw_fd();
        pipes.push((read_end, write_end));
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
                setup_pipeline_fds(i, num_commands, &pipes);

                for (read_fd, write_fd) in &pipes {
                    let _ = close(*read_fd);
                    let _ = close(*write_fd);
                }

                execute_pipeline_element_in_child(element, context);
            }
        }
    }

    // Parent: close all pipe ends
    for (read_fd, write_fd) in pipes {
        let _ = close(read_fd);
        let _ = close(write_fd);
    }

    // Wait for all children and collect exit statuses
    let mut statuses: Vec<i32> = Vec::with_capacity(pids.len());
    for pid in pids {
        match waitpid(pid, None) {
            Ok(WaitStatus::Exited(_pid, code)) => {
                statuses.push(code);
            }
            Ok(WaitStatus::Signaled(_pid, signal, _core_dumped)) => {
                statuses.push(128 + signal as i32);
            }
            _ => {
                statuses.push(0);
            }
        }
    }

    let final_status = if pipefail {
        statuses.iter().rev().find(|&&c| c != 0).copied().unwrap_or(0)
    } else {
        *statuses.last().unwrap_or(&0)
    };

    Ok(negate_exit_status(ExitStatus::new(final_status), is_negated))
}

fn negate_exit_status(status: ExitStatus, negated: bool) -> ExitStatus {
    if negated {
        if status.code() == 0 {
            ExitStatus::new(1)
        } else {
            ExitStatus::success()
        }
    } else {
        status
    }
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

/// Execute a statement in a child process (blocking, no async)
///
/// This is used to execute function bodies in pipeline elements.
/// Since we're after fork(), we can't use async/tokio.
fn execute_statement_in_child_blocking(
    stmt: &Statement,
    context: &mut ExpansionContext,
) -> Result<i32> {
    match stmt {
        Statement::SimpleCommand(cmd) => {
            execute_simple_command_in_child_blocking(cmd, context)
        }
        Statement::Pipeline(pipeline) => {
            let mut last_code = 0;
            for element in &pipeline.elements {
                last_code = execute_statement_in_child_blocking(&element.command, context)?;
            }
            Ok(last_code)
        }
        Statement::List(list) => {
            use colony_parser::ListOp;
            let mut last_code = execute_statement_in_child_blocking(&list.first, context)?;
            context.set_last_exit_status(last_code);
            
            for (op, stmt) in &list.rest {
                let should_exec = match op {
                    ListOp::Semi => true,
                    ListOp::And => last_code == 0,
                    ListOp::Or => last_code != 0,
                    ListOp::Background => false,
                };
                
                if should_exec {
                    last_code = execute_statement_in_child_blocking(stmt, context)?;
                    context.set_last_exit_status(last_code);
                }
            }
            Ok(last_code)
        }
        Statement::Return(ret_stmt) => {
            let code = if let Some(ref expr) = ret_stmt.value {
                use colony_parser::Expression;
                match expr {
                    Expression::Int(n, _) => *n as i32,
                    _ => 0
                }
            } else {
                context.last_exit_status()
            };
            Err(crate::ExecutionError::Return(code))
        }
        Statement::For(for_stmt) => {
            execute_for_in_child_blocking(for_stmt, context)
        }
        Statement::If(if_stmt) => {
            execute_if_in_child_blocking(if_stmt, context)
        }
        Statement::While(while_stmt) => {
            execute_while_in_child_blocking(while_stmt, context)
        }
        Statement::BraceGroup(brace) => {
            let mut last = 0;
            for stmt in &brace.body {
                match execute_statement_in_child_blocking(stmt, context) {
                    Ok(code) => {
                        last = code;
                        context.set_last_exit_status(code);
                    }
                    Err(crate::ExecutionError::Return(code)) => return Err(crate::ExecutionError::Return(code)),
                    Err(e) => return Err(e),
                }
            }
            Ok(last)
        }
        Statement::Subshell(subshell) => {
            let mut last = 0;
            for stmt in &subshell.body {
                match execute_statement_in_child_blocking(stmt, context) {
                    Ok(code) => {
                        last = code;
                        context.set_last_exit_status(code);
                    }
                    Err(crate::ExecutionError::Return(code)) => return Err(crate::ExecutionError::Return(code)),
                    Err(e) => return Err(e),
                }
            }
            Ok(last)
        }
        Statement::FunctionDef(func_def) => {
            context.define_function(func_def.name.clone(), func_def.clone());
            Ok(0)
        }
        Statement::Empty => Ok(0),
        _ => {
            eprintln!("colony: unsupported statement type in function body");
            Ok(1)
        }
    }
}

fn execute_simple_command_in_child_blocking(
    cmd: &colony_parser::SimpleCommand,
    context: &mut ExpansionContext,
) -> Result<i32> {
    use colony_parser::AssignmentValue;
    
    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
        crate::execute_and_capture(stmt, ctx)
    });
    
    let is_assignment_only = cmd.name.as_literal() == Some(":")
        && cmd.args.is_empty()
        && !cmd.env_assignments.is_empty();
    
    if is_assignment_only {
        for assignment in &cmd.env_assignments {
            if assignment.subscript.is_none() {
                match &assignment.value {
                    AssignmentValue::Word(word) => {
                        let value = expander.expand_word_for_assignment(word)
                            .unwrap_or_default();
                        if assignment.op == colony_parser::AssignOp::Append {
                            let existing = context.get_var_or_env(&assignment.name).unwrap_or_default();
                            context.set_var(assignment.name.clone(), existing + &value);
                        } else {
                            context.set_var(assignment.name.clone(), value);
                        }
                    }
                    AssignmentValue::Array(words) => {
                        let mut values = Vec::new();
                        for word in words {
                            if let Ok(fields) = expander.expand_word(word) {
                                for field in fields {
                                    values.push(field.text);
                                }
                            }
                        }
                        context.set_variable(assignment.name.clone(), colony_expand::Variable::Array(values));
                    }
                    AssignmentValue::AssocArray(entries) => {
                        let mut map = std::collections::HashMap::new();
                        for (k, v) in entries {
                            map.insert(k.clone(), v.clone());
                        }
                        context.set_variable(assignment.name.clone(), colony_expand::Variable::AssocArray(map));
                    }
                }
            }
        }
        let exit_from_expansion = expander.context().last_exit_status();
        context.set_last_exit_status(exit_from_expansion);
        return Ok(exit_from_expansion);
    }
    
    let name_fields = match expander.expand_word(&cmd.name) {
        Ok(fields) if !fields.is_empty() => fields,
        _ => return Err(crate::ExecutionError::CommandNotFound("<empty>".to_string())),
    };
    let command_name = &name_fields[0].text;
    let extra_args_from_name: Vec<String> = name_fields[1..].iter().map(|f| f.text.clone()).collect();
    
    if let Some(func_def) = context.get_function(command_name).cloned() {
        let mut args = extra_args_from_name.clone();
        for arg_word in &cmd.args {
            if let Ok(fields) = expander.expand_word(arg_word) {
                for field in fields {
                    args.push(field.text);
                }
            }
        }
        
        crate::xtrace::trace_simple_command(context, command_name, &args);
        context.set_positional_params(args);
        
        let mut last_exit = 0;
        for stmt in &func_def.body {
            match execute_statement_in_child_blocking(stmt, context) {
                Ok(code) => {
                    last_exit = code;
                    context.set_last_exit_status(code);
                }
                Err(crate::ExecutionError::Return(code)) => {
                    return Ok(code);
                }
                Err(e) => return Err(e),
            }
        }
        return Ok(last_exit);
    }
    
    // Check for alias expansion in pipeline context
    let expanding_aliases = std::collections::HashSet::new();
    match crate::alias::expand_alias(command_name, context, &expanding_aliases) {
        crate::alias::AliasExpansionResult::Reparsed(statements) => {
            let mut last_exit = 0;
            for (i, stmt) in statements.iter().enumerate() {
                if i == statements.len() - 1 {
                    if let colony_parser::Statement::SimpleCommand(mut last_cmd) = stmt.clone() {
                        last_cmd.args.extend(cmd.args.clone());
                        last_cmd.redirects.extend(cmd.redirects.clone());
                        last_exit = execute_statement_in_child_blocking(
                            &colony_parser::Statement::SimpleCommand(last_cmd), 
                            context
                        )?;
                    } else {
                        last_exit = execute_statement_in_child_blocking(stmt, context)?;
                    }
                } else {
                    last_exit = execute_statement_in_child_blocking(stmt, context)?;
                }
                context.set_last_exit_status(last_exit);
            }
            return Ok(last_exit);
        }
        crate::alias::AliasExpansionResult::SimpleExpansion { new_command, alias_args, expand_next } => {
            let mut new_cmd = cmd.clone();
            new_cmd.name = colony_parser::Word::literal(&new_command, cmd.span.clone());
            
            let mut new_args = Vec::new();
            for alias_arg in &alias_args {
                new_args.push(colony_parser::Word::literal(alias_arg, cmd.span.clone()));
            }
            
            // Handle trailing space expansion
            let mut skip_first_arg = false;
            if expand_next && !cmd.args.is_empty() {
                if let Some(first_arg) = cmd.args.first() {
                    if let Some(arg_name) = first_arg.as_literal() {
                        let mut exp_set = expanding_aliases.clone();
                        exp_set.insert(command_name.to_string());
                        if let crate::alias::AliasExpansionResult::SimpleExpansion { 
                            new_command: arg_cmd, 
                            alias_args: arg_alias_args, 
                            .. 
                        } = crate::alias::expand_alias(arg_name, context, &exp_set) {
                            new_args.push(colony_parser::Word::literal(&arg_cmd, first_arg.span.clone()));
                            for arg in &arg_alias_args {
                                new_args.push(colony_parser::Word::literal(arg, first_arg.span.clone()));
                            }
                            skip_first_arg = true;
                        }
                    }
                }
            }
            
            let args_to_add = if skip_first_arg { &cmd.args[1..] } else { &cmd.args[..] };
            new_args.extend(args_to_add.iter().cloned());
            new_cmd.args = new_args;
            
            return execute_simple_command_in_child_blocking(&new_cmd, context);
        }
        crate::alias::AliasExpansionResult::CycleDetected | crate::alias::AliasExpansionResult::NoAlias => {}
    }
    
    if crate::builtins::is_builtin(command_name) {
        let jobs = crate::JobTable::new();
        let tuple_space = colony_tuple::TupleSpace::new();
        let mut channels = colony_channels::ChannelManager::new();
        
        let mut builtin_args = extra_args_from_name.clone();
        for arg_word in &cmd.args {
            if let Ok(fields) = expander.expand_word(arg_word) {
                for field in fields {
                    builtin_args.push(field.text);
                }
            }
        }
        crate::xtrace::trace_simple_command(context, command_name, &builtin_args);
        
        if let Some(result) = crate::builtins::execute_builtin(
            cmd, 
            context, 
            &jobs, 
            &tuple_space, 
            &mut channels
        ) {
            match result {
                Ok(builtin_result) => return Ok(builtin_result.exit_code()),
                Err(e) => return Err(e),
            }
        }
    }
    
    let mut args = vec![command_name.clone()];
    args.extend(extra_args_from_name);
    for arg_word in &cmd.args {
        if let Ok(fields) = expander.expand_word(arg_word) {
            for field in fields {
                args.push(field.text);
            }
        }
    }
    
    crate::xtrace::trace_simple_command(context, command_name, &args[1..]);
    
    let path_env = context.get_env("PATH");
    let program_path = match resolve_command(command_name, path_env) {
        Ok(path) => path,
        Err(e) => return Err(e),
    };
    
    match unsafe { fork() }.map_err(crate::ExecutionError::ForkError)? {
        ForkResult::Child => {
            let _ = crate::apply_redirects(&cmd.redirects, &mut context.clone());
    let mut env = crate::build_env(&context);
            
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
            
            let c_program = CString::new(program_path.as_os_str().as_bytes()).unwrap();
            let c_args: Vec<CString> = args
                .iter()
                .map(|s| CString::new(s.as_bytes()).unwrap())
                .collect();
            let c_env: Vec<CString> = env
                .iter()
                .map(|(k, v)| CString::new(format!("{}={}", k, v).as_bytes()).unwrap())
                .collect();
            
            let _ = execve(&c_program, &c_args, &c_env);
            std::process::exit(127);
        }
        ForkResult::Parent { child } => {
            match waitpid(child, None) {
                Ok(WaitStatus::Exited(_, code)) => Ok(code),
                Ok(WaitStatus::Signaled(_, sig, _)) => Ok(128 + sig as i32),
                _ => Ok(1),
            }
        }
    }
}

fn execute_for_in_child_blocking(
    for_stmt: &colony_parser::ForStatement,
    context: &mut ExpansionContext,
) -> Result<i32> {
    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
        crate::execute_and_capture(stmt, ctx)
    });
    
    let mut words_expanded = Vec::new();
    for word in &for_stmt.words {
        if let Ok(fields) = expander.expand_word(word) {
            for field in fields {
                words_expanded.push(field.text);
            }
        }
    }
    
    let mut last_exit = 0;
    
    for value in words_expanded {
        context.set_var(for_stmt.var.clone(), value);
        
        for stmt in &for_stmt.body {
            match execute_statement_in_child_blocking(stmt, context) {
                Ok(code) => {
                    last_exit = code;
                    context.set_last_exit_status(code);
                }
                Err(crate::ExecutionError::Return(code)) => {
                    return Err(crate::ExecutionError::Return(code));
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    Ok(last_exit)
}

fn execute_if_in_child_blocking(
    if_stmt: &colony_parser::IfStatement,
    context: &mut ExpansionContext,
) -> Result<i32> {
    let cond_exit = execute_condition_in_child_blocking(&if_stmt.condition, context)?;
    
    if cond_exit == 0 {
        let mut last = 0;
        for stmt in &if_stmt.then_body {
            match execute_statement_in_child_blocking(stmt, context) {
                Ok(code) => last = code,
                Err(crate::ExecutionError::Return(code)) => return Err(crate::ExecutionError::Return(code)),
                Err(e) => return Err(e),
            }
        }
        return Ok(last);
    }
    
    for elif in &if_stmt.elif_branches {
        let cond_exit = execute_condition_in_child_blocking(&elif.condition, context)?;
        if cond_exit == 0 {
            let mut last = 0;
            for stmt in &elif.body {
                match execute_statement_in_child_blocking(stmt, context) {
                    Ok(code) => last = code,
                    Err(crate::ExecutionError::Return(code)) => return Err(crate::ExecutionError::Return(code)),
                    Err(e) => return Err(e),
                }
            }
            return Ok(last);
        }
    }
    
    if let Some(ref else_body) = if_stmt.else_body {
        let mut last = 0;
        for stmt in else_body {
            match execute_statement_in_child_blocking(stmt, context) {
                Ok(code) => last = code,
                Err(crate::ExecutionError::Return(code)) => return Err(crate::ExecutionError::Return(code)),
                Err(e) => return Err(e),
            }
        }
        return Ok(last);
    }
    
    Ok(0)
}

fn execute_while_in_child_blocking(
    while_stmt: &colony_parser::WhileStatement,
    context: &mut ExpansionContext,
) -> Result<i32> {
    let mut last_exit = 0;
    
    loop {
        let cond_exit = execute_condition_in_child_blocking(&while_stmt.condition, context)?;
        if cond_exit != 0 {
            break;
        }
        
        for stmt in &while_stmt.body {
            match execute_statement_in_child_blocking(stmt, context) {
                Ok(code) => {
                    last_exit = code;
                    context.set_last_exit_status(code);
                }
                Err(crate::ExecutionError::Return(code)) => {
                    return Err(crate::ExecutionError::Return(code));
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    Ok(last_exit)
}

fn execute_condition_in_child_blocking(
    condition: &colony_parser::Condition,
    context: &mut ExpansionContext,
) -> Result<i32> {
    match condition {
        colony_parser::Condition::Command(cmd) => {
            execute_statement_in_child_blocking(cmd, context)
        }
        colony_parser::Condition::Expression(_) => {
            Ok(1)
        }
    }
}

fn execute_pipeline_element_in_child(element: &PipelineElement, context: &ExpansionContext) -> ! {
    let mut context = context.clone();
    match &*element.command {
        Statement::SimpleCommand(_) => {}
        Statement::BraceGroup(brace) => {
            if !brace.redirects.is_empty() {
                if let Err(e) = crate::redirect::apply_redirects(&brace.redirects, &mut context) {
                    eprintln!("colony: redirect error: {}", e);
                    std::process::exit(1);
                }
            }
            match execute_statement_in_child_blocking(&element.command, &mut context) {
                Ok(code) => std::process::exit(code),
                Err(_) => std::process::exit(1),
            }
        }
        Statement::Subshell(subshell) => {
            if !subshell.redirects.is_empty() {
                if let Err(e) = crate::redirect::apply_redirects(&subshell.redirects, &mut context) {
                    eprintln!("colony: redirect error: {}", e);
                    std::process::exit(1);
                }
            }
            match execute_statement_in_child_blocking(&element.command, &mut context) {
                Ok(code) => std::process::exit(code),
                Err(_) => std::process::exit(1),
            }
        }
        Statement::For(_) | Statement::If(_) | Statement::While(_) | Statement::List(_) => {
            match execute_statement_in_child_blocking(&element.command, &mut context) {
                Ok(code) => std::process::exit(code),
                Err(_) => std::process::exit(1),
            }
        }
        _ => {
            eprintln!("colony-exec: unsupported pipeline element type");
            std::process::exit(1);
        }
    }

    let simple_cmd = match &*element.command {
        Statement::SimpleCommand(cmd) => cmd,
        _ => unreachable!(),
    };

    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
        crate::execute_and_capture(stmt, ctx)
    });

    let name_fields = match expander.expand_word(&simple_cmd.name) {
        Ok(fields) if !fields.is_empty() => fields,
        _ => {
            eprintln!("colony-exec: command expansion failed");
            std::process::exit(127);
        }
    };
    let command_name = &name_fields[0].text;
    let extra_args_from_name: Vec<String> = name_fields[1..].iter().map(|f| f.text.clone()).collect();

    if let Some(func_def) = context.get_function(command_name) {
        let mut args = extra_args_from_name.clone();
        for arg_word in &simple_cmd.args {
            if let Ok(fields) = expander.expand_word(arg_word) {
                for field in fields {
                    args.push(field.text);
                }
            }
        }
        
        crate::xtrace::trace_simple_command(&context, command_name, &args);
        
        let mut context = context.clone();
        context.set_positional_params(args);
        
        let mut last_exit = 0;
        for stmt in &func_def.body {
            match execute_statement_in_child_blocking(stmt, &mut context) {
                Ok(code) => {
                    last_exit = code;
                    context.set_last_exit_status(code);
                }
                Err(_) => {
                    std::process::exit(1);
                }
            }
        }
        std::process::exit(last_exit);
    }

    // Check for alias expansion in pipeline child
    let expanding_aliases = std::collections::HashSet::new();
    match crate::alias::expand_alias(command_name, &context, &expanding_aliases) {
        crate::alias::AliasExpansionResult::Reparsed(statements) => {
            let mut last_exit = 0;
            for (i, stmt) in statements.iter().enumerate() {
                let mut ctx = context.clone();
                if i == statements.len() - 1 {
                    if let colony_parser::Statement::SimpleCommand(mut last_cmd) = stmt.clone() {
                        last_cmd.args.extend(simple_cmd.args.clone());
                        last_cmd.redirects.extend(simple_cmd.redirects.clone());
                        match execute_statement_in_child_blocking(
                            &colony_parser::Statement::SimpleCommand(last_cmd), 
                            &mut ctx
                        ) {
                            Ok(code) => last_exit = code,
                            Err(_) => std::process::exit(1),
                        }
                    } else {
                        match execute_statement_in_child_blocking(stmt, &mut ctx) {
                            Ok(code) => last_exit = code,
                            Err(_) => std::process::exit(1),
                        }
                    }
                } else {
                    match execute_statement_in_child_blocking(stmt, &mut ctx) {
                        Ok(code) => last_exit = code,
                        Err(_) => std::process::exit(1),
                    }
                }
            }
            std::process::exit(last_exit);
        }
        crate::alias::AliasExpansionResult::SimpleExpansion { new_command, alias_args, expand_next } => {
            let mut new_cmd = simple_cmd.clone();
            new_cmd.name = colony_parser::Word::literal(&new_command, simple_cmd.span.clone());
            
            let mut new_args = Vec::new();
            for alias_arg in &alias_args {
                new_args.push(colony_parser::Word::literal(alias_arg, simple_cmd.span.clone()));
            }
            
            let mut skip_first_arg = false;
            if expand_next && !simple_cmd.args.is_empty() {
                if let Some(first_arg) = simple_cmd.args.first() {
                    if let Some(arg_name) = first_arg.as_literal() {
                        let mut exp_set = expanding_aliases.clone();
                        exp_set.insert(command_name.to_string());
                        if let crate::alias::AliasExpansionResult::SimpleExpansion { 
                            new_command: arg_cmd, 
                            alias_args: arg_alias_args, 
                            .. 
                        } = crate::alias::expand_alias(arg_name, &context, &exp_set) {
                            new_args.push(colony_parser::Word::literal(&arg_cmd, first_arg.span.clone()));
                            for arg in &arg_alias_args {
                                new_args.push(colony_parser::Word::literal(arg, first_arg.span.clone()));
                            }
                            skip_first_arg = true;
                        }
                    }
                }
            }
            
            let args_to_add = if skip_first_arg { &simple_cmd.args[1..] } else { &simple_cmd.args[..] };
            new_args.extend(args_to_add.iter().cloned());
            new_cmd.args = new_args;
            
            let mut ctx = context.clone();
            match execute_simple_command_in_child_blocking(&new_cmd, &mut ctx) {
                Ok(code) => std::process::exit(code),
                Err(_) => std::process::exit(1),
            }
        }
        crate::alias::AliasExpansionResult::CycleDetected | crate::alias::AliasExpansionResult::NoAlias => {}
    }

    if crate::builtins::is_builtin(command_name) {
        let mut context = context.clone();
        let jobs = crate::JobTable::new();
        let tuple_space = colony_tuple::TupleSpace::new();
        let mut channels = colony_channels::ChannelManager::new();
        
        let mut builtin_args = extra_args_from_name.clone();
        for arg_word in &simple_cmd.args {
            if let Ok(fields) = expander.expand_word(arg_word) {
                for field in fields {
                    builtin_args.push(field.text);
                }
            }
        }
        crate::xtrace::trace_simple_command(&context, command_name, &builtin_args);
        
        if let Some(result) = crate::builtins::execute_builtin(
            simple_cmd, 
            &mut context, 
            &jobs, 
            &tuple_space, 
            &mut channels
        ) {
            match result {
                Ok(builtin_result) => std::process::exit(builtin_result.exit_code()),
                Err(_) => std::process::exit(1),
            }
        }
    }

    let mut args = vec![command_name.clone()];
    args.extend(extra_args_from_name);
    for arg_word in &simple_cmd.args {
        if let Ok(fields) = expander.expand_word(arg_word) {
            for field in fields {
                args.push(field.text);
            }
        }
    }

    crate::xtrace::trace_simple_command(&context, command_name, &args[1..]);

    let path_env = context.get_env("PATH");
    let program_path = match resolve_command(command_name, path_env) {
        Ok(path) => path,
        Err(_) => {
            eprintln!("colony-exec: {}: command not found", command_name);
            std::process::exit(127);
        }
    };

    let mut env = crate::build_env(&context);
    
    for assignment in &simple_cmd.env_assignments {
        if assignment.subscript.is_none() {
            if let colony_parser::AssignmentValue::Word(word) = &assignment.value {
                if let Ok(value) = expander.expand_word_for_assignment(word) {
                    env.retain(|(k, _)| k != &assignment.name);
                    env.push((assignment.name.clone(), value));
                }
            }
        }
    }
    
    let c_program = CString::new(program_path.as_os_str().as_bytes()).unwrap();
    let c_args: Vec<CString> = args
        .iter()
        .map(|s| CString::new(s.as_bytes()).unwrap())
        .collect();
    let c_env: Vec<CString> = env
        .iter()
        .map(|(k, v)| CString::new(format!("{}={}", k, v).as_bytes()).unwrap())
        .collect();

    let mut ctx = context.clone();
    let _ = crate::apply_redirects(&simple_cmd.redirects, &mut ctx);

    let _ = execve(&c_program, &c_args, &c_env);
    eprintln!("colony-exec: exec failed");
    std::process::exit(127);
}

fn execute_single_pipeline_element(
    element: &PipelineElement,
    context: &ExpansionContext,
) -> Result<ExitStatus> {
    match &*element.command {
        Statement::SimpleCommand(cmd) => {
            let mut context_mut = context.clone();
            tokio::runtime::Handle::current().block_on(async {
                crate::command::execute_command(cmd, &mut context_mut).await
            })
        }
        Statement::For(for_stmt) => {
            let mut context_mut = context.clone();
            tokio::runtime::Handle::current().block_on(async {
                let mut executor = crate::Executor::new(context_mut);
                let code = executor.execute_for_loop_internal(for_stmt).await
                    .unwrap_or(1);
                Ok(ExitStatus::new(code))
            })
        }
        Statement::If(if_stmt) => {
            let mut context_mut = context.clone();
            tokio::runtime::Handle::current().block_on(async {
                let mut executor = crate::Executor::new(context_mut);
                let code = executor.execute_if_internal(if_stmt).await
                    .unwrap_or(1);
                Ok(ExitStatus::new(code))
            })
        }
        Statement::While(while_stmt) => {
            let mut context_mut = context.clone();
            tokio::runtime::Handle::current().block_on(async {
                let mut executor = crate::Executor::new(context_mut);
                let code = executor.execute_while_internal(while_stmt).await
                    .unwrap_or(1);
                Ok(ExitStatus::new(code))
            })
        }
        Statement::BraceGroup(brace) => {
            let mut context_mut = context.clone();
            tokio::runtime::Handle::current().block_on(async {
                let mut executor = crate::Executor::new(context_mut);
                let mut last = 0;
                for stmt in &brace.body {
                    last = executor.execute_statement_recursive(stmt).await.unwrap_or(1);
                }
                Ok(ExitStatus::new(last))
            })
        }
        Statement::Subshell(subshell) => {
            let mut context_mut = context.clone();
            context_mut.clear_non_ignored_traps();
            tokio::runtime::Handle::current().block_on(async {
                let mut executor = crate::Executor::new(context_mut);
                let mut last = 0;
                for stmt in &subshell.body {
                    last = executor.execute_statement_recursive(stmt).await.unwrap_or(1);
                }
                executor.run_exit_trap().await;
                Ok(ExitStatus::new(last))
            })
        }
        _ => Err(ExecutionError::CommandNotFound("<complex>".to_string())),
    }
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
