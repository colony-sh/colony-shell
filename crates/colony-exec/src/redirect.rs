//! I/O redirection handling
//!
//! Implements all POSIX redirection forms:
//! - File redirects: <, >, >>, >|, <>
//! - FD duplication: N>&M, N<&M
//! - FD closing: N>&-, N<&-
//! - Here-documents: <<, <<-
//! - Here-strings: <<<
//!
//! **F024 Phase 2:** Extended channels (fd 3-6)
//! - Channel redirects: >&3, >&4, >&5, >&6
//! - Creates pipes, captures output, writes to tuple space
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

/// Channel file descriptors (F024)
pub const STDCONTEXT: RawFd = 3;
pub const STDLLM: RawFd = 4;
pub const STDTOOL: RawFd = 5;
pub const STDTRACE: RawFd = 6;

/// Check if fd is a channel fd (3-6)
pub fn is_channel_fd(fd: RawFd) -> bool {
    matches!(fd, STDCONTEXT | STDLLM | STDTOOL | STDTRACE)
}

/// Channel pipe - represents a pipe created for capturing channel writes
#[derive(Debug)]
pub struct ChannelPipe {
    /// Target channel fd (3-6)
    pub channel_fd: RawFd,
    /// Read end (parent keeps this)
    pub read_end: std::os::fd::OwnedFd,
    /// Write end (child uses this, then closes)
    /// None in parent (already closed/moved to child)
    pub write_end: Option<std::os::fd::OwnedFd>,
}

impl ChannelPipe {
    /// Create a new channel pipe with both ends
    pub fn new(channel_fd: RawFd, read_end: std::os::fd::OwnedFd, write_end: std::os::fd::OwnedFd) -> Self {
        Self {
            channel_fd,
            read_end,
            write_end: Some(write_end),
        }
    }

    /// Create a parent-side pipe (read end only)
    pub fn parent_only(channel_fd: RawFd, read_end: std::os::fd::OwnedFd) -> Self {
        Self {
            channel_fd,
            read_end,
            write_end: None,
        }
    }
}

