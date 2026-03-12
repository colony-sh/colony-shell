mod engine;
pub(crate) mod execution;
mod orchestrator;
mod startup;

pub(crate) use engine::execute_shell_command;
#[allow(unused_imports)]
pub(crate) use execution::*;
pub(crate) use orchestrator::run;

/// Returns true if running inside colony-terminal (COLONY_TERMINAL env var set).
pub(crate) fn is_colony_terminal() -> bool {
    std::env::var("COLONY_TERMINAL").is_ok()
}
