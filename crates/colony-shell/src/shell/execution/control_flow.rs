use colony_exec::{ExecutionError, Executor};
use colony_expand::Expander;
use colony_parser::Statement;

use crate::shell::engine::{
    evaluate_arithmetic_command, evaluate_conditional, evaluate_cstyle_arith_expr,
    evaluate_cstyle_arith_value, execute_coproc,
};
use crate::shell::execution::background::run_in_background;
use crate::shell::execution::execute_err_trap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlFlow {
    Normal,
    Break,
    Continue,
    Exit,
    Return,
}

pub(crate) async fn execute_list_with_control(
    list: &colony_parser::CommandList,
    executor: &mut Executor,
) -> (i32, ControlFlow) {
    use colony_parser::ListOp;

    let Some((first_op, _)) = list.rest.first() else {
        return execute_statement_with_errexit_control(&list.first, executor, false).await;
    };

    let first_is_background = *first_op == ListOp::Background;
    let first_is_errexit_exempt = matches!(first_op, ListOp::And | ListOp::Or);

    let (mut last_exit_code, mut control) = if first_is_background {
        match run_in_background(&list.first, executor).await {
            Ok((job_id, pid)) => {
                if executor.context().is_interactive() {
                    eprintln!("[{}] {}", job_id, pid);
                }
                (0, ControlFlow::Normal)
            }
            Err(_) => (1, ControlFlow::Normal),
        }
    } else {
        execute_statement_with_errexit_control(&list.first, executor, first_is_errexit_exempt).await
    };
    executor.context_mut().set_last_exit_status(last_exit_code);

    if control != ControlFlow::Normal {
        return (last_exit_code, control);
    }

    for (idx, (op, stmt)) in list.rest.iter().enumerate() {
        let next_is_background = list
            .rest
            .get(idx + 1)
            .map(|(next_op, _)| *next_op == ListOp::Background)
            .unwrap_or(false);
        let next_is_errexit_exempt = list
            .rest
            .get(idx + 1)
            .map(|(next_op, _)| matches!(next_op, ListOp::And | ListOp::Or))
            .unwrap_or(false);
        let this_is_background = *op == ListOp::Background;

        let should_execute = match op {
            ListOp::Semi => true,
            ListOp::And => last_exit_code == 0,
            ListOp::Or => last_exit_code != 0,
            ListOp::Background => true,
        };

        if !should_execute {
            continue;
        }

        if next_is_background || (this_is_background && idx + 1 == list.rest.len()) {
            if this_is_background {
                continue;
            }
            match run_in_background(stmt, executor).await {
                Ok((job_id, pid)) => {
                    if executor.context().is_interactive() {
                        eprintln!("[{}] {}", job_id, pid);
                    }
                    last_exit_code = 0;
                }
                Err(err) => {
                    eprintln!("background start failed: {err}");
                    last_exit_code = 1;
                }
            }
        } else if !this_is_background {
            let (code, flow) =
                execute_statement_with_errexit_control(stmt, executor, next_is_errexit_exempt)
                    .await;
            last_exit_code = code;
            executor.context_mut().set_last_exit_status(last_exit_code);
            control = flow;

            if control != ControlFlow::Normal {
                return (last_exit_code, control);
            }
        }
    }

    (last_exit_code, control)
}