/// Prepare channel redirects BEFORE forking
///
/// Detects redirects to fd 3-6 and creates pipes for them.
/// Returns vector of ChannelPipe structs.
/// Parent keeps read_ends, child uses write_ends.
///
/// Examples:
/// - `>&3` - redirect stdout to channel fd 3
/// - `2>&4` - redirect stderr to channel fd 4
/// - `3>&-` - close channel fd 3 (no pipe needed)
pub fn prepare_channel_redirects(
    redirects: &[Redirect],
) -> Result<Vec<ChannelPipe>> {
    let mut channel_pipes = Vec::new();
    let mut prepared_fds = std::collections::HashSet::new();

    for redirect in redirects {
        // Check if redirecting TO a channel fd
        let channel_fd = match &redirect.target {
            RedirectTarget::Fd(target_fd) => {
                if is_channel_fd(*target_fd) {
                    Some(*target_fd)
                } else {
                    None
                }
            }
            RedirectTarget::File(word) => {
                // WORKAROUND: Parser bug - >&3 creates File("3") instead of Fd(3)
                if matches!(redirect.kind, RedirectKind::DupFd) {
                    if let Some(literal) = word.as_literal() {
                        if let Ok(target_fd) = literal.parse::<RawFd>() {
                            if is_channel_fd(target_fd) {
                                Some(target_fd)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(fd) = channel_fd {
            // Avoid duplicate pipes for same fd
            if prepared_fds.contains(&fd) {
                continue;
            }
            prepared_fds.insert(fd);

            tracing::debug!("Preparing channel pipe for fd {}", fd);

            // Create pipe for this channel
            let (read_end, write_end) = nix::unistd::pipe()
                .map_err(|e| ExecutionError::RedirectError(format!("pipe for channel: {}", e)))?;

            channel_pipes.push(ChannelPipe::new(fd, read_end, write_end));
        }
    }

    Ok(channel_pipes)
}

/// Apply all redirects for a command in the current process
///
/// IMPORTANT: This must be called in the child process AFTER fork() but
/// BEFORE execve(). File descriptors are not inherited correctly otherwise.
///
/// Returns the list of FDs that were opened (caller must close in parent).
///
/// **F024 Phase 2 Note:** This function is now only for regular (non-channel) redirects.
/// Channel redirects (fd 3-6) are handled separately via prepare_channel_redirects()
/// and apply_channel_redirects_in_child().
pub fn apply_redirects(
    redirects: &[Redirect],
    context: &mut ExpansionContext,
) -> Result<Vec<RawFd>> {
    let mut opened_fds = Vec::new();

    for redirect in redirects {
        // Handle {varname}>file fd allocation (vredir)
        if let Some(ref var_name) = redirect.fd_var {
            if var_name.is_empty() {
                return Err(ExecutionError::RedirectError("empty variable name in fd redirect".to_string()));
            }
            match &redirect.target {
                RedirectTarget::File(word) => {
                    // {fd}>file or {fd}<file - allocate fd and open file
                    if !matches!(redirect.kind, RedirectKind::DupFd | RedirectKind::DupFdInput) {
                        let allocated_fd = apply_vredir_file(redirect.kind, var_name, word, context, &mut opened_fds)?;
                        context.set_var(var_name.clone(), allocated_fd.to_string());
                        tracing::debug!("vredir: allocated fd {} for variable {}", allocated_fd, var_name);
                        continue;
                    }
                }
                RedirectTarget::Close => {
                    let fd = get_source_fd_with_context(redirect, context)?;
                    tracing::debug!("vredir: closing fd {} for variable {:?}", fd, var_name);
                    close_fd(fd)?;
                    continue;
                }
                RedirectTarget::Fd(target_fd) => {
                    // {fd}>&N - dup target to allocated fd
                    let source_fd = get_source_fd_with_context(redirect, context)?;
                    apply_fd_redirect(source_fd, *target_fd)?;
                    continue;
                }
                RedirectTarget::HereDoc { content, expand, strip_tabs, .. } => {
                    let allocated_fd = allocate_heredoc_fd(content, *expand, *strip_tabs, context)?;
                    context.set_var(var_name.clone(), allocated_fd.to_string());
                    continue;
                }
            }
        }

        let source_fd = if redirect.fd_var.is_some() {
            get_source_fd_with_context(redirect, context)?
        } else {
            get_source_fd(redirect)
        };

        tracing::debug!(
            "apply_redirects: kind={:?}, source_fd={}, fd_var={:?}, target={:?}",
            redirect.kind,
            source_fd,
            redirect.fd_var,
            redirect.target
        );

        match &redirect.target {
            RedirectTarget::File(word) => {
                if matches!(redirect.kind, RedirectKind::HereDoc(HereDocKind::HereString)) {
                    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
                        crate::execute_and_capture(stmt, ctx)
                    });
                    let content = if let Ok(fields) = expander.expand_word(word) {
                        fields.iter().map(|f| f.text.clone()).collect::<Vec<_>>().join(" ")
                    } else {
                        word.as_literal().unwrap_or_default().to_string()
                    };
                    apply_here_string(source_fd, &content)?;
                    continue;
                }
                
                if matches!(redirect.kind, RedirectKind::DupFd | RedirectKind::DupFdInput) {
                    if let Some(literal) = word.as_literal() {
                        if let Ok(target_fd) = literal.parse::<RawFd>() {
                            tracing::debug!("DupFd literal: '{}' → Fd({})", literal, target_fd);
                            apply_fd_redirect(source_fd, target_fd)?;
                            continue;
                        }
                    }
                    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
                        crate::execute_and_capture(stmt, ctx)
                    });
                    if let Ok(expanded_fields) = expander.expand_word(word) {
                        if let Some(first) = expanded_fields.first() {
                            if let Ok(target_fd) = first.text.parse::<RawFd>() {
                                tracing::debug!("DupFd expanded: '{}' → Fd({})", first.text, target_fd);
                                apply_fd_redirect(source_fd, target_fd)?;
                                continue;
                            }
                        }
                    }
                    return Err(ExecutionError::RedirectError(
                        format!("Invalid fd for redirect: {:?}", word)
                    ));
                }

                tracing::debug!("Applying file redirect");
                apply_file_redirect(redirect.kind, source_fd, word, context, &mut opened_fds)?;
            }
            RedirectTarget::Fd(target_fd) => {
                tracing::debug!("Target is Fd({}), is_channel={}", target_fd, is_channel_fd(*target_fd));
                apply_fd_redirect(source_fd, *target_fd)?;
            }
            RedirectTarget::Close => {
                close_fd(source_fd)?;
            }
            RedirectTarget::HereDoc {
                content, expand, strip_tabs, ..
            } => {
                apply_heredoc(source_fd, content, *expand, *strip_tabs, context)?;
            }
        }
    }

    Ok(opened_fds)
}

/// Apply vredir file redirect: {varname}>file
/// Opens the file, then dups to an fd >= 10 using F_DUPFD
fn apply_vredir_file(
    kind: RedirectKind,
    _var_name: &str,
    word: &colony_parser::Word,
    context: &mut ExpansionContext,
    opened_fds: &mut Vec<RawFd>,
) -> Result<RawFd> {
    use nix::fcntl::{fcntl, FcntlArg};
    
    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
        crate::execute_and_capture(stmt, ctx)
    });
    let fields = expander.expand_word(word)?;

    if fields.is_empty() || fields.len() > 1 {
        return Err(ExecutionError::AmbiguousRedirect);
    }

    let filename = &fields[0].text;

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
        _ => return Err(ExecutionError::InvalidRedirect(
            format!("Unsupported redirect kind for vredir: {:?}", kind)
        )),
    };

    let file_fd = open(filename.as_str(), flags, mode)
        .map_err(|e| ExecutionError::RedirectError(format!("open {}: {}", filename, e)))?;

    opened_fds.push(file_fd);

    let allocated_fd = fcntl(file_fd, FcntlArg::F_DUPFD(10))
        .map_err(|e| ExecutionError::RedirectError(format!("F_DUPFD: {}", e)))?;

    nix::unistd::close(file_fd)
        .map_err(|e| ExecutionError::RedirectError(format!("close: {}", e)))?;

    Ok(allocated_fd)
}

