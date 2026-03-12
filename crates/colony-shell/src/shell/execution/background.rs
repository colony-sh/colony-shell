use colony_exec::Executor;
use tracing::info;

use crate::shell::execution::statement_to_shell_string;

pub(crate) const MAX_JOBS: usize = 128;

pub(crate) async fn run_in_background(
    stmt: &colony_parser::Statement,
    executor: &mut Executor,
) -> std::result::Result<(usize, u32), String> {
    use colony_exec::JobStatus;
    use nix::sys::signal::{signal, SigHandler, Signal};
    use nix::unistd::{fork, setpgid, ForkResult, Pid};

    if executor.jobs().list_jobs().len() >= MAX_JOBS {
        return Err("too many background jobs".to_string());
    }

    let cmd_string = statement_to_shell_string(stmt);
    let mut child_context = executor.context().clone();

    // SAFETY: fork() is safe to call. The child process gets a copy of the
    // parent's address space. We handle both Parent and Child cases explicitly.
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            let pid = child.as_raw() as u32;
            executor.context().set_last_bg_pid(pid);

            let job_id = executor
                .jobs()
                .add_job(child, vec![child], cmd_string.clone());
            executor.jobs().update_status(job_id, JobStatus::Background);
            info!(job_id, pid, cmd = %cmd_string, "background job started");

            Ok((job_id, pid))
        }
        Ok(ForkResult::Child) => {
            // SAFETY: signal() with SigDfl is safe - we're resetting signals to
            // default handlers in the child process before exec.
            unsafe {
                let _ = signal(Signal::SIGINT, SigHandler::SigDfl);
                let _ = signal(Signal::SIGQUIT, SigHandler::SigDfl);
                let _ = signal(Signal::SIGTSTP, SigHandler::SigDfl);
            }

            if let Err(err) = setpgid(Pid::from_raw(0), Pid::from_raw(0)) {
                eprintln!("setpgid failed: {err}");
                // SAFETY: _exit() is async-signal-safe and appropriate for
                // terminating a child process after fork() failure.
                unsafe {
                    nix::libc::_exit(1);
                }
            }

            let exit_code =
                colony_exec::execute_statement_for_process_sub(stmt, &mut child_context);
            // SAFETY: _exit() is async-signal-safe and appropriate for
            // terminating a child process. We use _exit instead of exit to
            // avoid running atexit handlers in the child.
            unsafe {
                nix::libc::_exit(exit_code);
            }
        }
        Err(err) => Err(format!("fork failed: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_jobs_is_reasonable_power_of_two() {
        // MAX_JOBS should be a power of two for clean limits and > 0
        assert!(MAX_JOBS > 0);
        assert!(MAX_JOBS.is_power_of_two(), "MAX_JOBS should be a power of two");
        // Sanity: not too small (shells typically allow at least 32) and not absurdly large
        assert!(MAX_JOBS >= 32, "MAX_JOBS too small for real usage");
        assert!(MAX_JOBS <= 1024, "MAX_JOBS unreasonably large");
    }

    #[tokio::test]
    async fn rejects_when_job_table_full() {
        use colony_exec::Executor;
        use colony_expand::ExpansionContext;

        let mut executor = Executor::new(ExpansionContext::new());

        // Fill the job table to MAX_JOBS
        for _ in 0..MAX_JOBS {
            executor
                .jobs()
                .add_job(nix::unistd::Pid::from_raw(1), vec![], "dummy".to_string());
        }

        // Attempting to run in background should fail
        let stmt = colony_parser::pest_api::parse_statement("echo full")
            .expect("parse failed");
        let result = run_in_background(&stmt, &mut executor).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "too many background jobs");
    }
}
