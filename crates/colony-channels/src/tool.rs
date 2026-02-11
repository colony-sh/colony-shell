//! Tool channel (stdtool - fd 5)
//!
//! Structured channel for tool invocations and results.
//! Uses a request/response protocol with correlation IDs.

use crate::{Channel, ChannelId, InternalBuffer, STDTOOL};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;

/// A tool invocation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// A tool invocation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: String,
    pub request_id: String,
    pub output: serde_json::Value,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}

/// A tool call entry (request + optional result)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    pub request: ToolRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ToolResult>,
}

/// Message type for the tool channel wire format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolMessage {
    #[serde(rename = "request")]
    Request(ToolRequest),
    #[serde(rename = "result")]
    Result(ToolResult),
}

/// Tool channel for tool call logging (fd 5)
pub struct ToolChannel {
    buffer: InternalBuffer,
    entries: Vec<ToolEntry>,
    pending: HashMap<String, usize>,
}

impl ToolChannel {
    pub fn new() -> Self {
        Self {
            buffer: InternalBuffer::new(1000),
            entries: Vec::new(),
            pending: HashMap::new(),
        }
    }

    /// Record a tool request
    pub fn record_request(&mut self, request: ToolRequest) {
        let json = serde_json::to_string(&ToolMessage::Request(request.clone()))
            .expect("ToolMessage must serialize");
        self.buffer.append_line(&json);

        let idx = self.entries.len();
        self.pending.insert(request.id.clone(), idx);
        self.entries.push(ToolEntry {
            request,
            result: None,
        });
    }

    /// Record a tool result, correlating with its request
    pub fn record_result(&mut self, result: ToolResult) {
        let json = serde_json::to_string(&ToolMessage::Result(result.clone()))
            .expect("ToolMessage must serialize");
        self.buffer.append_line(&json);

        if let Some(idx) = self.pending.remove(&result.request_id) {
            if let Some(entry) = self.entries.get_mut(idx) {
                entry.result = Some(result);
            }
        }
    }

    /// Get all tool entries
    pub fn entries(&self) -> &[ToolEntry] {
        &self.entries
    }

    /// Get pending (unanswered) requests
    pub fn pending_requests(&self) -> Vec<&ToolRequest> {
        self.pending
            .values()
            .filter_map(|idx| self.entries.get(*idx))
            .map(|e| &e.request)
            .collect()
    }

    /// Get entries by tool name
    pub fn entries_by_tool(&self, name: &str) -> Vec<&ToolEntry> {
        self.entries
            .iter()
            .filter(|e| e.request.tool_name == name)
            .collect()
    }

    /// Export all entries to JSON
    pub fn export_json(&self) -> String {
        serde_json::to_string_pretty(&self.entries).expect("entries must serialize")
    }
}

impl Default for ToolChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel for ToolChannel {
    fn id(&self) -> ChannelId {
        STDTOOL
    }

    fn name(&self) -> &str {
        "tool"
    }

    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let text = std::str::from_utf8(data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        for line in text.lines() {
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<ToolMessage>(line) {
                Ok(ToolMessage::Request(req)) => self.record_request(req),
                Ok(ToolMessage::Result(res)) => self.record_result(res),
                Err(e) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid tool message JSON: {}", e),
                    ));
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
        self.pending.clear();
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

    fn make_request(id: &str, name: &str) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_name: name.to_string(),
            arguments: serde_json::json!({"query": "test"}),
            timestamp: Utc::now(),
        }
    }

    fn make_result(id: &str, request_id: &str, success: bool) -> ToolResult {
        ToolResult {
            id: id.to_string(),
            request_id: request_id.to_string(),
            output: serde_json::json!({"data": "result"}),
            success,
            error: if success {
                None
            } else {
                Some("failed".to_string())
            },
            duration_ms: 150,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_tool_request_response() {
        let mut ch = ToolChannel::new();

        ch.record_request(make_request("req1", "search"));
        assert_eq!(ch.entries().len(), 1);
        assert_eq!(ch.pending_requests().len(), 1);

        ch.record_result(make_result("res1", "req1", true));
        assert_eq!(ch.entries().len(), 1);
        assert_eq!(ch.pending_requests().len(), 0);
        assert!(ch.entries()[0].result.is_some());
    }

    #[test]
    fn test_tool_entries_by_name() {
        let mut ch = ToolChannel::new();

        ch.record_request(make_request("r1", "search"));
        ch.record_request(make_request("r2", "read_file"));
        ch.record_request(make_request("r3", "search"));

        let search_entries = ch.entries_by_tool("search");
        assert_eq!(search_entries.len(), 2);

        let file_entries = ch.entries_by_tool("read_file");
        assert_eq!(file_entries.len(), 1);
    }

    #[test]
    fn test_tool_write_json() {
        let mut ch = ToolChannel::new();
        let req = make_request("r1", "test_tool");
        let msg = ToolMessage::Request(req);
        let json = serde_json::to_string(&msg).unwrap();

        let written = ch.write(json.as_bytes()).unwrap();
        assert_eq!(written, json.len());
        assert_eq!(ch.entries().len(), 1);
    }

    #[test]
    fn test_tool_write_invalid_json() {
        let mut ch = ToolChannel::new();
        let result = ch.write(b"not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_channel_trait() {
        let ch = ToolChannel::new();
        assert_eq!(ch.id(), STDTOOL);
        assert_eq!(ch.name(), "tool");
    }

    #[test]
    fn test_tool_clear() {
        let mut ch = ToolChannel::new();
        ch.record_request(make_request("r1", "search"));
        assert_eq!(ch.entries().len(), 1);

        ch.clear();
        assert_eq!(ch.entries().len(), 0);
        assert!(ch.pending_requests().is_empty());
    }

    #[test]
    fn test_tool_export_json() {
        let mut ch = ToolChannel::new();
        ch.record_request(make_request("r1", "search"));
        ch.record_result(make_result("res1", "r1", true));

        let json = ch.export_json();
        let parsed: Vec<ToolEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].result.is_some());
    }

    #[test]
    fn test_tool_multiple_pending() {
        let mut ch = ToolChannel::new();
        ch.record_request(make_request("r1", "search"));
        ch.record_request(make_request("r2", "read"));
        ch.record_request(make_request("r3", "write"));

        assert_eq!(ch.pending_requests().len(), 3);

        ch.record_result(make_result("res1", "r2", true));
        assert_eq!(ch.pending_requests().len(), 2);
    }
}
