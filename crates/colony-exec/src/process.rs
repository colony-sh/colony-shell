//! Low-level process execution using nix

use crate::redirect::{apply_redirects, apply_channel_redirects_in_child, prepare_channel_redirects, ChannelPipe};
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

/// Handle to a running process (F024 Phase 2: includes channel pipes)
#[derive(Debug)]
pub struct ProcessHandle {
    pid: Pid,
    /// Channel pipes for capturing writes to fd 3-6
    pub channel_pipes: Vec<ChannelPipe>,
}

impl ProcessHandle {
    #[allow(dead_code)]
    fn new(pid: Pid) -> Self {
        Self {
            pid,
            channel_pipes: Vec::new(),
        }
    }

    fn with_channels(pid: Pid, channel_pipes: Vec<ChannelPipe>) -> Self {
        Self { pid, channel_pipes }
    }

    /// Take channel pipes (F024 Phase 2)
    ///
    /// Removes and returns channel pipes, leaving an empty vector.
    /// Must be called before wait() if you need the pipes.
    pub fn take_channel_pipes(&mut self) -> Vec<ChannelPipe> {
        std::mem::take(&mut self.channel_pipes)
    }

    /// Wait for the process to complete and return exit status
    ///
    /// **F024 Phase 2:** Call take_channel_pipes() first if you need them.
    /// This consumes self to prevent use-after-wait bugs.
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
///
/// **F024 Phase 2:** Detects channel redirects (fd 3-6) and creates pipes.
/// Returns ProcessHandle with channel_pipes that parent should process after wait().
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

    // F024 Phase 2: Prepare channel redirects BEFORE fork
    let channel_pipes = prepare_channel_redirects(redirects)?;
    let has_channel_pipes = !channel_pipes.is_empty();

    // Fork
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            // Parent process - explicitly close write ends, keep read ends
            // CRITICAL: Must close write ends or reads will hang forever
            let parent_pipes = if has_channel_pipes {
                channel_pipes.into_iter().map(|pipe| {
                    // Drop write end (close it in parent)
                    drop(pipe.write_end);
                    // Keep only read end for parent
                    ChannelPipe::parent_only(pipe.channel_fd, pipe.read_end)
                }).collect()
            } else {
                Vec::new()
            };

            // Return handle with channel pipes
            Ok(ProcessHandle::with_channels(child, parent_pipes))
        }
        Ok(ForkResult::Child) => {
            // Child process - F024 Phase 2: Apply channel redirects FIRST (before closing read ends)

            if has_channel_pipes {
                // Apply channel redirects while we still have write ends
                if let Err(e) = apply_channel_redirects_in_child(&channel_pipes) {
                    eprintln!("colony-exec: channel redirect failed: {}", e);
                    std::process::exit(126);
                }

                // Now close read ends (parent keeps these)
                for pipe in channel_pipes {
                    drop(pipe.read_end);
                    // write_end stays open for exec'd process to use
                }
            }

            // Apply regular redirects
            if let Err(e) = apply_redirects(redirects, context) {
                eprintln!("colony-exec: redirect failed: {}", e);
                std::process::exit(126);
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
