//! Job control - background jobs, process groups, and job management
//!
//! Implements POSIX job control:
//! - Job table tracking background and stopped jobs
//! - Process groups for job isolation
//! - Signal handling (SIGCHLD, SIGINT, SIGTSTP, SIGCONT)
//! - Builtins: fg, bg, jobs, wait

use crate::{ExecutionError, ExitStatus, Result};
use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
use nix::unistd::{setpgid, tcsetpgrp, Pid};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Job identifier (1-based, displayed to user)
pub type JobId = usize;

/// Job status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    /// Running in foreground
    Running,
    /// Running in background
    Background,
    /// Stopped (Ctrl-Z)
    Stopped,
    /// Completed
    Done(ExitStatus),
}

/// A job represents a pipeline or command that can be managed
#[derive(Debug, Clone)]
pub struct Job {
    /// Job ID (1, 2, 3, ...)
    pub id: JobId,
    /// Process group ID (same as leader PID)
    pub pgid: Pid,
    /// PIDs of all processes in the job
    pub pids: Vec<Pid>,
    /// Command string (for display in `jobs`)
    pub command: String,
    /// Current status
    pub status: JobStatus,
}

impl Job {
    pub fn new(id: JobId, pgid: Pid, pids: Vec<Pid>, command: String) -> Self {
        Self {
            id,
            pgid,
            pids,
            command,
            status: JobStatus::Running,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, JobStatus::Running | JobStatus::Background)
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self.status, JobStatus::Stopped)
    }

    pub fn is_done(&self) -> bool {
        matches!(self.status, JobStatus::Done(_))
    }
}

/// Job table - tracks all jobs in the shell
///
/// Uses parking_lot::RwLock for concurrent access (Rule #7)
pub struct JobTable {
    jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    next_id: Arc<RwLock<JobId>>,
}

impl JobTable {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }

    /// Add a new job
    pub fn add_job(&self, pgid: Pid, pids: Vec<Pid>, command: String) -> JobId {
        let mut next_id = self.next_id.write();
        let id = *next_id;
        *next_id += 1;

        let job = Job::new(id, pgid, pids, command);
        self.jobs.write().insert(id, job);

        id
    }

    /// Get a job by ID
    pub fn get_job(&self, id: JobId) -> Option<Job> {
        self.jobs.read().get(&id).cloned()
    }

    /// Remove a job
    pub fn remove_job(&self, id: JobId) -> Option<Job> {
        self.jobs.write().remove(&id)
    }

    /// Update job status
    pub fn update_status(&self, id: JobId, status: JobStatus) {
        if let Some(job) = self.jobs.write().get_mut(&id) {
            job.status = status;
        }
    }

    /// List all jobs
    pub fn list_jobs(&self) -> Vec<Job> {
        self.jobs.read().values().cloned().collect()
    }

    /// Find job by process group ID
    pub fn find_by_pgid(&self, pgid: Pid) -> Option<Job> {
        self.jobs.read().values().find(|j| j.pgid == pgid).cloned()
    }

    /// Find job by PID (searches all processes in all jobs)
    pub fn find_by_pid(&self, pid: Pid) -> Option<Job> {
        self.jobs
            .read()
            .values()
            .find(|j| j.pids.contains(&pid))
            .cloned()
    }

    /// Remove all done jobs
    pub fn remove_done_jobs(&self) {
        self.jobs.write().retain(|_, job| !job.is_done());
    }

    /// Find the most recent stopped job (highest job ID with Stopped status)
    ///
    /// Used by `fg` and `bg` when no job spec is given. Returns the stopped
    /// job with the highest ID, which corresponds to the most recently added.
    pub fn most_recent_stopped_job(&self) -> Option<Job> {
        self.jobs
            .read()
            .values()
            .filter(|j| j.is_stopped())
            .max_by_key(|j| j.id)
            .cloned()
    }

    /// Find the most recent background or stopped job (highest job ID)
    ///
    /// Fallback for `fg` when there are no stopped jobs but there are
    /// background jobs that can be brought to the foreground.
    pub fn most_recent_active_job(&self) -> Option<Job> {
        self.jobs
            .read()
            .values()
            .filter(|j| matches!(j.status, JobStatus::Stopped | JobStatus::Background))
            .max_by_key(|j| j.id)
            .cloned()
    }
}