fn allocate_heredoc_fd(
    content: &str,
    expand: bool,
    strip_tabs: bool,
    context: &mut ExpansionContext,
) -> Result<RawFd> {
    use nix::fcntl::{fcntl, FcntlArg};
    use std::os::fd::{AsRawFd, BorrowedFd};
    
    let expanded_content = if expand {
        expand_heredoc_content(content, context)
    } else {
        content.to_string()
    };
    
    let final_content = if strip_tabs {
        expanded_content
            .lines()
            .map(|line| line.trim_start_matches('\t'))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        expanded_content
    };

    let (read_fd, write_fd) = nix::unistd::pipe()
        .map_err(|e| ExecutionError::RedirectError(format!("pipe: {}", e)))?;

    let write_borrowed = unsafe { BorrowedFd::borrow_raw(write_fd.as_raw_fd()) };
    nix::unistd::write(&write_borrowed, final_content.as_bytes())
        .map_err(|e| ExecutionError::RedirectError(format!("write: {}", e)))?;
    
    drop(write_fd);

    let allocated_fd = fcntl(read_fd.as_raw_fd(), FcntlArg::F_DUPFD(10))
        .map_err(|e| ExecutionError::RedirectError(format!("F_DUPFD: {}", e)))?;

    Ok(allocated_fd)
}

/// Apply channel redirects in child process
///
/// Takes the write ends of channel pipes and dups them to the appropriate fds.
/// Called in child after fork, before exec.
pub fn apply_channel_redirects_in_child(channel_pipes: &[ChannelPipe]) -> Result<()> {
    use std::os::fd::AsRawFd;

    for pipe in channel_pipes {
        // Dup the write end to the channel fd
        if let Some(ref write_end) = pipe.write_end {
            dup2(write_end.as_raw_fd(), pipe.channel_fd)
                .map_err(|e| ExecutionError::RedirectError(format!("dup2 channel: {}", e)))?;
        } else {
            return Err(ExecutionError::RedirectError(
                "Channel pipe missing write end in child".to_string()
            ));
        }
    }

    Ok(())
}

/// Read from channel pipes and write to tuple space
///
/// Called in PARENT after child exits. Reads captured output from
/// pipes and emits tuples: ("channel_write", fd, content, timestamp)
///
/// **F024 Phase 2:** This enables child processes to write to channels
/// via tuple space IPC.
pub fn process_channel_writes(
    channel_pipes: Vec<ChannelPipe>,
    tuple_space: &colony_tuple::TupleSpace,
) -> Result<()> {
    use colony_tuple::{Tuple, Value};
    use nix::unistd::read;
    use std::os::fd::AsRawFd;

    tracing::debug!("process_channel_writes: {} pipes to process", channel_pipes.len());

    for pipe in channel_pipes {
        tracing::debug!("Reading from channel pipe fd={}", pipe.channel_fd);

        // Read all data from pipe using nix::unistd::read
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];

        loop {
            match read(pipe.read_end.as_raw_fd(), &mut chunk) {
                Ok(0) => {
                    tracing::debug!("EOF on pipe fd={}", pipe.channel_fd);
                    break; // EOF
                }
                Ok(n) => {
                    tracing::debug!("Read {} bytes from pipe fd={}", n, pipe.channel_fd);
                    buffer.extend_from_slice(&chunk[..n])
                }
                Err(e) => {
                    tracing::warn!("Failed to read from channel pipe: {}", e);
                    break;
                }
            }
        }

        // Convert to string (lossy for non-UTF8)
        let content = String::from_utf8_lossy(&buffer).to_string();

        tracing::debug!("Captured {} bytes from fd={}: {:?}", buffer.len(), pipe.channel_fd, content);

        if !content.is_empty() {
            // Emit tuple: ("channel_write", fd, content, timestamp)
            let timestamp = chrono::Utc::now().to_rfc3339();
            tuple_space.out(Tuple::from(vec![
                Value::Str("channel_write".to_string()),
                Value::Int(pipe.channel_fd as i64),
                Value::Str(content.clone()),
                Value::Str(timestamp),
            ]));

            tracing::debug!(
                "Channel write captured: fd={}, content={:?}",
                pipe.channel_fd,
                content
            );
        } else {
            tracing::debug!("No content captured from fd={}", pipe.channel_fd);
        }

        // pipe.read_end automatically closed when dropped
    }

    tracing::debug!("process_channel_writes complete");
    Ok(())
}

