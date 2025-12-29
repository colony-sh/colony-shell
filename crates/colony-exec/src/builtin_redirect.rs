//! Redirect support for builtin commands
//!
//! Builtins run in the shell process (no fork), so redirects must be
//! applied to the shell's own file descriptors temporarily.
//!
//! Strategy:
//! 1. Save current FDs that will be redirected (using F_DUPFD to fds >= 100)
//! 2. Apply redirects using dup2 on shell's FDs
//! 3. Execute builtin
//! 4. Restore saved FDs
//!
//! This matches bash's approach for builtin redirects.

use crate::{ExecutionError, Result};
use colony_expand::ExpansionContext;
use colony_parser::Redirect;
use nix::fcntl::{fcntl, FcntlArg};
use std::os::unix::io::RawFd;

/// Minimum fd number for saved fds (to avoid conflicts with user fds like {fd}>file)
const SAVED_FD_MIN: RawFd = 100;

/// Saved file descriptor for restoration
struct SavedFd {
    original_fd: RawFd,
    saved_fd: RawFd,
}

/// RAII guard for builtin redirects
///
/// Saves FDs on creation, restores on drop.
/// This ensures FDs are restored even if builtin panics.
pub struct BuiltinRedirectGuard {
    saved_fds: Vec<SavedFd>,
}

impl BuiltinRedirectGuard {
    /// Apply redirects for a builtin command
    ///
    /// Returns a guard that will restore FDs when dropped.
    pub fn apply(redirects: &[Redirect], context: &mut ExpansionContext) -> Result<Self> {
        let mut saved_fds = Vec::new();

        if redirects.is_empty() {
            return Ok(Self { saved_fds });
        }

        // Determine which FDs will be affected
        let affected_fds = get_affected_fds(redirects);
        tracing::debug!("BuiltinRedirectGuard: affected_fds={:?}", affected_fds);

        // Save affected FDs by duplicating them to high fd numbers (>= 100)
        // This avoids conflicts with user-allocated fds like exec {fd}>file
        // Only save FDs that currently exist (don't try to save fd 3 if it will be created)
        for &fd in &affected_fds {
            match fcntl(fd, FcntlArg::F_DUPFD(SAVED_FD_MIN)) {
                Ok(saved_fd) => {
                    tracing::debug!(
                        "BuiltinRedirectGuard: F_DUPFD({}, {}) = {}",
                        fd,
                        SAVED_FD_MIN,
                        saved_fd
                    );
                    saved_fds.push(SavedFd {
                        original_fd: fd,
                        saved_fd,
                    });
                }
                Err(nix::errno::Errno::EBADF) => {
                    tracing::debug!(
                        "BuiltinRedirectGuard: fd {} doesn't exist, will be created by redirect",
                        fd
                    );
                }
                Err(e) => {
                    // Clean up any FDs we've already saved
                    for saved in saved_fds {
                        let _ = nix::unistd::close(saved.saved_fd);
                    }
                    return Err(ExecutionError::RedirectError(format!(
                        "Failed to save fd {}: {}",
                        fd, e
                    )));
                }
            }
        }

        // Apply redirects to the shell's FDs
        // Note: We don't track opened FDs here because restore will fix everything
        let _opened_fds = crate::apply_redirects(redirects, context)?;

        Ok(Self { saved_fds })
    }
}

impl Drop for BuiltinRedirectGuard {
    fn drop(&mut self) {
        use nix::unistd::{close, dup2};

        // Restore saved FDs
        for saved in &self.saved_fds {
            // Restore: dup2(saved_fd, original_fd)
            if let Err(e) = dup2(saved.saved_fd, saved.original_fd) {
                eprintln!("Warning: Failed to restore fd {}: {}", saved.original_fd, e);
            }

            // Close the saved copy
            if let Err(e) = close(saved.saved_fd) {
                eprintln!(
                    "Warning: Failed to close saved fd {}: {}",
                    saved.saved_fd, e
                );
            }
        }
    }
}

/// Determine which FDs will be affected by redirects
fn get_affected_fds(redirects: &[Redirect]) -> Vec<RawFd> {
    use colony_parser::RedirectKind;
    let mut fds = Vec::new();

    for redirect in redirects {
        // Determine source FD
        let source_fd = redirect.fd.unwrap_or_else(|| match redirect.kind {
            RedirectKind::Input | RedirectKind::ReadWrite | RedirectKind::HereDoc(_) => 0,
            _ => 1,
        });

        if !fds.contains(&source_fd) {
            fds.push(source_fd);
        }
    }

    fds
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::{Redirect, RedirectKind, RedirectTarget, Span, Word};

    #[test]
    fn test_get_affected_fds() {
        let redirects = vec![Redirect {
            fd: None,
            fd_var: None,
            kind: RedirectKind::Output,
            target: RedirectTarget::File(Word::literal("file.txt", Span::DUMMY)),
            span: Span::DUMMY,
        }];

        let fds = get_affected_fds(&redirects);
        assert_eq!(fds, vec![1]);

        let redirects = vec![Redirect {
            fd: None,
            fd_var: None,
            kind: RedirectKind::Input,
            target: RedirectTarget::File(Word::literal("file.txt", Span::DUMMY)),
            span: Span::DUMMY,
        }];

        let fds = get_affected_fds(&redirects);
        assert_eq!(fds, vec![0]);
    }
}
