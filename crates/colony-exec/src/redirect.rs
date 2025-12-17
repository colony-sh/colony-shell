//! I/O redirection handling
//!
//! Implements all POSIX redirection forms:
//! - File redirects: <, >, >>, >|, <>
//! - FD duplication: N>&M, N<&M
//! - FD closing: N>&-, N<&-
//! - Here-documents: <<, <<-
//! - Here-strings: <<<
//!
//! Redirects must be applied in the child process BEFORE execve(),
//! as they modify the file descriptor table.

use crate::{ExecutionError, Result};
use colony_expand::{ExpansionContext, Expander};
use colony_parser::{HereDocKind, Redirect, RedirectKind, RedirectTarget};
use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use nix::unistd::dup2;
use std::os::unix::io::RawFd;

/// Apply all redirects for a command in the current process
///
/// IMPORTANT: This must be called in the child process AFTER fork() but
/// BEFORE execve(). File descriptors are not inherited correctly otherwise.
///
/// Returns the list of FDs that were opened (caller must close in parent)
pub fn apply_redirects(
    redirects: &[Redirect],
    context: &mut ExpansionContext,
) -> Result<Vec<RawFd>> {
    let mut opened_fds = Vec::new();

    for redirect in redirects {
        let source_fd = get_source_fd(redirect);

        match &redirect.target {
            RedirectTarget::File(word) => {
                apply_file_redirect(redirect.kind, source_fd, word, context, &mut opened_fds)?;
            }
            RedirectTarget::Fd(target_fd) => {
                apply_fd_redirect(source_fd, *target_fd)?;
            }
            RedirectTarget::Close => {
                close_fd(source_fd)?;
            }
            RedirectTarget::HereDoc {
                content, expand, ..
            } => {
                apply_heredoc(source_fd, content, *expand, context)?;
            }
        }
    }

    Ok(opened_fds)
}

/// Get the source file descriptor for a redirect
///
/// Defaults:
/// - Input redirects: fd 0 (stdin)
/// - Output redirects: fd 1 (stdout)
fn get_source_fd(redirect: &Redirect) -> RawFd {
    redirect.fd.unwrap_or_else(|| match redirect.kind {
        RedirectKind::Input | RedirectKind::ReadWrite | RedirectKind::HereDoc(_) => 0,
        _ => 1,
    })
}

/// Apply a file redirect (>, >>, <, >|, <>)
fn apply_file_redirect(
    kind: RedirectKind,
    source_fd: RawFd,
    word: &colony_parser::Word,
    context: &mut ExpansionContext,
    opened_fds: &mut Vec<RawFd>,
) -> Result<()> {
    // Expand the filename
    let mut expander = Expander::new(context.clone());
    let fields = expander.expand_word(word)?;

    if fields.is_empty() {
        return Err(ExecutionError::AmbiguousRedirect);
    }
    if fields.len() > 1 {
        return Err(ExecutionError::AmbiguousRedirect);
    }

    let filename = &fields[0].text;

    // Determine open flags based on redirect type
    let (flags, mode) = match kind {
        RedirectKind::Input => (OFlag::O_RDONLY, Mode::empty()),
        RedirectKind::Output => (
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_TRUNC,
            Mode::from_bits_truncate(0o666),
        ),
        RedirectKind::Append => (
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_APPEND,
            Mode::from_bits_truncate(0o666),
        ),
        RedirectKind::Clobber => (
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_TRUNC,
            Mode::from_bits_truncate(0o666),
        ),
        RedirectKind::ReadWrite => (
            OFlag::O_RDWR | OFlag::O_CREAT,
            Mode::from_bits_truncate(0o666),
        ),
        RedirectKind::DupFd => {
            return Err(ExecutionError::InvalidRedirect(
                "DupFd should use apply_fd_redirect".to_string(),
            ))
        }
        RedirectKind::HereDoc(_) => {
            return Err(ExecutionError::InvalidRedirect(
                "HereDoc should use apply_heredoc".to_string(),
            ))
        }
    };

    // Open the file
    let target_fd = open(filename.as_str(), flags, mode)
        .map_err(|e| ExecutionError::RedirectError(format!("open {}: {}", filename, e)))?;

    opened_fds.push(target_fd);

    // Duplicate to the target file descriptor
    dup2(target_fd, source_fd)
        .map_err(|e| ExecutionError::RedirectError(format!("dup2: {}", e)))?;

    // Close the original (now duplicated)
    nix::unistd::close(target_fd)
        .map_err(|e| ExecutionError::RedirectError(format!("close: {}", e)))?;

    Ok(())
}