/// Get source fd, handling both literal fd numbers and {VAR} syntax
fn get_source_fd_with_context(redirect: &Redirect, context: &ExpansionContext) -> Result<RawFd> {
    if let Some(ref var_name) = redirect.fd_var {
        if let Some(bracket_pos) = var_name.find('[') {
            if let Some(end_pos) = var_name.find(']') {
                let name = &var_name[..bracket_pos];
                let index_str = &var_name[bracket_pos+1..end_pos];
                
                if let Some(colony_expand::Variable::Array(arr)) = context.get_variable(name) {
                    if let Ok(index) = index_str.parse::<usize>() {
                        if let Some(val) = arr.get(index) {
                            return val.parse::<RawFd>()
                                .map_err(|_| ExecutionError::RedirectError(
                                    format!("Invalid fd from variable: {}", val)
                                ));
                        }
                    }
                }
            }
        }
        let value = context.get_var_or_env(var_name).unwrap_or_default();
        return value.parse::<RawFd>()
            .map_err(|_| ExecutionError::RedirectError(
                format!("Invalid fd from variable {}: {}", var_name, value)
            ));
    }
    
    Ok(redirect.fd.unwrap_or_else(|| match redirect.kind {
        RedirectKind::Input | RedirectKind::ReadWrite | RedirectKind::HereDoc(_) | RedirectKind::DupFdInput => 0,
        _ => 1,
    }))
}

/// Get the source file descriptor for a redirect
///
/// Defaults:
/// - Input redirects: fd 0 (stdin)
/// - Output redirects: fd 1 (stdout)
fn get_source_fd(redirect: &Redirect) -> RawFd {
    redirect.fd.unwrap_or_else(|| match redirect.kind {
        RedirectKind::Input | RedirectKind::ReadWrite | RedirectKind::HereDoc(_) | RedirectKind::DupFdInput => 0,
        _ => 1,
    })
}

/// Apply a file redirect (>, >>, <, >|, <>, &>, &>>)
fn apply_file_redirect(
    kind: RedirectKind,
    source_fd: RawFd,
    word: &colony_parser::Word,
    context: &mut ExpansionContext,
    opened_fds: &mut Vec<RawFd>,
) -> Result<()> {
    // Expand the filename (with command substitution support)
    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
        crate::execute_and_capture(stmt, ctx)
    });
    let fields = expander.expand_word(word)?;

    if fields.is_empty() {
        return Err(ExecutionError::AmbiguousRedirect);
    }
    if fields.len() > 1 {
        return Err(ExecutionError::AmbiguousRedirect);
    }

    let filename = &fields[0].text;

    // Handle &> and &>> (redirect both stdout and stderr)
    if matches!(kind, RedirectKind::AndOutput | RedirectKind::AndAppend) {
        let flags = if kind == RedirectKind::AndOutput {
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_TRUNC
        } else {
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_APPEND
        };
        let mode = Mode::from_bits_truncate(0o666);
        
        let target_fd = open(filename.as_str(), flags, mode)
            .map_err(|e| ExecutionError::RedirectError(format!("open {}: {}", filename, e)))?;
        
        opened_fds.push(target_fd);
        
        // Redirect stdout to file
        dup2(target_fd, 1)
            .map_err(|e| ExecutionError::RedirectError(format!("dup2 stdout: {}", e)))?;
        // Redirect stderr to same file (via stdout)
        dup2(1, 2)
            .map_err(|e| ExecutionError::RedirectError(format!("dup2 stderr: {}", e)))?;
        
        nix::unistd::close(target_fd)
            .map_err(|e| ExecutionError::RedirectError(format!("close: {}", e)))?;
        
        return Ok(());
    }

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
        RedirectKind::DupFd | RedirectKind::DupFdInput => {
            return Err(ExecutionError::InvalidRedirect(
                "DupFd should use apply_fd_redirect".to_string(),
            ))
        }
        RedirectKind::HereDoc(_) => {
            return Err(ExecutionError::InvalidRedirect(
                "HereDoc should use apply_heredoc".to_string(),
            ))
        }
        RedirectKind::AndOutput | RedirectKind::AndAppend => {
            unreachable!("Already handled above")
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
    tracing::debug!("apply_fd_redirect: dup2({}, {})", target_fd, source_fd);
    match dup2(target_fd, source_fd) {
        Ok(_) => {
            tracing::debug!("apply_fd_redirect: dup2 succeeded");
            Ok(())
        }
        Err(e) => {
            tracing::debug!("apply_fd_redirect: dup2 failed: {}", e);
            Err(ExecutionError::RedirectError(format!("dup2: {}", e)))
        }
    }
}

