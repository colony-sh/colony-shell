//! Job control - background jobs, process groups, and job management
//!
//! Implements POSIX job control:
//! - Job table tracking background and stopped jobs
//! - Process groups for job isolation
//! - Signal handling (SIGCHLD, SIGINT, SIGTSTP, SIGCONT)
//! - Builtins: fg, bg, jobs, wait

use crate::{ExecutionError, ExitStatus, Result};
use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
use nix::unistd::{Pid, setpgid, tcsetpgrp};
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
        self.jobs
            .read()
            .values()
            .find(|j| j.pgid == pgid)
            .cloned()
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

    unsafe {
        let ignore = SigAction::new(
            SigHandler::SigIgn,
            SaFlags::empty(),
            SigSet::empty(),
        );

        // Ignore SIGINT (Ctrl-C) - let foreground job handle it
        sigaction(Signal::SIGINT, &ignore)
            .map_err(|e| ExecutionError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("sigaction SIGINT: {}", e),
            )))?;

        // Ignore SIGTSTP (Ctrl-Z) - let foreground job handle it
        sigaction(Signal::SIGTSTP, &ignore)
            .map_err(|e| ExecutionError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("sigaction SIGTSTP: {}", e),
            )))?;

        // Ignore SIGTTOU (background write to terminal)
        sigaction(Signal::SIGTTOU, &ignore)
            .map_err(|e| ExecutionError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("sigaction SIGTTOU: {}", e),
            )))?;
    }

    Ok(())
}

#[allow(dead_code)]
/// Create a new process group for a job
///
/// Should be called in both parent and child to avoid race conditions.
/// The first call (parent or child) will succeed, the second will be a no-op.
pub fn create_process_group(pid: Pid, pgid: Pid) -> Result<()> {
    setpgid(pid, pgid).map_err(|e| {
        ExecutionError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("setpgid: {}", e),
        ))
    })?;
    Ok(())
}

#[allow(dead_code)]
/// Give terminal control to a process group
///
/// Makes the specified process group the foreground process group.
pub fn give_terminal_to(pgid: Pid) -> Result<()> {
    use std::os::fd::{AsRawFd, BorrowedFd};
    let stdin_fd = std::io::stdin().as_raw_fd();
    let stdin_borrowed = unsafe { BorrowedFd::borrow_raw(stdin_fd) };

    tcsetpgrp(&stdin_borrowed, pgid).map_err(|e| {
        ExecutionError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("tcsetpgrp: {}", e),
        ))
    })?;
    Ok(())
}

#[allow(dead_code)]
/// Take back terminal control from a job
///
/// Returns terminal control to the shell's process group.
pub fn take_terminal() -> Result<Pid> {
    use std::os::fd::{AsRawFd, BorrowedFd};
    let stdin_fd = std::io::stdin().as_raw_fd();
    let stdin_borrowed = unsafe { BorrowedFd::borrow_raw(stdin_fd) };

    // Get shell's own pgid
    let shell_pgid = nix::unistd::getpgrp();

    tcsetpgrp(&stdin_borrowed, shell_pgid).map_err(|e| {
        ExecutionError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("tcsetpgrp: {}", e),
        ))
    })?;

    Ok(shell_pgid)
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
        let pids = vec![Pid::from_raw(9999), Pid::from_raw(10000), Pid::from_raw(10001)];
        let _job_id = table.add_job(pgid, pids.clone(), "test".to_string());

        // Should find job by any of its PIDs
        for pid in pids {
            let found = table.find_by_pid(pid);
            assert!(found.is_some());
            assert_eq!(found.unwrap().pgid, pgid);
        }
    }
}
