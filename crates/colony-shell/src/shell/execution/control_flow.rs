use colony_exec::Executor;
use colony_expand::{Expander, TrapAction, TrapSignal};
use colony_parser::{AliasContext, Statement};
use tracing::info;

pub(crate) const MAX_JOBS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ControlFlow {
    Normal,
    Break,
    Continue,
    Return,
    Exit,
}

pub(crate) async fn execute_list_with_control(list: &colony_parser::CommandList, executor: &mut Executor) -> (i32, ControlFlow) {
    use colony_parser::ListOp;

    let first_is_background = list
        .rest
        .first()
        .map(|(op, _)| *op == ListOp::Background)
        .unwrap_or(false);
    let first_is_errexit_exempt = list
        .rest
        .first()
        .map(|(op, _)| matches!(op, ListOp::And | ListOp::Or))
        .unwrap_or(false);

    let (mut last_exit_code, mut control) = if first_is_background {
        match super::background::run_in_background(&list.first, executor).await {
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

    let mut i = 0;
    while i < list.rest.len() {
        let (op, stmt) = &list.rest[i];
        let next_is_background = list
            .rest
            .get(i + 1)
            .map(|(next_op, _)| *next_op == ListOp::Background)
            .unwrap_or(false);
        let next_is_errexit_exempt = list
            .rest
            .get(i + 1)
            .map(|(next_op, _)| matches!(next_op, ListOp::And | ListOp::Or))
            .unwrap_or(false);
        let this_is_background = *op == ListOp::Background;

        let should_execute = match op {
            ListOp::Semi => true,
            ListOp::And => last_exit_code == 0,
            ListOp::Or => last_exit_code != 0,
            ListOp::Background => true,
        };

        if should_execute {
            if next_is_background || (this_is_background && i == list.rest.len() - 1) {
                if !this_is_background {
                    match super::background::run_in_background(stmt, executor).await {
                        Ok((job_id, pid)) => {
                            if executor.context().is_interactive() {
                                eprintln!("[{}] {}", job_id, pid);
                            }
                            last_exit_code = 0;
                        }
                        Err(err) => {
                            eprintln!("background start failed: {}", err);
                            last_exit_code = 1;
                        }
                    }
                }
            } else if !this_is_background {
                let (code, flow) = execute_statement_with_errexit_control(stmt, executor, next_is_errexit_exempt).await;
                last_exit_code = code;
                executor.context_mut().set_last_exit_status(last_exit_code);
                control = flow;
                if control != ControlFlow::Normal {
                    return (last_exit_code, control);
                }
            }
        }

        i += 1;
    }

    (last_exit_code, control)
}

pub(crate) async fn execute_statement_with_control(stmt: &Statement, executor: &mut Executor) -> (i32, ControlFlow) {
    execute_statement_with_errexit_control(stmt, executor, false).await
}

pub(crate) async fn execute_statement_with_errexit_control(
    stmt: &Statement,
    executor: &mut Executor,
    is_exempt: bool,
) -> (i32, ControlFlow) {
    let saved_errexit = executor.context().shell_options().errexit;
    if is_exempt {
        executor.context_mut().shell_options_mut().errexit = false;
    }

    let (code, flow) = match stmt {
        Statement::SimpleCommand(cmd) => super::super::engine::execute_simple_command(cmd, executor).await,
        Statement::Pipeline(pipe) => super::super::engine::execute_pipeline(pipe, executor).await,
        Statement::CommandList(list) => execute_list_with_control(list, executor).await,
        Statement::For(for_stmt) => execute_for_loop_with_control(for_stmt, executor).await,
        Statement::While(while_stmt) => execute_while_loop_with_control(while_stmt, executor).await,
        Statement::Until(until_stmt) => execute_until_loop_with_control(until_stmt, executor).await,
        Statement::If(if_stmt) => execute_if_with_control(if_stmt, executor).await,
        Statement::FunctionDef(func_def) => {
            executor
                .context_mut()
                .define_function(func_def.name.clone(), func_def.clone());
            (0, ControlFlow::Normal)
        }
        Statement::Time(time_stmt) => super::super::engine::execute_time_statement(time_stmt, executor).await,
        Statement::Subshell(subshell) => super::super::engine::execute_subshell(subshell, executor).await,
        Statement::Assignment(assign) => {
            if let Err(e) = super::super::engine::execute_assignment(assign, executor).await {
                eprintln!("assignment error: {}", e);
                (1, ControlFlow::Normal)
            } else {
                (0, ControlFlow::Normal)
            }
        }
        Statement::Coproc(coproc) => match super::super::engine::execute_coproc(coproc, executor).await {
            Ok(code) => (code, ControlFlow::Normal),
            Err(e) => {
                eprintln!("coproc error: {}", e);
                (1, ControlFlow::Normal)
            }
        },
    };

    if is_exempt {
        executor.context_mut().shell_options_mut().errexit = saved_errexit;
    }

    if executor.context().shell_options().errexit && code != 0 && !is_exempt {
        (code, ControlFlow::Exit)
    } else {
        (code, flow)
    }
}

async fn execute_if_with_control(if_stmt: &colony_parser::IfStatement, executor: &mut Executor) -> (i32, ControlFlow) {
    use colony_parser::Condition;

    let saved_errexit = executor.context().shell_options().errexit;
    executor.context_mut().shell_options_mut().errexit = false;
    let (condition_exit, _) = match &if_stmt.condition {
        Condition::Command(cmd) => execute_statement_with_errexit_control(cmd, executor, true).await,
        Condition::Expression(_expr) => {
            eprintln!("Expression conditions not yet supported");
            (1, ControlFlow::Normal)
        }
    };
    executor.context_mut().shell_options_mut().errexit = saved_errexit;

    let condition_result = condition_exit == 0;

    if condition_result {
        let mut last_exit = 0;
        for stmt in &if_stmt.then_body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;
            if flow != ControlFlow::Normal {
                return (last_exit, flow);
            }
        }
        (last_exit, ControlFlow::Normal)
    } else {
        for elif in &if_stmt.elif_branches {
            let saved_errexit = executor.context().shell_options().errexit;
            executor.context_mut().shell_options_mut().errexit = false;
            let (elif_exit, _) = match &elif.condition {
                Condition::Command(cmd) => execute_statement_with_errexit_control(cmd, executor, true).await,
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
            (last_exit, ControlFlow::Normal)
        } else {
            (0, ControlFlow::Normal)
        }
    }
}

async fn execute_for_loop_with_control(for_stmt: &colony_parser::ForStatement, executor: &mut Executor) -> (i32, ControlFlow) {
    let mut expander = Expander::with_executor(executor.context().clone(), |stmt, ctx| {
        colony_exec::execute_and_capture(stmt, ctx)
    });
    let mut values = Vec::new();

    for word in &for_stmt.words {
        match expander.expand_word(word) {
            Ok(fields) => values.extend(fields),
            Err(e) => {
                eprintln!("expansion error: {}", e);
                return (1, ControlFlow::Normal);
            }
        }
    }

    let mut last_exit = 0;
    for field in values {
        executor.context_mut().set_var(for_stmt.var.clone(), field.text);
        for stmt in &for_stmt.body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;
            match flow {
                ControlFlow::Break => return (last_exit, ControlFlow::Normal),
                ControlFlow::Continue => break,
                ControlFlow::Return | ControlFlow::Exit => return (last_exit, flow),
                ControlFlow::Normal => {}
            }
        }
    }

    (last_exit, ControlFlow::Normal)
}

async fn execute_while_loop_with_control(while_stmt: &colony_parser::WhileStatement, executor: &mut Executor) -> (i32, ControlFlow) {
    loop {
        let (cond_exit, _) = match &while_stmt.condition {
            colony_parser::Condition::Command(cmd) => execute_statement_with_errexit_control(cmd, executor, true).await,
            colony_parser::Condition::Expression(_) => {
                eprintln!("Expression conditions not yet supported");
                (1, ControlFlow::Normal)
            }
        };

        if cond_exit != 0 {
            return (0, ControlFlow::Normal);
        }

        for stmt in &while_stmt.body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            if flow == ControlFlow::Break {
                return (code, ControlFlow::Normal);
            }
            if flow == ControlFlow::Continue {
                break;
            }
            if flow == ControlFlow::Return || flow == ControlFlow::Exit {
                return (code, flow);
            }
        }
    }
}

async fn execute_until_loop_with_control(until_stmt: &colony_parser::WhileStatement, executor: &mut Executor) -> (i32, ControlFlow) {
    loop {
        let (cond_exit, _) = match &until_stmt.condition {
            colony_parser::Condition::Command(cmd) => execute_statement_with_errexit_control(cmd, executor, true).await,
            colony_parser::Condition::Expression(_) => {
                eprintln!("Expression conditions not yet supported");
                (1, ControlFlow::Normal)
            }
        };

        if cond_exit == 0 {
            return (0, ControlFlow::Normal);
        }

        for stmt in &until_stmt.body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            if flow == ControlFlow::Break {
                return (code, ControlFlow::Normal);
            }
            if flow == ControlFlow::Continue {
                break;
            }
            if flow == ControlFlow::Return || flow == ControlFlow::Exit {
                return (code, flow);
            }
        }
    }
}

pub(crate) async fn execute_statement_with_traps(stmt: &Statement, executor: &mut Executor) -> Result<(i32, ControlFlow)> {
    let (code, flow) = execute_statement_with_control(stmt, executor).await;
    super::traps::execute_exit_trap(executor).await?;
    Ok((code, flow))
}