/// Close a file descriptor (N>&- or N<&-)
fn close_fd(fd: RawFd) -> Result<()> {
    tracing::debug!("close_fd: closing fd {}", fd);
    match nix::unistd::close(fd) {
        Ok(_) => {
            tracing::debug!("close_fd: fd {} closed successfully", fd);
            Ok(())
        }
        Err(e) => {
            tracing::debug!("close_fd: failed to close fd {}: {}", fd, e);
            Err(ExecutionError::RedirectError(format!("close: {}", e)))
        }
    }
}

fn expand_heredoc_content(content: &str, context: &mut ExpansionContext) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '$' {
            if let Some(&next_ch) = chars.peek() {
                if next_ch == '{' {
                    chars.next();
                    let mut var_name = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch == '}' {
                            chars.next();
                            break;
                        }
                        var_name.push(chars.next().unwrap());
                    }
                    if let Some(value) = context.get_var_or_env(&var_name) {
                        result.push_str(&value);
                    }
                } else if next_ch.is_alphanumeric() || next_ch == '_' {
                    let mut var_name = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch.is_alphanumeric() || ch == '_' {
                            var_name.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    if let Some(value) = context.get_var_or_env(&var_name) {
                        result.push_str(&value);
                    }
                } else {
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }
    
    result
}

fn apply_heredoc(
    source_fd: RawFd,
    content: &str,
    expand: bool,
    strip_tabs: bool,
    context: &mut ExpansionContext,
) -> Result<()> {
    let expanded_content = if expand {
        expand_heredoc_content(content, context)
    } else {
        content.to_string()
    };
    
    let final_content = if strip_tabs {
        expanded_content
            .lines()
            .map(|line| line.trim_start_matches('\t'))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        expanded_content
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

fn apply_here_string(source_fd: RawFd, content: &str) -> Result<()> {
    use std::os::fd::{AsRawFd, BorrowedFd};
    
    let (read_fd, write_fd) = nix::unistd::pipe()
        .map_err(|e| ExecutionError::RedirectError(format!("pipe: {}", e)))?;

    let mut final_content = content.to_string();
    final_content.push('\n');
    
    let write_borrowed = unsafe { BorrowedFd::borrow_raw(write_fd.as_raw_fd()) };
    nix::unistd::write(&write_borrowed, final_content.as_bytes())
        .map_err(|e| ExecutionError::RedirectError(format!("write: {}", e)))?;

    drop(write_fd);

    dup2(read_fd.as_raw_fd(), source_fd)
        .map_err(|e| ExecutionError::RedirectError(format!("dup2: {}", e)))?;

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
            fd: None,
            fd_var: None,
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
            fd: None,
            fd_var: None,
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
            fd_var: None,
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
            fd_var: None,
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
            Redirect {
                fd: Some(1),
                fd_var: None,
                kind: RedirectKind::Output,
                target: RedirectTarget::File(Word::literal(test_file, Span::DUMMY)),
                span: Span::DUMMY,
            },
            Redirect {
                fd: Some(2),
                fd_var: None,
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
            fd: None,
            fd_var: None,
            kind: RedirectKind::HereDoc(HereDocKind::Normal),
            target: RedirectTarget::HereDoc {
                delimiter: "EOF".to_string(),
                content: heredoc_content.to_string(),
                expand: true,
                strip_tabs: false,
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
