//! Internal buffer for channel content storage
//!
//! Phase 1: Simple line-based storage
//! Phase 2 (F025): Full Buffer with styling, ANSI, scrollback

use chrono::{DateTime, Utc};

/// Minimal buffer for Phase 1
///
/// Stores lines of text. Full styling and ANSI support in F025.
pub struct InternalBuffer {
    lines: Vec<BufferLine>,
    max_lines: usize,
}

/// A single line in the buffer
#[derive(Clone, Debug)]
pub struct BufferLine {
    pub text: String,
    pub timestamp: DateTime<Utc>,
}

impl InternalBuffer {
    /// Create new buffer with max line limit
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: Vec::new(),
            max_lines,
        }
    }

    /// Append a line of text
    pub fn append_line(&mut self, text: impl Into<String>) {
        let line = BufferLine {
            text: text.into(),
            timestamp: Utc::now(),
        };

        self.lines.push(line);

        // Enforce max lines
        while self.lines.len() > self.max_lines {
            self.lines.remove(0);
        }
    }

    /// Get all lines
    pub fn lines(&self) -> &[BufferLine] {
        &self.lines
    }

    /// Get line count
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Clear all lines
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Get last N lines
    pub fn last_lines(&self, n: usize) -> &[BufferLine] {
        let start = self.lines.len().saturating_sub(n);
        &self.lines[start..]
    }

    /// Export to plain text
    pub fn to_plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for InternalBuffer {
    fn default() -> Self {
        Self::new(1000) // Default 1000 line scrollback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_append() {
        let mut buffer = InternalBuffer::new(10);
        buffer.append_line("line 1");
        buffer.append_line("line 2");

        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.lines()[0].text, "line 1");
        assert_eq!(buffer.lines()[1].text, "line 2");
    }

    #[test]
    fn test_buffer_max_lines() {
        let mut buffer = InternalBuffer::new(3);
        buffer.append_line("1");
        buffer.append_line("2");
        buffer.append_line("3");
        buffer.append_line("4"); // Should evict "1"

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.lines()[0].text, "2");
        assert_eq!(buffer.lines()[2].text, "4");
    }

    #[test]
    fn test_buffer_clear() {
        let mut buffer = InternalBuffer::new(10);
        buffer.append_line("test");
        assert_eq!(buffer.len(), 1);

        buffer.clear();
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_to_plain_text() {
        let mut buffer = InternalBuffer::new(10);
        buffer.append_line("first");
        buffer.append_line("second");

        let text = buffer.to_plain_text();
        assert_eq!(text, "first\nsecond");
    }
}
