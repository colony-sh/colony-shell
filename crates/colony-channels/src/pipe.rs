//! Pipe-based redirection for external processes (P5-26)
//!
//! Redirects channel FDs when spawning child processes so that
//! external programs can read/write channels via inherited FDs.
//!
//! ## Architecture
//!
//! When spawning an external command that needs channel access:
//!
//! 1. Create OS pipes for each channel fd (3, 4, 5, 6)
//! 2. In the child, dup2 the pipe ends onto fd 3/4/5/6
//! 3. In the parent, read from the pipe and feed into the Channel
//! 4. On process exit, close pipe FDs and flush remaining data

use crate::{ChannelId, STDCONTEXT, STDDIAG, STDLLM, STDTOOL};
use std::collections::HashMap;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

/// A pipe pair: read end + write end
#[derive(Debug)]
pub struct PipePair {
    pub read_end: OwnedFd,
    pub write_end: OwnedFd,
}

impl PipePair {
    /// Create a new pipe pair
    pub fn new() -> std::io::Result<Self> {
        // SAFETY: pipe() returns two valid fds on success
        let (read_fd, write_fd) = nix::unistd::pipe()
            .map_err(std::io::Error::other)?;
        Ok(Self {
            read_end: read_fd,
            write_end: write_fd,
        })
    }

    /// Raw fd for the read end
    pub fn read_raw(&self) -> RawFd {
        self.read_end.as_raw_fd()
    }

    /// Raw fd for the write end
    pub fn write_raw(&self) -> RawFd {
        self.write_end.as_raw_fd()
    }
}

/// Manages pipe redirects for a child process
///
/// Creates pipes for channel FDs and provides methods to:
/// - Set up the child side (dup2 write ends onto channel FDs)
/// - Read from parent side and feed into channels
pub struct ChannelPipes {
    pipes: HashMap<ChannelId, PipePair>,
}

impl ChannelPipes {
    /// Create pipes for the standard channel FDs (3, 4, 5, 6)
    pub fn new_standard() -> std::io::Result<Self> {
        let mut pipes = HashMap::new();
        for &fd in &[STDCONTEXT, STDLLM, STDTOOL, STDDIAG] {
            pipes.insert(fd, PipePair::new()?);
        }
        Ok(Self { pipes })
    }

    /// Create pipes for specific channel FDs
    pub fn new_for(channel_ids: &[ChannelId]) -> std::io::Result<Self> {
        let mut pipes = HashMap::new();
        for &fd in channel_ids {
            pipes.insert(fd, PipePair::new()?);
        }
        Ok(Self { pipes })
    }

    /// Get the parent-side read fd for a channel
    pub fn parent_read_fd(&self, channel_id: ChannelId) -> Option<RawFd> {
        self.pipes.get(&channel_id).map(|p| p.read_raw())
    }

    /// Get the child-side write fd for a channel
    pub fn child_write_fd(&self, channel_id: ChannelId) -> Option<RawFd> {
        self.pipes.get(&channel_id).map(|p| p.write_raw())
    }

    /// Get all channel IDs that have pipes
    pub fn channel_ids(&self) -> Vec<ChannelId> {
        self.pipes.keys().copied().collect()
    }

    /// Prepare the child process side:
    /// - dup2 write ends onto the target FDs
    /// - Close the read ends (parent side)
    ///
    /// This must be called in the child process after fork, before exec.
    ///
    /// # Safety
    ///
    /// Must only be called in a forked child process. Uses dup2 to reassign FDs.
    pub unsafe fn setup_child(&self) -> std::io::Result<()> {
        for (&channel_id, pipe) in &self.pipes {
            let target_fd = channel_id as RawFd;
            let write_fd = pipe.write_raw();

            // Close read end in child (belongs to parent)
            nix::unistd::close(pipe.read_raw())
                .map_err(std::io::Error::other)?;

            // dup2 write end onto the target channel fd
            if write_fd != target_fd {
                nix::unistd::dup2(write_fd, target_fd)
                    .map_err(std::io::Error::other)?;
                nix::unistd::close(write_fd)
                    .map_err(std::io::Error::other)?;
            }
        }
        Ok(())
    }

