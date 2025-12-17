//! Channel trait and types
//!
//! Channels extend the Unix fd model with semantic meaning.
//! Each channel accumulates structured information during the session.

use crate::Result;
use std::io;

/// Channel identifier (file descriptor number)
pub type ChannelId = u8;

/// Standard channel constants
pub const STDIN: ChannelId = 0;
pub const STDOUT: ChannelId = 1;
pub const STDERR: ChannelId = 2;
pub const STDCONTEXT: ChannelId = 3;
pub const STDLLM: ChannelId = 4;
pub const STDTOOL: ChannelId = 5;
pub const STDTRACE: ChannelId = 6;

/// Channel trait - all channels implement this
///
/// Channels provide write/read access to semantic information streams.
/// Phase 1: Builtins write/read directly (parent process)
/// Phase 2: External commands write via pipe redirects (child process)
pub trait Channel: Send + Sync {
    /// Get channel ID (file descriptor number)
    fn id(&self) -> ChannelId;

    /// Get channel name for display
    fn name(&self) -> &str;

    /// Write raw bytes to channel
    ///
    /// For Phase 1, this is called by builtins directly.
    /// Phase 2 will pipe child process writes here.
    fn write(&mut self, data: &[u8]) -> io::Result<usize>;

    /// Read accumulated channel content
    ///
    /// Returns accumulated data from buffer.
    /// Builtins use this to query channel state.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;

    /// Flush any pending writes
    fn flush(&mut self) -> io::Result<()>;

    /// Clear channel content
    fn clear(&mut self);

    /// Get line count (for display)
    fn line_count(&self) -> usize;

    /// Downcast to Any for type-specific access
    fn as_any(&self) -> &dyn std::any::Any;

    /// Downcast to Any for mutable type-specific access
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_constants() {
        assert_eq!(STDIN, 0);
        assert_eq!(STDOUT, 1);
        assert_eq!(STDERR, 2);
        assert_eq!(STDCONTEXT, 3);
        assert_eq!(STDLLM, 4);
        assert_eq!(STDTOOL, 5);
        assert_eq!(STDTRACE, 6);
    }
}
