use colony_exec::Executor;
use colony_expand::{TrapAction, TrapSignal};

use crate::shell::execution::ControlFlow;

pub(crate) type ExecWithControlFn = for<'a> fn(
    &'a colony_parser::Statement,
    &'a mut Executor,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = (i32, ControlFlow)> + 'a>,
>;

pub(crate) type ExecForCodeFn =
    for<'a> fn(
        &'a colony_parser::Statement,
        &'a mut Executor,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + 'a>>;

pub(crate) async fn execute_exit_trap(
    executor: &mut Executor,
    execute_with_control: ExecWithControlFn,
) -> Option<i32> {
    use colony_parser::pest_api::parse;

    let Some(TrapAction::Command(cmd)) = executor.context().get_trap(&TrapSignal::Exit).cloned()
    else {
        return None;
    };

    let Ok(program) = parse(&cmd) else {
        return None;
    };

    for stmt in &program.statements {
        let (code, flow) = execute_with_control(stmt, executor).await;
        if matches!(flow, ControlFlow::Exit) {
            return Some(code);
        }
    }

    None
}

pub(crate) async fn execute_err_trap(executor: &mut Executor, execute_for_code: ExecForCodeFn) {
    use colony_parser::pest_api::parse;

    let Some(TrapAction::Command(cmd)) = executor.context().get_trap(&TrapSignal::Err).cloned()
    else {
        return;
    };

    let Ok(program) = parse(&cmd) else {
        return;
    };

    for stmt in &program.statements {
        let _ = execute_for_code(stmt, executor).await;
    }
}