pub(crate) async fn execute_if_with_control(
    if_stmt: &colony_parser::IfStatement,
    executor: &mut Executor,
) -> (i32, ControlFlow) {
    use colony_parser::Condition;

    let saved_errexit = executor.context().shell_options().errexit;
    executor.context_mut().shell_options_mut().errexit = false;
    let (condition_exit, _) = match &if_stmt.condition {
        Condition::Command(cmd) => {
            execute_statement_with_errexit_control(cmd, executor, true).await
        }
        Condition::Expression(_expr) => {
            eprintln!("Expression conditions not yet supported");
            (1, ControlFlow::Normal)
        }
    };
    executor.context_mut().shell_options_mut().errexit = saved_errexit;

    if condition_exit == 0 {
        let mut last_exit = 0;
        for stmt in &if_stmt.then_body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;
            if flow != ControlFlow::Normal {
                return (last_exit, flow);
            }
        }
        return (last_exit, ControlFlow::Normal);
    }

    for elif in &if_stmt.elif_branches {
        let saved_errexit = executor.context().shell_options().errexit;
        executor.context_mut().shell_options_mut().errexit = false;
        let (elif_exit, _) = match &elif.condition {
            Condition::Command(cmd) => {
                execute_statement_with_errexit_control(cmd, executor, true).await
            }
            Condition::Expression(_) => (1, ControlFlow::Normal),
        };
        executor.context_mut().shell_options_mut().errexit = saved_errexit;

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

    if let Some(else_body) = &if_stmt.else_body {
        let mut last_exit = 0;
        for stmt in else_body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;
            if flow != ControlFlow::Normal {
                return (last_exit, flow);
            }
        }
        return (last_exit, ControlFlow::Normal);
    }

    (0, ControlFlow::Normal)
}

pub(crate) async fn execute_for_loop_with_control(
    for_stmt: &colony_parser::ForStatement,
    executor: &mut Executor,
) -> (i32, ControlFlow) {
    let mut expander = Expander::with_executor_and_expr(
        executor.context().clone(),
        colony_exec::execute_and_capture,
        |expr, ctx| colony_exec::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
    );
    let mut values = Vec::new();

    for word in &for_stmt.words {
        match expander.expand_word(word) {
            Ok(fields) => values.extend(fields),
            Err(e) => {
                eprintln!("expansion error: {e}");
                return (1, ControlFlow::Normal);
            }
        }
    }

    let mut last_exit = 0;
    for field in values {
        if executor.context().is_readonly(&for_stmt.var) {
            eprintln!("colony: {}: readonly variable", for_stmt.var);
            return (1, ControlFlow::Normal);
        }

        executor
            .context_mut()
            .set_var(for_stmt.var.clone(), field.text);

        for stmt in &for_stmt.body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;

            match flow {
                ControlFlow::Break => return (last_exit, ControlFlow::Normal),
                ControlFlow::Continue => break,
                ControlFlow::Exit => return (last_exit, ControlFlow::Exit),
                ControlFlow::Return => return (last_exit, ControlFlow::Return),
                ControlFlow::Normal => {}
            }
        }
    }

    (last_exit, ControlFlow::Normal)
}

pub(crate) async fn execute_cstyle_for_loop_with_control(
    cstyle_for: &colony_parser::CStyleForStatement,
    executor: &mut Executor,
) -> i32 {
    if let Some(init) = &cstyle_for.init {
        evaluate_cstyle_arith_expr(init, executor);
    }

    let mut last_exit = 0;

    loop {
        if let Some(cond) = &cstyle_for.condition {
            let cond_result = evaluate_cstyle_arith_value(cond, executor);
            if cond_result == 0 {
                break;
            }
        }

        let mut loop_flow = ControlFlow::Normal;
        for stmt in &cstyle_for.body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;
            loop_flow = flow;
            if !matches!(loop_flow, ControlFlow::Normal) {
                break;
            }
        }

        match loop_flow {
            ControlFlow::Break => break,
            ControlFlow::Exit | ControlFlow::Return => return last_exit,
            ControlFlow::Continue | ControlFlow::Normal => {}
        }

        if let Some(update) = &cstyle_for.update {
            evaluate_cstyle_arith_expr(update, executor);
        }
    }

    last_exit
}

pub(crate) fn execute_statement_with_control<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = (i32, ControlFlow)> + 'a>> {
    execute_statement_with_errexit_control(stmt, executor, false)
}

