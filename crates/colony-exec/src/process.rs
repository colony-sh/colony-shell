//! Low-level process execution using nix

use crate::redirect::apply_redirects;
use crate::{ExecutionError, Result};
use colony_expand::ExpansionContext;
use colony_parser::Redirect;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{execve, fork, ForkResult, Pid};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Exit status of a process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus {
    /// Exit code (0-255)
    code: i32,
}

impl ExitStatus {
    pub fn new(code: i32) -> Self {
        Self { code }
    }

    pub fn success() -> Self {
        Self { code: 0 }
    }

    pub fn code(&self) -> i32 {
        self.code
    }

    pub fn is_success(&self) -> bool {
        self.code == 0
    }
}

/// Handle to a running process
#[derive(Debug)]
pub struct ProcessHandle {
    pid: Pid,
}

impl ProcessHandle {
    fn new(pid: Pid) -> Self {
        Self { pid }
    }

    /// Wait for the process to complete and return exit status
    pub fn wait(self) -> Result<ExitStatus> {
        match waitpid(self.pid, None) {
            Ok(WaitStatus::Exited(_pid, code)) => Ok(ExitStatus::new(code)),
            Ok(WaitStatus::Signaled(_pid, signal, _core_dumped)) => {
                // Process was killed by signal
                // Convention: exit code = 128 + signal number
                Ok(ExitStatus::new(128 + signal as i32))
            }
            Ok(_status) => {
                // Other wait statuses (stopped, continued, etc.)
                // For now, treat as error
                Err(ExecutionError::WaitError(nix::Error::EINVAL))
            }
            Err(e) => Err(ExecutionError::WaitError(e)),
        }
    }
}

/// Spawn a process by forking and executing a command
///
/// This is the low-level primitive that directly uses fork/exec.
/// It runs synchronously - the caller is responsible for using spawn_blocking
/// if async execution is needed.
///
/// Redirects are applied in the child process before exec.
pub fn spawn_process(
    program: &Path,
    args: &[String],
    env: &[(String, String)],
    redirects: &[Redirect],
    context: &mut ExpansionContext,
) -> Result<ProcessHandle> {
    // Convert arguments to CStrings
    let c_program = path_to_cstring(program)?;
    let c_args: Vec<CString> = args
        .iter()
        .map(|s| string_to_cstring(s))
        .collect::<Result<_>>()?;

    let c_env: Vec<CString> = env
        .iter()
        .map(|(k, v)| string_to_cstring(&format!("{}={}", k, v)))
        .collect::<Result<_>>()?;

    // Fork
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            // Parent process - return handle to child
            Ok(ProcessHandle::new(child))
        }
        Ok(ForkResult::Child) => {
            // Child process - apply redirects then exec

            // Apply all redirects BEFORE execve()
            if let Err(e) = apply_redirects(redirects, context) {
                eprintln!("colony-exec: redirect failed: {}", e);
                std::process::exit(126); // Convention: 126 = command found but not executable
            }

            // This replaces the current process image
            let exec_result = execve(&c_program, &c_args, &c_env);

            // If we get here, exec failed
            eprintln!(
                "colony-exec: failed to exec {}: {:?}",
                program.display(),
                exec_result
            );
            std::process::exit(127); // Convention: 127 = command not executable
        }
        Err(e) => Err(ExecutionError::ForkError(e)),
    }
}

/// Convert a Path to a CString
fn path_to_cstring(path: &Path) -> Result<CString> {
    let bytes = path.as_os_str().as_bytes();
    CString::new(bytes).map_err(|_| ExecutionError::InvalidUtf8)
}

/// Convert a String to a CString
fn string_to_cstring(s: &str) -> Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| ExecutionError::InvalidUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_spawn_echo() {
        let program = PathBuf::from("/bin/echo");
        let args = vec!["echo".to_string(), "hello".to_string()];
        let env = vec![];
        let redirects = vec![];
        let mut context = ExpansionContext::new();

        let handle = spawn_process(&program, &args, &env, &redirects, &mut context).unwrap();
        let status = handle.wait().unwrap();

        assert!(status.is_success());
    }

    #[test]
    fn test_spawn_false() {
        let program = PathBuf::from("/usr/bin/false");
        let args = vec!["false".to_string()];
        let env = vec![];
        let redirects = vec![];
        let mut context = ExpansionContext::new();

        let handle = spawn_process(&program, &args, &env, &redirects, &mut context).unwrap();
        let status = handle.wait().unwrap();

        assert!(!status.is_success());
        assert_eq!(status.code(), 1);
    }

    #[test]
    fn test_spawn_true() {
        let program = PathBuf::from("/usr/bin/true");
        let args = vec!["true".to_string()];
        let env = vec![];
        let redirects = vec![];
        let mut context = ExpansionContext::new();

        let handle = spawn_process(&program, &args, &env, &redirects, &mut context).unwrap();
        let status = handle.wait().unwrap();

        assert!(status.is_success());
        assert_eq!(status.code(), 0);
    }
}