impl Default for JobTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize job control
///
/// This should be called once at shell startup.
/// Sets up signal handlers and makes the shell a session leader.
pub fn init_job_control() -> Result<()> {
    // Ignore job control signals in the shell itself
    // Children will inherit these and we'll change them as needed

    // SAFETY: sigaction is safe to call with SigIgn handler and empty signal
    // set. We ignore these signals in the shell so that child processes in
    // the foreground process group receive them instead.
    unsafe {
        let ignore = SigAction::new(SigHandler::SigIgn, SaFlags::empty(), SigSet::empty());

        // Ignore SIGINT (Ctrl-C) - let foreground job handle it
        sigaction(Signal::SIGINT, &ignore).map_err(|e| {
            ExecutionError::IoError(std::io::Error::other(format!("sigaction SIGINT: {}", e)))
        })?;

        // Ignore SIGTSTP (Ctrl-Z) - let foreground job handle it
        sigaction(Signal::SIGTSTP, &ignore).map_err(|e| {
            ExecutionError::IoError(std::io::Error::other(format!("sigaction SIGTSTP: {}", e)))
        })?;

        // Ignore SIGTTOU (background write to terminal)
        sigaction(Signal::SIGTTOU, &ignore).map_err(|e| {
            ExecutionError::IoError(std::io::Error::other(format!("sigaction SIGTTOU: {}", e)))
        })?;
    }

    Ok(())
}

/// Create a new process group for a job
///
/// Should be called in both parent and child to avoid race conditions.
/// The first call (parent or child) will succeed, the second will be a no-op.
#[allow(dead_code)]
pub fn create_process_group(pid: Pid, pgid: Pid) -> Result<()> {
    setpgid(pid, pgid)
        .map_err(|e| ExecutionError::IoError(std::io::Error::other(format!("setpgid: {}", e))))?;
    Ok(())
}

/// Give terminal control to a process group
///
/// Makes the specified process group the foreground process group.
pub fn give_terminal_to(pgid: Pid) -> Result<()> {
    use std::os::fd::{AsRawFd, BorrowedFd};
    let stdin_fd = std::io::stdin().as_raw_fd();
    // SAFETY: stdin (fd 0) is always a valid open file descriptor in a shell process.
    let stdin_borrowed = unsafe { BorrowedFd::borrow_raw(stdin_fd) };

    tcsetpgrp(stdin_borrowed, pgid)
        .map_err(|e| ExecutionError::IoError(std::io::Error::other(format!("tcsetpgrp: {}", e))))?;
    Ok(())
}

/// Take back terminal control from a job
///
/// Returns terminal control to the shell's process group.
pub fn take_terminal() -> Result<Pid> {
    use std::os::fd::{AsRawFd, BorrowedFd};
    let stdin_fd = std::io::stdin().as_raw_fd();
    // SAFETY: stdin (fd 0) is always a valid open file descriptor in a shell process.
    let stdin_borrowed = unsafe { BorrowedFd::borrow_raw(stdin_fd) };

    // Get shell's own pgid
    let shell_pgid = nix::unistd::getpgrp();

    tcsetpgrp(stdin_borrowed, shell_pgid)
        .map_err(|e| ExecutionError::IoError(std::io::Error::other(format!("tcsetpgrp: {}", e))))?;

    Ok(shell_pgid)
}

/// Send SIGCONT to a job's process group
///
/// Resumes a stopped process group. Used by both `fg` and `bg` builtins.
/// Returns an error if the process group no longer exists.
pub fn continue_job(pgid: Pid) -> Result<()> {
    nix::sys::signal::killpg(pgid, Signal::SIGCONT).map_err(|e| {
        ExecutionError::IoError(std::io::Error::other(format!(
            "killpg({}, SIGCONT): {}",
            pgid, e
        )))
    })?;
    Ok(())
}