    /// Close the child-side write ends in the parent process.
    ///
    /// Call this in the parent after forking. The parent only needs the read ends.
    pub fn close_child_ends(&mut self) -> std::io::Result<()> {
        for pipe in self.pipes.values() {
            nix::unistd::close(pipe.write_raw())
                .map_err(std::io::Error::other)?;
        }
        Ok(())
    }

    /// Take ownership of the parent-side read fds as OwnedFds.
    ///
    /// Returns a map of channel_id -> OwnedFd for the read ends.
    /// The caller is responsible for reading from these and feeding into channels.
    pub fn take_parent_read_fds(&mut self) -> HashMap<ChannelId, OwnedFd> {
        let mut fds = HashMap::new();
        for (&channel_id, pipe) in &self.pipes {
            let raw = pipe.read_raw();
            // SAFETY: We own these fds and are transferring ownership
            let owned = unsafe { OwnedFd::from_raw_fd(raw) };
            fds.insert(channel_id, owned);
        }
        fds
    }
}

/// Read all available data from a raw fd (non-blocking)
///
/// Returns the data read, or empty vec if no data available.
pub fn read_available(fd: RawFd) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; 4096];
    let mut all_data = Vec::new();

    loop {
        // SAFETY: fd is valid, buf is properly sized
        let n = nix::unistd::read(fd, &mut buf)
            .map_err(std::io::Error::other)?;

        if n == 0 {
            break;
        }

        all_data.extend_from_slice(&buf[..n]);

        if n < buf.len() {
            break;
        }
    }

    Ok(all_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipe_pair_creation() {
        let pipe = PipePair::new().unwrap();
        assert!(pipe.read_raw() >= 0);
        assert!(pipe.write_raw() >= 0);
        assert_ne!(pipe.read_raw(), pipe.write_raw());
    }

    #[test]
    fn test_pipe_read_write() {
        let pipe = PipePair::new().unwrap();
        let data = b"hello channel";

        nix::unistd::write(&pipe.write_end, data).unwrap();

        let mut buf = [0u8; 64];
        let n = nix::unistd::read(pipe.read_raw(), &mut buf).unwrap();
        assert_eq!(&buf[..n], data);
    }

    #[test]
    fn test_channel_pipes_standard() {
        let pipes = ChannelPipes::new_standard().unwrap();
        let ids = pipes.channel_ids();

        assert!(ids.contains(&STDCONTEXT));
        assert!(ids.contains(&STDLLM));
        assert!(ids.contains(&STDTOOL));
        assert!(ids.contains(&STDDIAG));
    }

    #[test]
    fn test_channel_pipes_custom() {
        let pipes = ChannelPipes::new_for(&[STDLLM]).unwrap();
        let ids = pipes.channel_ids();

        assert!(ids.contains(&STDLLM));
        assert!(!ids.contains(&STDTOOL));
    }

    #[test]
    fn test_channel_pipes_fd_accessors() {
        let pipes = ChannelPipes::new_standard().unwrap();

        assert!(pipes.parent_read_fd(STDLLM).is_some());
        assert!(pipes.child_write_fd(STDLLM).is_some());
        assert!(pipes.parent_read_fd(99).is_none());
    }

    #[test]
    fn test_pipe_data_through_channel_pipe() {
        let pipes = ChannelPipes::new_for(&[STDLLM]).unwrap();
        let write_fd = pipes.child_write_fd(STDLLM).unwrap();
        let read_fd = pipes.parent_read_fd(STDLLM).unwrap();

        let msg = b"test message through pipe";
        // SAFETY: write_fd is valid from ChannelPipes
        nix::unistd::write(unsafe { std::os::fd::BorrowedFd::borrow_raw(write_fd) }, msg).unwrap();

        let mut buf = [0u8; 128];
        let n = nix::unistd::read(read_fd, &mut buf).unwrap();
        assert_eq!(&buf[..n], msg);
    }
}
