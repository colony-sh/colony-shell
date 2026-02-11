//! Streaming tool call support in SSE parser (P5-30)
//!
//! Extends the SSE parser to handle tool_use content blocks.
//! Accumulates partial tool call JSON across SSE events and
//! emits complete tool calls when done.

use serde::{Deserialize, Serialize};

/// A tool call accumulated from streaming events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamToolCall {
    pub id: String,
    pub tool_type: String,
    pub function: StreamToolFunction,
}

/// Function call details within a streaming tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamToolFunction {
    pub name: String,
    pub arguments: String,
}

/// Delta for a tool call in a streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub tool_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ToolFunctionDelta>,
}

/// Function delta within a tool call delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunctionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// Accumulated content from streaming response
#[derive(Debug, Clone)]
pub enum StreamContent {
    Text(String),
    ToolCall(StreamToolCall),
}

/// State for a tool call being accumulated
#[derive(Debug, Clone)]
struct ToolCallAccumulator {
    id: String,
    tool_type: String,
    function_name: String,
    arguments_buffer: String,
}

/// Assembles streaming SSE events into complete content blocks
///
/// Handles interleaved text and tool_use content, accumulating
/// partial tool call arguments across multiple SSE chunks.
pub struct StreamAssembler {
    text_buffer: String,
    tool_accumulators: Vec<ToolCallAccumulator>,
    completed: Vec<StreamContent>,
}

impl StreamAssembler {
    pub fn new() -> Self {
        Self {
            text_buffer: String::new(),
            tool_accumulators: Vec::new(),
            completed: Vec::new(),
        }
    }

    /// Feed a text content delta
    pub fn feed_text(&mut self, text: &str) {
        self.text_buffer.push_str(text);
    }

    /// Feed a tool call delta
    pub fn feed_tool_call_delta(&mut self, delta: &ToolCallDelta) {
        let idx = delta.index as usize;

        // Ensure accumulator exists for this index
        while self.tool_accumulators.len() <= idx {
            self.tool_accumulators.push(ToolCallAccumulator {
                id: String::new(),
                tool_type: String::new(),
                function_name: String::new(),
                arguments_buffer: String::new(),
            });
        }

        let acc = &mut self.tool_accumulators[idx];

        if let Some(ref id) = delta.id {
            acc.id = id.clone();
        }
        if let Some(ref tt) = delta.tool_type {
            acc.tool_type = tt.clone();
        }
        if let Some(ref func) = delta.function {
            if let Some(ref name) = func.name {
                acc.function_name = name.clone();
            }
            if let Some(ref args) = func.arguments {
                acc.arguments_buffer.push_str(args);
            }
        }
    }

    /// Finalize the stream: flush text and complete any pending tool calls
    pub fn finish(&mut self) -> Vec<StreamContent> {
        let mut results = std::mem::take(&mut self.completed);

        if !self.text_buffer.is_empty() {
            results.push(StreamContent::Text(std::mem::take(&mut self.text_buffer)));
        }

        for acc in self.tool_accumulators.drain(..) {
            if !acc.id.is_empty() {
                results.push(StreamContent::ToolCall(StreamToolCall {
                    id: acc.id,
                    tool_type: acc.tool_type,
                    function: StreamToolFunction {
                        name: acc.function_name,
                        arguments: acc.arguments_buffer,
                    },
                }));
            }
        }

        results
    }

    /// Flush text buffer as a completed content block (call when switching to tool content)
    pub fn flush_text(&mut self) {
        if !self.text_buffer.is_empty() {
            self.completed
                .push(StreamContent::Text(std::mem::take(&mut self.text_buffer)));
        }
    }

    /// Get the current text buffer content
    pub fn text_so_far(&self) -> &str {
        &self.text_buffer
    }

    /// Get the number of tool calls being accumulated
    pub fn pending_tool_calls(&self) -> usize {
        self.tool_accumulators.len()
    }
}

impl Default for StreamAssembler {
    fn default() -> Self {
        Self::new()
    }
}

/// Extended streaming choice delta with tool call support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedStreamDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// Extended streaming chunk with tool call support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedStreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ExtendedStreamChoice>,
}