/// Wait for a foreground job to complete or stop
///
/// Blocks until the job either exits or is stopped (e.g., Ctrl-Z).
/// Returns the resulting `JobStatus`: either `Done(exit_status)` or `Stopped`.
pub fn wait_for_foreground_job(job: &Job) -> JobStatus {
    use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};

    let mut last_status = ExitStatus::new(0);

    // Wait for each process in the job. We use WUNTRACED so that
    // waitpid returns when a child is stopped (SIGTSTP), not only
    // when it exits.
    for &pid in &job.pids {
        loop {
            match waitpid(pid, Some(WaitPidFlag::WUNTRACED)) {
                Ok(WaitStatus::Exited(_pid, code)) => {
                    last_status = ExitStatus::new(code);
                    break;
                }
                Ok(WaitStatus::Signaled(_pid, signal, _core_dumped)) => {
                    last_status = ExitStatus::new(128 + signal as i32);
                    break;
                }
                Ok(WaitStatus::Stopped(_pid, _signal)) => {
                    // Job was stopped again (Ctrl-Z during fg)
                    return JobStatus::Stopped;
                }
                Ok(WaitStatus::Continued(_pid)) => {
                    // Process was continued; keep waiting
                    continue;
                }
                Err(nix::errno::Errno::ECHILD) => {
                    // Process already reaped or not our child
                    break;
                }
                Err(nix::errno::Errno::EINTR) => {
                    // Interrupted by signal; retry
                    continue;
                }
                Err(_) => {
                    break;
                }
                // StillAlive cannot occur without WNOHANG
                _ => break,
            }
        }
    }

    JobStatus::Done(last_status)
}

