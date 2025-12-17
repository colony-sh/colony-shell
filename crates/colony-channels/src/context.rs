//! Context channel (stdcontext - fd 3)
//!
//! Accumulates session context: observations, decisions, actions, errors.
//! Phase 1: Simple text entries with timestamps
//! Phase 2: Structured ContextEntry with types and metadata

use crate::{Channel, ChannelId, InternalBuffer, STDCONTEXT};
use std::io;

/// Context channel for session observations
pub struct ContextChannel {
    buffer: InternalBuffer,
}

impl ContextChannel {
    pub fn new() -> Self {
        Self {
            buffer: InternalBuffer::new(1000), // 1000 context entries max
        }
    }

    /// Add a context entry (text-based for Phase 1)
    ///
    /// Phase 2 will add structured entries with ContextType
    pub fn add_text(&mut self, text: impl Into<String>) {
        self.buffer.append_line(text);
    }

    /// Get all context lines
    pub fn lines(&self) -> Vec<String> {
        self.buffer
            .lines()
            .iter()
            .map(|line| line.text.clone())
            .collect()
    }

    /// Get last N context entries
    pub fn last(&self, n: usize) -> Vec<String> {
        self.buffer
            .last_lines(n)
            .iter()
            .map(|line| line.text.clone())
            .collect()
    }

    /// Search context for text
    pub fn search(&self, pattern: &str) -> Vec<String> {
        self.buffer
            .lines()
            .iter()
            .filter(|line| line.text.contains(pattern))
            .map(|line| line.text.clone())
            .collect()
    }

    /// Export context to string
    pub fn export(&self) -> String {
        self.buffer.to_plain_text()
    }
}

impl Default for ContextChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel for ContextChannel {
    fn id(&self) -> ChannelId {
        STDCONTEXT
    }

    fn name(&self) -> &str {
        "context"
    }

    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(data);

        // Split into lines and add each
        for line in text.lines() {
            if !line.is_empty() {
                self.add_text(line);
            }
        }

        Ok(data.len())
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let text = self.export();
        let bytes = text.as_bytes();
        let len = buf.len().min(bytes.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(()) // No-op for in-memory buffer
    }

    fn clear(&mut self) {
        self.buffer.clear();
    }

    fn line_count(&self) -> usize {
        self.buffer.len()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_add() {
        let mut ctx = ContextChannel::new();
        ctx.add_text("Started task");
        ctx.add_text("Made decision");

        assert_eq!(ctx.line_count(), 2);
        assert_eq!(ctx.lines()[0], "Started task");
        assert_eq!(ctx.lines()[1], "Made decision");
    }

    #[test]
    fn test_context_search() {
        let mut ctx = ContextChannel::new();
        ctx.add_text("Started feature X");
        ctx.add_text("Completed feature Y");
        ctx.add_text("Started feature Z");

        let results = ctx.search("Started");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_context_channel_trait() {
        let mut ctx = ContextChannel::new();

        // Write via Channel trait
        assert_eq!(ctx.id(), STDCONTEXT);
        assert_eq!(ctx.name(), "context");

        let data = b"Test entry\nAnother entry\n";
        let written = ctx.write(data).unwrap();
        assert_eq!(written, data.len());
        assert_eq!(ctx.line_count(), 2);

        // Read via Channel trait
        let mut read_buf = vec![0u8; 1024];
        let read_len = ctx.read(&mut read_buf).unwrap();
        assert!(read_len > 0);
    }

    #[test]
    fn test_context_clear() {
        let mut ctx = ContextChannel::new();
        ctx.add_text("Entry 1");
        ctx.add_text("Entry 2");
        assert_eq!(ctx.line_count(), 2);

        ctx.clear();
        assert_eq!(ctx.line_count(), 0);
    }
}