/// Apply fd duplication (N>&M or N<&M)
fn apply_fd_redirect(source_fd: RawFd, target_fd: RawFd) -> Result<()> {
    dup2(target_fd, source_fd)
        .map_err(|e| ExecutionError::RedirectError(format!("dup2: {}", e)))?;
    Ok(())
}

/// Close a file descriptor (N>&- or N<&-)
fn close_fd(fd: RawFd) -> Result<()> {
    nix::unistd::close(fd)
        .map_err(|e| ExecutionError::RedirectError(format!("close: {}", e)))?;
    Ok(())
}

/// Apply a heredoc or herestring (<<, <<-, <<<)
fn apply_heredoc(
    source_fd: RawFd,
    content: &str,
    expand: bool,
    context: &mut ExpansionContext,
) -> Result<()> {
    // Expand content if needed
    let final_content = if expand {
        // TODO: Expand variables in heredoc content
        // For now, just use literal content
        content.to_string()
    } else {
        content.to_string()
    };

    // Create a pipe
    use std::os::fd::{AsRawFd, BorrowedFd};
    let (read_fd, write_fd) = nix::unistd::pipe()
        .map_err(|e| ExecutionError::RedirectError(format!("pipe: {}", e)))?;

    // Write content to pipe
    use nix::unistd::write;
    let write_borrowed = unsafe { BorrowedFd::borrow_raw(write_fd.as_raw_fd()) };
    write(&write_borrowed, final_content.as_bytes())
        .map_err(|e| ExecutionError::RedirectError(format!("write: {}", e)))?;

    // Don't need to close write_fd explicitly - dropped automatically

    // Duplicate read end to source_fd
    dup2(read_fd.as_raw_fd(), source_fd)
        .map_err(|e| ExecutionError::RedirectError(format!("dup2: {}", e)))?;

    // read_fd automatically closed when dropped

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::{Redirect, RedirectKind, RedirectTarget, Span, Word};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_output_redirect() {
        // Test: echo hello > /tmp/colony_test_output.txt
        let test_file = "/tmp/colony_test_output.txt";
        let _ = fs::remove_file(test_file); // Clean up if exists

        let redirects = vec![Redirect {
            fd: None, // Default to stdout (1)
            kind: RedirectKind::Output,
            target: RedirectTarget::File(Word::literal(test_file, Span::DUMMY)),
            span: Span::DUMMY,
        }];

        let mut context = ExpansionContext::new();

        // This test spawns a real process with redirect
        let program = PathBuf::from("/bin/echo");
        let args = vec!["echo".to_string(), "hello".to_string()];
        let env = vec![];

        let handle = crate::process::spawn_process(&program, &args, &env, &redirects, &mut context)
            .unwrap();
        let status = handle.wait().unwrap();

        assert!(status.is_success());

        // Verify file was created and contains output
        let content = fs::read_to_string(test_file).unwrap();
        assert_eq!(content.trim(), "hello");

        // Clean up
        fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_input_redirect() {
        // Test: cat < /tmp/colony_test_input.txt
        let test_file = "/tmp/colony_test_input.txt";
        fs::write(test_file, "test input\n").unwrap();

        let redirects = vec![Redirect {
            fd: None, // Default to stdin (0)
            kind: RedirectKind::Input,
            target: RedirectTarget::File(Word::literal(test_file, Span::DUMMY)),
            span: Span::DUMMY,
        }];

        let mut context = ExpansionContext::new();

        let program = PathBuf::from("/bin/cat");
        let args = vec!["cat".to_string()];
        let env = vec![];

        let handle = crate::process::spawn_process(&program, &args, &env, &redirects, &mut context)
            .unwrap();
        let status = handle.wait().unwrap();

        assert!(status.is_success());

        // Clean up
        fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_append_redirect() {
        // Test: echo line1 > file && echo line2 >> file
        let test_file = "/tmp/colony_test_append.txt";
        let _ = fs::remove_file(test_file);

        // First: echo line1 > file
        let redirect1 = vec![Redirect {
            fd: None,
            kind: RedirectKind::Output,
            target: RedirectTarget::File(Word::literal(test_file, Span::DUMMY)),
            span: Span::DUMMY,
        }];

        let mut context = ExpansionContext::new();
        let program = PathBuf::from("/bin/echo");
        let args1 = vec!["echo".to_string(), "line1".to_string()];
        let env = vec![];

        let handle = crate::process::spawn_process(&program, &args1, &env, &redirect1, &mut context)
            .unwrap();
        handle.wait().unwrap();

        // Second: echo line2 >> file (append)
        let redirect2 = vec![Redirect {
            fd: None,
            kind: RedirectKind::Append,
            target: RedirectTarget::File(Word::literal(test_file, Span::DUMMY)),
            span: Span::DUMMY,
        }];

        let args2 = vec!["echo".to_string(), "line2".to_string()];
        let handle = crate::process::spawn_process(&program, &args2, &env, &redirect2, &mut context)
            .unwrap();
        handle.wait().unwrap();

        // Verify both lines are in file
        let content = fs::read_to_string(test_file).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");

        // Clean up
        fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_fd_duplication() {
        // Test: cmd 2>&1 (redirect stderr to stdout)
        let test_file = "/tmp/colony_test_fd_dup.txt";
        let _ = fs::remove_file(test_file);

        let redirects = vec![
            // Redirect stdout to file
            Redirect {
                fd: Some(1),
                kind: RedirectKind::Output,
                target: RedirectTarget::File(Word::literal(test_file, Span::DUMMY)),
                span: Span::DUMMY,
            },
            // Duplicate stderr to stdout (now both go to file)
            Redirect {
                fd: Some(2),
                kind: RedirectKind::DupFd,
                target: RedirectTarget::Fd(1),
                span: Span::DUMMY,
            },
        ];

        let mut context = ExpansionContext::new();
        // Use a command that writes to both stdout and stderr
        let program = PathBuf::from("/bin/sh");
        let args = vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo stdout; echo stderr >&2".to_string(),
        ];
        let env = vec![];

        let handle = crate::process::spawn_process(&program, &args, &env, &redirects, &mut context)
            .unwrap();
        let status = handle.wait().unwrap();

        assert!(status.is_success());

        // Both stdout and stderr should be in the file
        let content = fs::read_to_string(test_file).unwrap();
        assert!(content.contains("stdout"));
        assert!(content.contains("stderr"));

        // Clean up
        fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_heredoc() {
        // Test: cat << EOF
        // hello
        // world
        // EOF
        let heredoc_content = "hello\nworld\n";

        let redirects = vec![Redirect {
            fd: None, // Default to stdin
            kind: RedirectKind::HereDoc(HereDocKind::Normal),
            target: RedirectTarget::HereDoc {
                delimiter: "EOF".to_string(),
                content: heredoc_content.to_string(),
                expand: true,
            },
            span: Span::DUMMY,
        }];

        let mut context = ExpansionContext::new();

        let program = PathBuf::from("/bin/cat");
        let args = vec!["cat".to_string()];
        let env = vec![];

        let handle = crate::process::spawn_process(&program, &args, &env, &redirects, &mut context)
            .unwrap();
        let status = handle.wait().unwrap();

        assert!(status.is_success());
        // Note: Output goes to stdout, we'd need to capture it to verify
    }
}