/// Reap completed background children (non-blocking)
///
/// Should be called periodically (e.g., before each prompt) to collect
/// exit statuses from background jobs that have terminated, preventing
/// zombie processes. Updates job statuses in the table.
pub fn reap_children(jobs: &JobTable) {
    use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};

    // Non-blocking reap loop: collect all available child statuses
    loop {
        match waitpid(
            Pid::from_raw(-1),
            Some(WaitPidFlag::WNOHANG | WaitPidFlag::WUNTRACED),
        ) {
            Ok(WaitStatus::Exited(pid, code)) => {
                if let Some(job) = jobs.find_by_pid(pid) {
                    jobs.update_status(job.id, JobStatus::Done(ExitStatus::new(code)));
                }
            }
            Ok(WaitStatus::Signaled(pid, signal, _core_dumped)) => {
                if let Some(job) = jobs.find_by_pid(pid) {
                    jobs.update_status(
                        job.id,
                        JobStatus::Done(ExitStatus::new(128 + signal as i32)),
                    );
                }
            }
            Ok(WaitStatus::Stopped(pid, _signal)) => {
                if let Some(job) = jobs.find_by_pid(pid) {
                    jobs.update_status(job.id, JobStatus::Stopped);
                }
            }
            Ok(WaitStatus::StillAlive) => {
                // No more children with status changes
                break;
            }
            Ok(_) => {
                // Continued or other status; keep reaping
                continue;
            }
            Err(nix::errno::Errno::ECHILD) => {
                // No children at all
                break;
            }
            Err(_) => {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_table() {
        let table = JobTable::new();

        let pgid = Pid::from_raw(1234);
        let pids = vec![Pid::from_raw(1234), Pid::from_raw(1235)];
        let job_id = table.add_job(pgid, pids.clone(), "echo hello | grep world".to_string());

        assert_eq!(job_id, 1);

        let job = table.get_job(job_id).unwrap();
        assert_eq!(job.id, 1);
        assert_eq!(job.pgid, pgid);
        assert_eq!(job.pids, pids);
        assert!(job.is_running());
    }

    #[test]
    fn test_job_status_transitions() {
        let table = JobTable::new();

        let pgid = Pid::from_raw(1234);
        let pids = vec![Pid::from_raw(1234)];
        let job_id = table.add_job(pgid, pids, "sleep 10".to_string());

        // Initially running
        let job = table.get_job(job_id).unwrap();
        assert_eq!(job.status, JobStatus::Running);

        // Stop it
        table.update_status(job_id, JobStatus::Stopped);
        let job = table.get_job(job_id).unwrap();
        assert!(job.is_stopped());

        // Resume in background
        table.update_status(job_id, JobStatus::Background);
        let job = table.get_job(job_id).unwrap();
        assert!(job.is_running());

        // Complete it
        table.update_status(job_id, JobStatus::Done(ExitStatus::new(0)));
        let job = table.get_job(job_id).unwrap();
        assert!(job.is_done());
    }

    #[test]
    fn test_job_table_removal() {
        let table = JobTable::new();

        let pgid = Pid::from_raw(1234);
        let pids = vec![Pid::from_raw(1234)];
        let job_id = table.add_job(pgid, pids, "test".to_string());

        assert!(table.get_job(job_id).is_some());

        table.remove_job(job_id);
        assert!(table.get_job(job_id).is_none());
    }

    #[test]
    fn test_find_by_pgid() {
        let table = JobTable::new();

        let pgid = Pid::from_raw(5678);
        let pids = vec![Pid::from_raw(5678), Pid::from_raw(5679)];
        let _job_id = table.add_job(pgid, pids, "test".to_string());

        let found = table.find_by_pgid(pgid);
        assert!(found.is_some());
        assert_eq!(found.unwrap().pgid, pgid);
    }

    #[test]
    fn test_find_by_pid() {
        let table = JobTable::new();

        let pgid = Pid::from_raw(9999);
        let pids = vec![
            Pid::from_raw(9999),
            Pid::from_raw(10000),
            Pid::from_raw(10001),
        ];
        let _job_id = table.add_job(pgid, pids.clone(), "test".to_string());

        // Should find job by any of its PIDs
        for pid in pids {
            let found = table.find_by_pid(pid);
            assert!(found.is_some());
            assert_eq!(found.unwrap().pgid, pgid);
        }
    }

    #[test]
    fn test_most_recent_stopped_job() {
        let table = JobTable::new();

        // Add three jobs: first two running, third stopped
        let id1 = table.add_job(
            Pid::from_raw(1001),
            vec![Pid::from_raw(1001)],
            "job1".to_string(),
        );
        let _id2 = table.add_job(
            Pid::from_raw(1002),
            vec![Pid::from_raw(1002)],
            "job2".to_string(),
        );
        let id3 = table.add_job(
            Pid::from_raw(1003),
            vec![Pid::from_raw(1003)],
            "job3".to_string(),
        );

        // No stopped jobs yet
        assert!(table.most_recent_stopped_job().is_none());

        // Stop job 1 and job 3
        table.update_status(id1, JobStatus::Stopped);
        table.update_status(id3, JobStatus::Stopped);

        // Most recent stopped should be job 3 (highest ID)
        let stopped = table.most_recent_stopped_job().unwrap();
        assert_eq!(stopped.id, id3);
    }

    #[test]
    fn test_most_recent_active_job() {
        let table = JobTable::new();

        // Add a completed job and a background job
        let id1 = table.add_job(
            Pid::from_raw(2001),
            vec![Pid::from_raw(2001)],
            "done_job".to_string(),
        );
        let id2 = table.add_job(
            Pid::from_raw(2002),
            vec![Pid::from_raw(2002)],
            "bg_job".to_string(),
        );
        let id3 = table.add_job(
            Pid::from_raw(2003),
            vec![Pid::from_raw(2003)],
            "stopped_job".to_string(),
        );

        table.update_status(id1, JobStatus::Done(ExitStatus::new(0)));
        table.update_status(id2, JobStatus::Background);
        table.update_status(id3, JobStatus::Stopped);

        // Most recent active should be job 3 (stopped, highest ID among active)
        let active = table.most_recent_active_job().unwrap();
        assert_eq!(active.id, id3);
    }

    #[test]
    fn test_most_recent_active_job_only_background() {
        let table = JobTable::new();

        let id1 = table.add_job(
            Pid::from_raw(3001),
            vec![Pid::from_raw(3001)],
            "bg_job".to_string(),
        );
        table.update_status(id1, JobStatus::Background);

        // Should find the background job when no stopped jobs exist
        let active = table.most_recent_active_job().unwrap();
        assert_eq!(active.id, id1);
        assert_eq!(active.status, JobStatus::Background);
    }

    #[test]
    fn test_most_recent_stopped_empty_table() {
        let table = JobTable::new();
        assert!(table.most_recent_stopped_job().is_none());
        assert!(table.most_recent_active_job().is_none());
    }
}
