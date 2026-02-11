//! Diagnostic channel (stddiag - fd 6)
//!
//! Structured channel for diagnostic and logging data.
//! Supports severity levels and structured log entries.

use crate::{Channel, ChannelId, InternalBuffer, STDDIAG};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io;

/// Diagnostic severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// A structured diagnostic entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagEntry {
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

impl DiagEntry {
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            message: message.into(),
            source: None,
            code: None,
            metadata: serde_json::Value::Null,
            timestamp: Utc::now(),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Diagnostic channel for structured logging (fd 6)
pub struct DiagChannel {
    buffer: InternalBuffer,
    entries: Vec<DiagEntry>,
    min_severity: Severity,
}

impl DiagChannel {
    pub fn new() -> Self {
        Self {
            buffer: InternalBuffer::new(5000),
            entries: Vec::new(),
            min_severity: Severity::Trace,
        }
    }

    /// Set minimum severity level (entries below this are dropped)
    pub fn set_min_severity(&mut self, level: Severity) {
        self.min_severity = level;
    }

    /// Add a diagnostic entry
    pub fn add_entry(&mut self, entry: DiagEntry) {
        if entry.severity < self.min_severity {
            return;
        }

        let json = serde_json::to_string(&entry).expect("DiagEntry must serialize");
        self.buffer.append_line(&json);
        self.entries.push(entry);
    }

    /// Convenience: log a trace message
    pub fn trace(&mut self, message: impl Into<String>) {
        self.add_entry(DiagEntry::new(Severity::Trace, message));
    }

    /// Convenience: log a debug message
    pub fn debug(&mut self, message: impl Into<String>) {
        self.add_entry(DiagEntry::new(Severity::Debug, message));
    }

    /// Convenience: log an info message
    pub fn info(&mut self, message: impl Into<String>) {
        self.add_entry(DiagEntry::new(Severity::Info, message));
    }

    /// Convenience: log a warning
    pub fn warn(&mut self, message: impl Into<String>) {
        self.add_entry(DiagEntry::new(Severity::Warn, message));
    }

    /// Convenience: log an error
    pub fn error(&mut self, message: impl Into<String>) {
        self.add_entry(DiagEntry::new(Severity::Error, message));
    }

    /// Get all entries
    pub fn entries(&self) -> &[DiagEntry] {
        &self.entries
    }

    /// Get entries at or above a severity level
    pub fn entries_at_level(&self, min: Severity) -> Vec<&DiagEntry> {
        self.entries.iter().filter(|e| e.severity >= min).collect()
    }

    /// Get entries from a specific source
    pub fn entries_from_source(&self, source: &str) -> Vec<&DiagEntry> {
        self.entries
            .iter()
            .filter(|e| e.source.as_deref() == Some(source))
            .collect()
    }

    /// Export entries to JSON array
    pub fn export_json(&self) -> String {
        serde_json::to_string_pretty(&self.entries).expect("entries must serialize")
    }
}

impl Default for DiagChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel for DiagChannel {
    fn id(&self) -> ChannelId {
        STDDIAG
    }

    fn name(&self) -> &str {
        "diag"
    }

    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let text = std::str::from_utf8(data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        for line in text.lines() {
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<DiagEntry>(line) {
                Ok(entry) => self.add_entry(entry),
                Err(_) => {
                    self.add_entry(DiagEntry::new(Severity::Info, line));
                }
            }
        }

        Ok(data.len())
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let text = self.export_json();
        let bytes = text.as_bytes();
        let len = buf.len().min(bytes.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.entries.clear();
    }

    fn line_count(&self) -> usize {
        self.entries.len()
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
    fn test_diag_severity_ordering() {
        assert!(Severity::Trace < Severity::Debug);
        assert!(Severity::Debug < Severity::Info);
        assert!(Severity::Info < Severity::Warn);
        assert!(Severity::Warn < Severity::Error);
    }

    #[test]
    fn test_diag_add_entries() {
        let mut ch = DiagChannel::new();
        ch.trace("trace msg");
        ch.debug("debug msg");
        ch.info("info msg");
        ch.warn("warn msg");
        ch.error("error msg");

        assert_eq!(ch.entries().len(), 5);
        assert_eq!(ch.entries()[0].severity, Severity::Trace);
        assert_eq!(ch.entries()[4].severity, Severity::Error);
    }

    #[test]
    fn test_diag_min_severity_filter() {
        let mut ch = DiagChannel::new();
        ch.set_min_severity(Severity::Warn);

        ch.trace("dropped");
        ch.debug("dropped");
        ch.info("dropped");
        ch.warn("kept");
        ch.error("kept");

        assert_eq!(ch.entries().len(), 2);
        assert_eq!(ch.entries()[0].severity, Severity::Warn);
        assert_eq!(ch.entries()[1].severity, Severity::Error);
    }

    #[test]
    fn test_diag_entries_at_level() {
        let mut ch = DiagChannel::new();
        ch.trace("t");
        ch.info("i");
        ch.warn("w");
        ch.error("e");

        let warnings_up = ch.entries_at_level(Severity::Warn);
        assert_eq!(warnings_up.len(), 2);
    }

    #[test]
    fn test_diag_entries_from_source() {
        let mut ch = DiagChannel::new();
        ch.add_entry(DiagEntry::new(Severity::Info, "from parser").with_source("parser"));
        ch.add_entry(DiagEntry::new(Severity::Info, "from executor").with_source("executor"));
        ch.add_entry(DiagEntry::new(Severity::Warn, "from parser again").with_source("parser"));

        let parser_entries = ch.entries_from_source("parser");
        assert_eq!(parser_entries.len(), 2);
    }

    #[test]
    fn test_diag_entry_with_metadata() {
        let entry = DiagEntry::new(Severity::Error, "test error")
            .with_source("test")
            .with_code("E001")
            .with_metadata(serde_json::json!({"line": 42}));

        assert_eq!(entry.source.as_deref(), Some("test"));
        assert_eq!(entry.code.as_deref(), Some("E001"));
        assert_eq!(entry.metadata["line"], 42);
    }

    #[test]
    fn test_diag_write_json() {
        let mut ch = DiagChannel::new();
        let entry = DiagEntry::new(Severity::Info, "structured");
        let json = serde_json::to_string(&entry).unwrap();

        let written = ch.write(json.as_bytes()).unwrap();
        assert_eq!(written, json.len());
        assert_eq!(ch.entries().len(), 1);
        assert_eq!(ch.entries()[0].message, "structured");
    }

    #[test]
    fn test_diag_write_plain_text() {
        let mut ch = DiagChannel::new();
        ch.write(b"plain text diagnostic").unwrap();

        assert_eq!(ch.entries().len(), 1);
        assert_eq!(ch.entries()[0].severity, Severity::Info);
        assert_eq!(ch.entries()[0].message, "plain text diagnostic");
    }

    #[test]
    fn test_diag_channel_trait() {
        let ch = DiagChannel::new();
        assert_eq!(ch.id(), STDDIAG);
        assert_eq!(ch.name(), "diag");
    }

    #[test]
    fn test_diag_clear() {
        let mut ch = DiagChannel::new();
        ch.info("test");
        assert_eq!(ch.line_count(), 1);

        ch.clear();
        assert_eq!(ch.line_count(), 0);
        assert!(ch.entries().is_empty());
    }

    #[test]
    fn test_diag_export_json() {
        let mut ch = DiagChannel::new();
        ch.info("hello");
        ch.warn("world");

        let json = ch.export_json();
        let parsed: Vec<DiagEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 2);
    }
}