/// Extended streaming choice with tool call support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedStreamChoice {
    pub index: u32,
    pub delta: ExtendedStreamDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_assembler_text_only() {
        let mut asm = StreamAssembler::new();
        asm.feed_text("Hello ");
        asm.feed_text("world!");

        let results = asm.finish();
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], StreamContent::Text(t) if t == "Hello world!"));
    }

    #[test]
    fn test_stream_assembler_tool_call() {
        let mut asm = StreamAssembler::new();

        asm.feed_tool_call_delta(&ToolCallDelta {
            index: 0,
            id: Some("call_123".to_string()),
            tool_type: Some("function".to_string()),
            function: Some(ToolFunctionDelta {
                name: Some("search".to_string()),
                arguments: Some("{\"q".to_string()),
            }),
        });

        asm.feed_tool_call_delta(&ToolCallDelta {
            index: 0,
            id: None,
            tool_type: None,
            function: Some(ToolFunctionDelta {
                name: None,
                arguments: Some("uery\":\"test\"}".to_string()),
            }),
        });

        let results = asm.finish();
        assert_eq!(results.len(), 1);

        match &results[0] {
            StreamContent::ToolCall(tc) => {
                assert_eq!(tc.id, "call_123");
                assert_eq!(tc.function.name, "search");
                assert_eq!(tc.function.arguments, "{\"query\":\"test\"}");
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_stream_assembler_interleaved() {
        let mut asm = StreamAssembler::new();

        // Text first
        asm.feed_text("Let me search for that.");
        asm.flush_text();

        // Then tool call
        asm.feed_tool_call_delta(&ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            tool_type: Some("function".to_string()),
            function: Some(ToolFunctionDelta {
                name: Some("search".to_string()),
                arguments: Some("{\"q\":\"test\"}".to_string()),
            }),
        });

        let results = asm.finish();
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], StreamContent::Text(_)));
        assert!(matches!(&results[1], StreamContent::ToolCall(_)));
    }

    #[test]
    fn test_stream_assembler_multiple_tool_calls() {
        let mut asm = StreamAssembler::new();

        asm.feed_tool_call_delta(&ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            tool_type: Some("function".to_string()),
            function: Some(ToolFunctionDelta {
                name: Some("read_file".to_string()),
                arguments: Some("{\"path\":\"a.txt\"}".to_string()),
            }),
        });

        asm.feed_tool_call_delta(&ToolCallDelta {
            index: 1,
            id: Some("call_2".to_string()),
            tool_type: Some("function".to_string()),
            function: Some(ToolFunctionDelta {
                name: Some("read_file".to_string()),
                arguments: Some("{\"path\":\"b.txt\"}".to_string()),
            }),
        });

        let results = asm.finish();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_tool_call_delta_serialization() {
        let delta = ToolCallDelta {
            index: 0,
            id: Some("call_123".to_string()),
            tool_type: Some("function".to_string()),
            function: Some(ToolFunctionDelta {
                name: Some("search".to_string()),
                arguments: Some("{\"q\":\"test\"}".to_string()),
            }),
        };

        let json = serde_json::to_string(&delta).unwrap();
        let parsed: ToolCallDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, Some("call_123".to_string()));
    }

    #[test]
    fn test_extended_stream_chunk_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1700000000,
            "model": "claude-3.5-sonnet",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_abc",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "{\"q\":"
                        }
                    }]
                },
                "finish_reason": null
            }]
        }"#;

        let chunk: ExtendedStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        let tool_calls = chunk.choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0].id, Some("call_abc".to_string()));
    }

    #[test]
    fn test_stream_assembler_text_so_far() {
        let mut asm = StreamAssembler::new();
        asm.feed_text("partial ");
        assert_eq!(asm.text_so_far(), "partial ");

        asm.feed_text("content");
        assert_eq!(asm.text_so_far(), "partial content");
    }

    #[test]
    fn test_stream_assembler_pending_tool_calls() {
        let mut asm = StreamAssembler::new();
        assert_eq!(asm.pending_tool_calls(), 0);

        asm.feed_tool_call_delta(&ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            tool_type: None,
            function: None,
        });
        assert_eq!(asm.pending_tool_calls(), 1);
    }
}