pub(crate) fn execute_statement_with_errexit_control<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
    errexit_exempt: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = (i32, ControlFlow)> + 'a>> {
    Box::pin(async move {
        match stmt {
            Statement::SimpleCommand(cmd) => {
                if let Some(literal) = cmd.name.as_literal() {
                    if literal == "break" {
                        return (0, ControlFlow::Break);
                    }
                    if literal == "continue" {
                        return (0, ControlFlow::Continue);
                    }
                }

                let result = match executor.execute_simple_command(cmd).await {
                    Ok(status) => {
                        let code = status.code();
                        if !errexit_exempt
                            && executor.context().shell_options().errexit
                            && code != 0
                        {
                            (code, ControlFlow::Exit)
                        } else {
                            (code, ControlFlow::Normal)
                        }
                    }
                    Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                    Err(ExecutionError::Errexit(code)) => (code, ControlFlow::Exit),
                    Err(ref e) => {
                        eprintln!("execution error: {e}");
                        let exit_code = match e {
                            ExecutionError::CommandNotFound(_) => 127,
                            ExecutionError::PermissionDenied(_) => 126,
                            _ => 1,
                        };
                        if !errexit_exempt && executor.context().shell_options().errexit {
                            (exit_code, ControlFlow::Exit)
                        } else {
                            (exit_code, ControlFlow::Normal)
                        }
                    }
                };
                if result.0 != 0 {
                    execute_err_trap(executor, execute_statement_for_code).await;
                }
                result
            }
            Statement::Try(try_stmt) => {
                let result = match executor.execute_try_internal(try_stmt).await {
                    Ok(code) => {
                        if !errexit_exempt
                            && executor.context().shell_options().errexit
                            && code != 0
                        {
                            (code, ControlFlow::Exit)
                        } else {
                            (code, ControlFlow::Normal)
                        }
                    }
                    Err(ExecutionError::Return(code)) => (code, ControlFlow::Return),
                    Err(ExecutionError::Break) => (0, ControlFlow::Break),
                    Err(ExecutionError::Continue) => (0, ControlFlow::Continue),
                    Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                    Err(ExecutionError::Errexit(code)) => (code, ControlFlow::Exit),
                    Err(ref e) => {
                        eprintln!("execution error: {e}");
                        let exit_code = match e {
                            ExecutionError::CommandNotFound(_) => 127,
                            ExecutionError::PermissionDenied(_) => 126,
                            _ => 1,
                        };
                        if !errexit_exempt && executor.context().shell_options().errexit {
                            (exit_code, ControlFlow::Exit)
                        } else {
                            (exit_code, ControlFlow::Normal)
                        }
                    }
                };
                if result.0 != 0 {
                    execute_err_trap(executor, execute_statement_for_code).await;
                }
                result
            }
            Statement::Match(match_stmt) => {
                let result = match executor.execute_match_internal(match_stmt).await {
                    Ok(code) => {
                        if !errexit_exempt
                            && executor.context().shell_options().errexit
                            && code != 0
                        {
                            (code, ControlFlow::Exit)
                        } else {
                            (code, ControlFlow::Normal)
                        }
                    }
                    Err(ExecutionError::Return(code)) => (code, ControlFlow::Return),
                    Err(ExecutionError::Break) => (0, ControlFlow::Break),
                    Err(ExecutionError::Continue) => (0, ControlFlow::Continue),
                    Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                    Err(ExecutionError::Errexit(code)) => (code, ControlFlow::Exit),
                    Err(ref e) => {
                        eprintln!("execution error: {e}");
                        let exit_code = match e {
                            ExecutionError::CommandNotFound(_) => 127,
                            ExecutionError::PermissionDenied(_) => 126,
                            _ => 1,
                        };
                        if !errexit_exempt && executor.context().shell_options().errexit {
                            (exit_code, ControlFlow::Exit)
                        } else {
                            (exit_code, ControlFlow::Normal)
                        }
                    }
                };
                if result.0 != 0 {
                    execute_err_trap(executor, execute_statement_for_code).await;
                }
                result
            }
            Statement::Pipeline(pipeline) => {
                let pipeline_has_negation = pipeline
                    .elements
                    .first()
                    .map(|e| e.negated)
                    .unwrap_or(false);
                if pipeline.elements.len() == 1 {
                    let element = &pipeline.elements[0];
                    let saved_errexit = executor.context().shell_options().errexit;
                    if element.negated {
                        executor.context_mut().shell_options_mut().errexit = false;
                    }
                    let (mut code, flow) = execute_statement_with_errexit_control(
                        &element.command,
                        executor,
                        errexit_exempt || element.negated,
                    )
                    .await;
                    if element.negated {
                        executor.context_mut().shell_options_mut().errexit = saved_errexit;
                        code = if code == 0 { 1 } else { 0 };
                    }
                    executor.context_mut().set_last_exit_status(code);
                    return (code, flow);
                }
                let result = match executor.execute_pipeline(pipeline).await {
                    Ok(status) => {
                        let code = status.code();
                        if !errexit_exempt
                            && !pipeline_has_negation
                            && executor.context().shell_options().errexit
                            && code != 0
                        {
                            (code, ControlFlow::Exit)
                        } else {
                            (code, ControlFlow::Normal)
                        }
                    }
                    Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                    Err(ExecutionError::Errexit(code)) => (code, ControlFlow::Exit),
                    Err(e) => {
                        eprintln!("execution error: {e}");
                        (1, ControlFlow::Normal)
                    }
                };
                if result.0 != 0 && result.1 == ControlFlow::Normal {
                    execute_err_trap(executor, execute_statement_for_code).await;
                }
                result
            }
            Statement::List(list) => execute_list_with_control(list, executor).await,
            Statement::For(for_stmt) => execute_for_loop_with_control(for_stmt, executor).await,
            Statement::CStyleFor(cstyle_for) => (
                execute_cstyle_for_loop_with_control(cstyle_for, executor).await,
                ControlFlow::Normal,
            ),
            Statement::If(if_stmt) => execute_if_with_control(if_stmt, executor).await,
            Statement::While(_) | Statement::Until(_) | Statement::Case(_) => {
                match executor.execute_statement_recursive(stmt).await {
                    Ok(code) => (code, ControlFlow::Normal),
                    Err(ExecutionError::Break) => (0, ControlFlow::Break),
                    Err(ExecutionError::Continue) => (0, ControlFlow::Continue),
                    Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                    Err(ExecutionError::Errexit(code)) => (code, ControlFlow::Exit),
                    Err(e) => {
                        eprintln!("execution error: {e}");
                        let exit_code = match &e {
                            ExecutionError::CommandNotFound(_) => 127,
                            ExecutionError::PermissionDenied(_) => 126,
                            _ => 1,
                        };
                        if !errexit_exempt && executor.context().shell_options().errexit {
                            (exit_code, ControlFlow::Exit)
                        } else {
                            (exit_code, ControlFlow::Normal)
                        }
                    }
                }
            }
            Statement::BraceGroup(brace_group) => {
                use colony_exec::builtin_redirect::BuiltinRedirectGuard;
                let redirect_guard = match BuiltinRedirectGuard::apply(
                    &brace_group.redirects,
                    executor.context_mut(),
                ) {
                    Ok(guard) => guard,
                    Err(e) => {
                        eprintln!("colony: redirect error: {e}");
                        return (1, ControlFlow::Normal);
                    }
                };
                let mut last_exit = 0;
                for stmt in &brace_group.body {
                    let (exit_code, flow) = execute_statement_with_control(stmt, executor).await;
                    last_exit = exit_code;
                    if flow != ControlFlow::Normal {
                        drop(redirect_guard);
                        return (last_exit, flow);
                    }
                }
                (last_exit, ControlFlow::Normal)
            }
            Statement::Subshell(subshell) => {
                use colony_exec::builtin_redirect::BuiltinRedirectGuard;
                let mut subshell_executor = Executor::new(executor.context().clone());
                subshell_executor.context_mut().clear_non_ignored_traps();
                let redirect_guard = match BuiltinRedirectGuard::apply(
                    &subshell.redirects,
                    subshell_executor.context_mut(),
                ) {
                    Ok(guard) => guard,
                    Err(e) => {
                        eprintln!("colony: redirect error: {e}");
                        return (1, ControlFlow::Normal);
                    }
                };
                let mut last_exit = 0;
                for stmt in &subshell.body {
                    let (exit_code, flow) =
                        execute_statement_with_control(stmt, &mut subshell_executor).await;
                    last_exit = exit_code;
                    match flow {
                        ControlFlow::Normal => {}
                        ControlFlow::Return | ControlFlow::Break | ControlFlow::Continue => {
                            drop(redirect_guard);
                            return (last_exit, flow);
                        }
                        ControlFlow::Exit => {
                            drop(redirect_guard);
                            return (last_exit, ControlFlow::Exit);
                        }
                    }
                }
                subshell_executor.run_exit_trap().await;
                (last_exit, ControlFlow::Normal)
            }
            Statement::FunctionDef(func_def) => {
                executor
                    .context_mut()
                    .define_function(func_def.name.clone(), func_def.clone());
                (0, ControlFlow::Normal)
            }
            Statement::Return(_ret_stmt) => (0, ControlFlow::Return),
            Statement::ProcDef(proc_def) => {
                executor
                    .context_mut()
                    .define_proc(proc_def.name.clone(), proc_def.clone());
                (0, ControlFlow::Normal)
            }
            Statement::VarDecl(_) => {
                eprintln!("This declaration not yet supported");
                (1, ControlFlow::Normal)
            }
            Statement::Conditional(cond_cmd) => {
                let result = evaluate_conditional(&cond_cmd.expr, executor);
                (i32::from(!result), ControlFlow::Normal)
            }
            Statement::Arithmetic(arith_cmd) => {
                let result = evaluate_arithmetic_command(&arith_cmd.expr, executor);
                (if result != 0 { 0 } else { 1 }, ControlFlow::Normal)
            }
            Statement::Coproc(coproc) => match execute_coproc(coproc, executor).await {
                Ok(exit_code) => (exit_code, ControlFlow::Normal),
                Err(e) => {
                    eprintln!("colony: coproc error: {e}");
                    (1, ControlFlow::Normal)
                }
            },
            Statement::AgentInvocation(agent_invoke) => {
                let result = executor.execute_agent_invocation(agent_invoke).await;
                match result {
                    Ok(status) => (status.code(), ControlFlow::Normal),
                    Err(e) => {
                        eprintln!("colony: agent error: {e}");
                        (1, ControlFlow::Normal)
                    }
                }
            }
            Statement::Empty => (executor.context().last_exit_status(), ControlFlow::Normal),
        }
    })
}

pub(crate) fn execute_statement_for_code<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + 'a>> {
    Box::pin(async move {
        let (code, _) = execute_statement_with_control(stmt, executor).await;
        executor.context_mut().set_last_exit_status(code);
        code
    })
}

#[cfg(test)]
mod tests {
    use super::ControlFlow;

    #[test]
    fn control_flow_default_is_normal() {
        // ControlFlow doesn't impl Default, but Normal is the zero state
        let flow = ControlFlow::Normal;
        assert_eq!(flow, ControlFlow::Normal);
        assert_ne!(flow, ControlFlow::Break);
        assert_ne!(flow, ControlFlow::Continue);
        assert_ne!(flow, ControlFlow::Exit);
        assert_ne!(flow, ControlFlow::Return);
    }

    #[test]
    fn control_flow_is_copy() {
        // ControlFlow must be Copy for efficient passing through loop/function boundaries
        let flow = ControlFlow::Break;
        let copied = flow;
        assert_eq!(flow, copied);
    }

    // Behavioral tests for break/continue/return live in colony-exec::tests
    // (for_break_exits_after_first_iteration, for_continue_skips_loop_body, etc.)
    // because they require the full Executor + parser integration.
}
