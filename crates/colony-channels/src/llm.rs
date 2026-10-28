//! LLM channel (stdllm - fd 4)
//!
//! Structured channel for LLM conversation history.
//! Messages are JSON-encoded prompts and responses.

use crate::{Channel, ChannelId, InternalBuffer, STDLLM};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io;

/// Role in an LLM conversation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A single message in the LLM conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl LlmMessage {
    pub fn new(role: LlmRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            name: None,
            tool_call_id: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_tool_call_id(mut self, id: impl Into<String>) -> Self {
        self.tool_call_id = Some(id.into());
        self
    }
}

/// LLM channel for conversation history (fd 4)
pub struct LlmChannel {
    buffer: InternalBuffer,
    messages: Vec<LlmMessage>,
}

impl LlmChannel {
    pub fn new() -> Self {
        Self {
            buffer: InternalBuffer::new(1000),
            messages: Vec::new(),
        }
    }

    /// Add a structured LLM message
    pub fn add_message(&mut self, message: LlmMessage) {
        let json = serde_json::to_string(&message).expect("LlmMessage must serialize");
        self.buffer.append_line(&json);
        self.messages.push(message);
    }

    /// Get all messages
    pub fn messages(&self) -> &[LlmMessage] {
        &self.messages
    }

    /// Get messages by role
    pub fn messages_by_role(&self, role: &LlmRole) -> Vec<&LlmMessage> {
        self.messages.iter().filter(|m| &m.role == role).collect()
    }

    /// Get last N messages
    pub fn last_messages(&self, n: usize) -> &[LlmMessage] {
        let start = self.messages.len().saturating_sub(n);
        &self.messages[start..]
    }

    /// Export conversation to JSON array
    pub fn export_json(&self) -> String {
        serde_json::to_string_pretty(&self.messages).expect("messages must serialize")
    }
}

impl Default for LlmChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel for LlmChannel {
    fn id(&self) -> ChannelId {
        STDLLM
    }

    fn name(&self) -> &str {
        "llm"
    }

    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let text =
            std::str::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        for line in text.lines() {
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<LlmMessage>(line) {
                Ok(msg) => self.add_message(msg),
                Err(_) => {
                    self.add_message(LlmMessage::new(LlmRole::User, line));
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
        self.messages.clear();
    }

    fn line_count(&self) -> usize {
        self.messages.len()
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
    fn test_llm_add_message() {
        let mut ch = LlmChannel::new();
        ch.add_message(LlmMessage::new(LlmRole::User, "Hello"));
        ch.add_message(LlmMessage::new(LlmRole::Assistant, "Hi there"));

        assert_eq!(ch.line_count(), 2);
        assert_eq!(ch.messages()[0].role, LlmRole::User);
        assert_eq!(ch.messages()[1].role, LlmRole::Assistant);
    }

    #[test]
    fn test_llm_messages_by_role() {
        let mut ch = LlmChannel::new();
        ch.add_message(LlmMessage::new(LlmRole::System, "You are helpful"));
        ch.add_message(LlmMessage::new(LlmRole::User, "Hello"));
        ch.add_message(LlmMessage::new(LlmRole::Assistant, "Hi"));
        ch.add_message(LlmMessage::new(LlmRole::User, "How are you?"));

        let user_msgs = ch.messages_by_role(&LlmRole::User);
        assert_eq!(user_msgs.len(), 2);
    }

    #[test]
    fn test_llm_last_messages() {
        let mut ch = LlmChannel::new();
        for i in 0..5 {
            ch.add_message(LlmMessage::new(LlmRole::User, format!("msg {}", i)));
        }

        let last = ch.last_messages(3);
        assert_eq!(last.len(), 3);
        assert_eq!(last[0].content, "msg 2");
        assert_eq!(last[2].content, "msg 4");
    }

    #[test]
    fn test_llm_write_json() {
        let mut ch = LlmChannel::new();
        let msg = LlmMessage::new(LlmRole::User, "test");
        let json = serde_json::to_string(&msg).unwrap();

        let written = ch.write(json.as_bytes()).unwrap();
        assert_eq!(written, json.len());
        assert_eq!(ch.messages().len(), 1);
        assert_eq!(ch.messages()[0].content, "test");
    }

    #[test]
    fn test_llm_write_plain_text() {
        let mut ch = LlmChannel::new();
        let written = ch.write(b"plain text message").unwrap();
        assert!(written > 0);
        assert_eq!(ch.messages().len(), 1);
        assert_eq!(ch.messages()[0].role, LlmRole::User);
        assert_eq!(ch.messages()[0].content, "plain text message");
    }

    #[test]
    fn test_llm_channel_trait() {
        let ch = LlmChannel::new();
        assert_eq!(ch.id(), STDLLM);
        assert_eq!(ch.name(), "llm");
    }

    #[test]
    fn test_llm_export_json() {
        let mut ch = LlmChannel::new();
        ch.add_message(LlmMessage::new(LlmRole::User, "Hello"));

        let json = ch.export_json();
        let parsed: Vec<LlmMessage> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].content, "Hello");
    }

    #[test]
    fn test_llm_clear() {
        let mut ch = LlmChannel::new();
        ch.add_message(LlmMessage::new(LlmRole::User, "Hello"));
        assert_eq!(ch.line_count(), 1);

        ch.clear();
        assert_eq!(ch.line_count(), 0);
        assert!(ch.messages().is_empty());
    }

    #[test]
    fn test_llm_message_with_metadata() {
        let msg = LlmMessage::new(LlmRole::Tool, "result data")
            .with_name("search")
            .with_tool_call_id("call_123");

        assert_eq!(msg.name.as_deref(), Some("search"));
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_123"));
    }
}
