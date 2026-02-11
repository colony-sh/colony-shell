//! Line-based styled text buffer
//!
//! `StyledBuffer` stores lines of `StyledSpan` sequences and provides
//! append, clear, and line access operations.

use super::span::StyledSpan;
use serde::{Deserialize, Serialize};

/// A single line in the buffer, composed of styled spans
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferLine {
    pub spans: Vec<StyledSpan>,
}

impl BufferLine {
    /// Create an empty line
    pub fn new() -> Self {
        Self { spans: Vec::new() }
    }

    /// Create a line from a single plain text string
    pub fn plain(text: &str) -> Self {
        Self {
            spans: vec![StyledSpan::plain(text)],
        }
    }

    /// Get the plain text content (spans concatenated)
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Total character count
    pub fn len(&self) -> usize {
        self.spans.iter().map(|s| s.len()).sum()
    }

    /// Whether the line has no content
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty() || self.spans.iter().all(StyledSpan::is_empty)
    }

    /// Append a span to this line
    pub fn push(&mut self, span: StyledSpan) {
        self.spans.push(span);
    }
}

impl Default for BufferLine {
    fn default() -> Self {
        Self::new()
    }
}

/// A buffer of styled text lines with optional max capacity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyledBuffer {
    lines: Vec<BufferLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_lines: Option<usize>,
}

impl StyledBuffer {
    /// Create an empty buffer with no line limit
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            max_lines: None,
        }
    }

    /// Create a buffer with a maximum scrollback capacity
    pub fn with_capacity(max_lines: usize) -> Self {
        Self {
            lines: Vec::with_capacity(max_lines.min(1024)),
            max_lines: Some(max_lines),
        }
    }

    /// Append a styled line to the buffer
    pub fn push_line(&mut self, line: BufferLine) {
        self.lines.push(line);
        self.trim_to_capacity();
    }

    /// Append plain text, splitting on newlines
    pub fn push_text(&mut self, text: &str) {
        for line_text in text.split('\n') {
            self.push_line(BufferLine::plain(line_text));
        }
    }

    /// Get a line by index
    pub fn line(&self, index: usize) -> Option<&BufferLine> {
        self.lines.get(index)
    }

    /// Total number of lines
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Whether the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Get all lines
    pub fn lines(&self) -> &[BufferLine] {
        &self.lines
    }

    /// Get a range of lines
    pub fn lines_range(&self, start: usize, count: usize) -> &[BufferLine] {
        let end = (start + count).min(self.lines.len());
        let start = start.min(self.lines.len());
        &self.lines[start..end]
    }

    /// Clear all lines
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Get plain text content of the entire buffer
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn trim_to_capacity(&mut self) {
        let Some(max) = self.max_lines else {
            return;
        };
        if self.lines.len() > max {
            let excess = self.lines.len() - max;
            self.lines.drain(..excess);
        }
    }
}

impl Default for StyledBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffers::span::Color;

    #[test]
    fn test_buffer_line_plain() {
        let line = BufferLine::plain("hello world");
        assert_eq!(line.text(), "hello world");
        assert_eq!(line.len(), 11);
        assert!(!line.is_empty());
    }

    #[test]
    fn test_buffer_line_empty() {
        let line = BufferLine::new();
        assert!(line.is_empty());
        assert_eq!(line.len(), 0);
    }

    #[test]
    fn test_buffer_line_push() {
        let mut line = BufferLine::new();
        line.push(StyledSpan::colored("red", Color::Red));
        line.push(StyledSpan::plain(" text"));
        assert_eq!(line.text(), "red text");
        assert_eq!(line.spans.len(), 2);
    }

    #[test]
    fn test_styled_buffer_push_text() {
        let mut buf = StyledBuffer::new();
        buf.push_text("line 1\nline 2\nline 3");
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.line(0).unwrap().text(), "line 1");
        assert_eq!(buf.line(2).unwrap().text(), "line 3");
    }

    #[test]
    fn test_styled_buffer_capacity() {
        let mut buf = StyledBuffer::with_capacity(3);
        buf.push_text("a\nb\nc\nd\ne");
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.line(0).unwrap().text(), "c");
        assert_eq!(buf.line(2).unwrap().text(), "e");
    }

    #[test]
    fn test_styled_buffer_clear() {
        let mut buf = StyledBuffer::new();
        buf.push_text("hello\nworld");
        assert_eq!(buf.line_count(), 2);
        buf.clear();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_styled_buffer_text() {
        let mut buf = StyledBuffer::new();
        buf.push_text("hello\nworld");
        assert_eq!(buf.text(), "hello\nworld");
    }

    #[test]
    fn test_lines_range() {
        let mut buf = StyledBuffer::new();
        buf.push_text("a\nb\nc\nd\ne");
        let range = buf.lines_range(1, 3);
        assert_eq!(range.len(), 3);
        assert_eq!(range[0].text(), "b");
        assert_eq!(range[2].text(), "d");
    }

    #[test]
    fn test_lines_range_clamped() {
        let mut buf = StyledBuffer::new();
        buf.push_text("a\nb");
        let range = buf.lines_range(0, 100);
        assert_eq!(range.len(), 2);
    }

    #[test]
    fn test_buffer_serialization() {
        let mut buf = StyledBuffer::new();
        buf.push_line(BufferLine::plain("test line"));
        let json = serde_json::to_string(&buf).unwrap();
        assert!(json.contains("test line"));

        let deserialized: StyledBuffer = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.line_count(), 1);
        assert_eq!(deserialized.line(0).unwrap().text(), "test line");
    }
}
