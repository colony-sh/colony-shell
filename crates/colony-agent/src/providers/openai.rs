//! OpenAI GPT provider with SSE streaming
//!
//! Provides access to GPT-4, GPT-3.5, and other OpenAI models via
//! the ChatCompletions API. Supports real-time streaming via Server-Sent
//! Events (SSE) and incremental tool call assembly.
//!
//! ## Streaming format
//!
//! OpenAI's `/v1/chat/completions` with `stream: true` returns SSE:
//! ```text
//! data: {"id":"...","choices":[{"delta":{"content":"Hello"},"index":0}]}
//! data: {"id":"...","choices":[{"delta":{"content":" world"},"index":0}]}
//! data: [DONE]
//! ```

use crate::provider::{
    CompletionRequest, CompletionStream, ModelProvider, ProviderHealth,
    StreamEvent as ProviderStreamEvent,
};
use crate::{AgentError, Result};
use async_trait::async_trait;
use futures::stream;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// OpenAI provider with SSE streaming support
pub struct OpenAIProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAIProvider {
    /// Create provider with API key
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    /// Use custom base URL (for Azure OpenAI, etc.)
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Build the messages array for the OpenAI request, prepending any system prompt
    fn build_messages(request: &CompletionRequest) -> Vec<OpenAIMessage> {
        let mut messages: Vec<OpenAIMessage> = Vec::new();

        if let Some(ref system) = request.system_prompt {
            messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: Some(system.clone()),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        for msg in &request.messages {
            messages.push(OpenAIMessage {
                role: format!("{:?}", msg.role).to_lowercase(),
                content: Some(match &msg.content {
                    crate::provider::MessageContent::Text(t) => t.clone(),
                    crate::provider::MessageContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b {
                            crate::provider::ContentBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                }),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        messages
    }

    /// Build the tools array for the OpenAI request
    fn build_tools(request: &CompletionRequest) -> Option<Vec<OpenAITool>> {
        if request.tools.is_empty() {
            None
        } else {
            Some(
                request
                    .tools
                    .iter()
                    .map(|t| OpenAITool {
                        tool_type: "function".to_string(),
                        function: OpenAIFunction {
                            name: t.name.clone(),
                            description: Some(t.description.clone()),
                            parameters: t.input_schema.clone(),
                        },
                    })
                    .collect(),
            )
        }
    }
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream> {
        debug!("OpenAI provider streaming with model: {}", self.model);

        let openai_request = OpenAIRequest {
            model: self.model.clone(),
            messages: Self::build_messages(&request),
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
            stream: Some(true),
            tools: Self::build_tools(&request),
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Accept", "text/event-stream")
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| AgentError::Protocol(format!("OpenAI request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(AgentError::Protocol(format!(
                "OpenAI API error HTTP {status}: {body}"
            )));
        }

        let sse_stream = parse_openai_sse_stream(response);
        Ok(Box::pin(sse_stream))
    }

    fn max_context_tokens(&self) -> usize {
        // GPT-4 Turbo: 128k context
        128_000
    }

    fn provider_name(&self) -> &str {
        "openai"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn health_check(&self) -> Result<ProviderHealth> {
        let start = std::time::Instant::now();

        let response = self
            .client
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => Ok(ProviderHealth {
                available: true,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: None,
            }),
            Ok(resp) => Ok(ProviderHealth {
                available: false,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("HTTP {}", resp.status())),
            }),
            Err(e) => Ok(ProviderHealth {
                available: false,
                latency_ms: None,
                error: Some(e.to_string()),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// SSE stream parsing
// ---------------------------------------------------------------------------

/// State for accumulating tool calls across multiple SSE chunks.
///
/// OpenAI sends tool calls incrementally: the first chunk for an index
/// carries the `id` and function `name`, while subsequent chunks append
/// to the `arguments` string.
#[derive(Debug, Default)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments: String,
}

/// Internal state carried through the `stream::unfold` loop.
struct SseState<S> {
    inner: S,
    buffer: String,
    tool_accumulators: Vec<ToolCallAccumulator>,
    /// Track whether we have emitted ToolCallStart for each index
    started: Vec<bool>,
}

/// Parse an SSE byte stream from OpenAI into a stream of `ProviderStreamEvent`.
///
/// Handles:
/// - `data: {...}` lines with text deltas (`choices[0].delta.content`)
/// - `data: {...}` lines with tool call deltas (`choices[0].delta.tool_calls`)
/// - `data: [DONE]` termination sentinel
/// - Empty lines and other SSE fields (ignored)
/// - Partial TCP chunks (buffered until a full line is available)
fn parse_openai_sse_stream(
    response: reqwest::Response,
) -> impl futures::Stream<Item = Result<ProviderStreamEvent>> + Send {
    let byte_stream = response.bytes_stream();

    stream::unfold(
        SseState {
            inner: Box::pin(byte_stream),
            buffer: String::new(),
            tool_accumulators: Vec::new(),
            started: Vec::new(),
        },
        |mut state| async move {
            loop {
                // Try to extract a complete line from the buffer
                while let Some(line_end) = state.buffer.find('\n') {
                    let line = state.buffer[..line_end]
                        .trim_end_matches('\r')
                        .to_string();
                    state.buffer = state.buffer[line_end + 1..].to_string();

                    // Skip empty lines (SSE event boundaries)
                    if line.is_empty() {
                        continue;
                    }

                    // Only process data: lines
                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };

                    // Termination sentinel
                    if data == "[DONE]" {
                        // Emit ToolCallComplete for any pending tool calls
                        if let Some(event) = flush_tool_calls(&mut state) {
                            return Some((Ok(event), state));
                        }
                        return Some((Ok(ProviderStreamEvent::Done), state));
                    }

                    // Parse the chunk JSON
                    let chunk: OpenAIStreamChunk = match serde_json::from_str(data) {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Failed to parse OpenAI SSE chunk: {e}");
                            continue;
                        }
                    };

                    let Some(choice) = chunk.choices.first() else {
                        continue;
                    };

                    // Handle text content delta
                    if let Some(ref content) = choice.delta.content {
                        if !content.is_empty() {
                            return Some((
                                Ok(ProviderStreamEvent::TextDelta {
                                    text: content.clone(),
                                    index: choice.index as usize,
                                }),
                                state,
                            ));
                        }
                    }

                    // Handle tool call deltas
                    if let Some(ref tool_calls) = choice.delta.tool_calls {
                        for tc_delta in tool_calls {
                            let idx = tc_delta.index as usize;

                            // Grow accumulators as needed
                            while state.tool_accumulators.len() <= idx {
                                state.tool_accumulators.push(ToolCallAccumulator::default());
                                state.started.push(false);
                            }

                            let acc = &mut state.tool_accumulators[idx];

                            if let Some(ref id) = tc_delta.id {
                                acc.id.clone_from(id);
                            }
                            if let Some(ref func) = tc_delta.function {
                                if let Some(ref name) = func.name {
                                    acc.name.clone_from(name);
                                }
                                if let Some(ref args) = func.arguments {
                                    acc.arguments.push_str(args);
                                }
                            }

                            // Emit ToolCallStart the first time we see each index
                            if !state.started[idx] && !acc.name.is_empty() {
                                state.started[idx] = true;
                                return Some((
                                    Ok(ProviderStreamEvent::ToolCallStart {
                                        id: acc.id.clone(),
                                        name: acc.name.clone(),
                                    }),
                                    state,
                                ));
                            }
                        }
                    }

                    // Handle finish_reason: "tool_calls" -- flush pending accumulators
                    if choice.finish_reason.as_deref() == Some("tool_calls") {
                        if let Some(event) = flush_tool_calls(&mut state) {
                            return Some((Ok(event), state));
                        }
                    }

                    // Handle finish_reason: "stop"
                    if choice.finish_reason.as_deref() == Some("stop") {
                        return Some((Ok(ProviderStreamEvent::Done), state));
                    }
                }

                // Need more data from the byte stream
                match state.inner.next().await {
                    Some(Ok(bytes)) => match std::str::from_utf8(&bytes) {
                        Ok(text) => state.buffer.push_str(text),
                        Err(e) => {
                            return Some((
                                Err(AgentError::Protocol(format!(
                                    "OpenAI SSE: invalid UTF-8: {e}"
                                ))),
                                state,
                            ));
                        }
                    },
                    Some(Err(e)) => {
                        return Some((
                            Err(AgentError::Protocol(format!("OpenAI SSE read error: {e}"))),
                            state,
                        ));
                    }
                    None => {
                        // Stream ended without [DONE] -- emit Done anyway
                        return None;
                    }
                }
            }
        },
    )
}

/// Flush the first pending tool call accumulator into a `ToolCallComplete` event.
///
/// Called when `finish_reason == "tool_calls"` or at `[DONE]`. Returns `None`
/// when all accumulators have already been flushed.
fn flush_tool_calls<S>(state: &mut SseState<S>) -> Option<ProviderStreamEvent> {
    let pos = state
        .tool_accumulators
        .iter()
        .position(|a| !a.id.is_empty());

    let Some(idx) = pos else {
        return None;
    };

    let acc = state.tool_accumulators.remove(idx);
    if idx < state.started.len() {
        state.started.remove(idx);
    }

    let input: serde_json::Value = serde_json::from_str(&acc.arguments)
        .unwrap_or(serde_json::Value::Object(Default::default()));

    Some(ProviderStreamEvent::ToolCallComplete {
        id: acc.id,
        name: acc.name,
        input,
    })
}

// ---------------------------------------------------------------------------
// OpenAI API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: serde_json::Value,
}

/// Non-streaming response (kept for health checks or fallback)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAIChoice {
    message: OpenAIChoiceMessage,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAIChoiceMessage {
    role: String,
    #[serde(default)]
    content: String,
}

// --- Streaming response types ---

/// A single SSE chunk from OpenAI streaming
#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    #[allow(dead_code)]
    id: String,
    choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    index: u32,
    delta: OpenAIStreamDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamDelta {
    #[allow(dead_code)]
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallDelta {
    index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function: Option<OpenAIFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunctionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    #[test]
    fn test_provider_creation() {
        let provider = OpenAIProvider::new("test-key", "gpt-4o");
        assert_eq!(provider.provider_name(), "openai");
        assert_eq!(provider.model_name(), "gpt-4o");
        assert!(provider.supports_streaming());
        assert_eq!(provider.max_context_tokens(), 128_000);
    }

    #[test]
    fn test_custom_base_url() {
        let provider =
            OpenAIProvider::new("key", "gpt-4o").with_base_url("https://custom.api.com/v1");
        assert_eq!(provider.base_url, "https://custom.api.com/v1");
    }

    #[test]
    fn test_parse_stream_chunk_text() {
        let json = r#"{
            "id": "chatcmpl-abc",
            "object": "chat.completion.chunk",
            "created": 1700000000,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": {"content": "Hello"},
                "finish_reason": null
            }]
        }"#;

        let chunk: OpenAIStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    #[test]
    fn test_parse_stream_chunk_tool_call() {
        let json = r#"{
            "id": "chatcmpl-abc",
            "object": "chat.completion.chunk",
            "created": 1700000000,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"loc"
                        }
                    }]
                },
                "finish_reason": null
            }]
        }"#;

        let chunk: OpenAIStreamChunk = serde_json::from_str(json).unwrap();
        let tc = chunk.choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(tc[0].id.as_deref(), Some("call_abc123"));
        assert_eq!(
            tc[0].function.as_ref().unwrap().name.as_deref(),
            Some("get_weather")
        );
    }

    #[test]
    fn test_parse_stream_chunk_tool_call_continuation() {
        let json = r#"{
            "id": "chatcmpl-abc",
            "object": "chat.completion.chunk",
            "created": 1700000000,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": {
                            "arguments": "ation\":\"NYC\"}"
                        }
                    }]
                },
                "finish_reason": null
            }]
        }"#;

        let chunk: OpenAIStreamChunk = serde_json::from_str(json).unwrap();
        let tc = chunk.choices[0].delta.tool_calls.as_ref().unwrap();
        assert!(tc[0].id.is_none());
        assert_eq!(
            tc[0].function.as_ref().unwrap().arguments.as_deref(),
            Some("ation\":\"NYC\"}")
        );
    }

    #[test]
    fn test_parse_stream_chunk_finish_stop() {
        let json = r#"{
            "id": "chatcmpl-abc",
            "object": "chat.completion.chunk",
            "created": 1700000000,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": "stop"
            }]
        }"#;

        let chunk: OpenAIStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
    }

    /// Test SSE line parsing by simulating the byte stream
    #[tokio::test]
    async fn test_sse_text_stream() {
        let sse_body = concat!(
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );

        let byte_stream =
            futures::stream::once(async move { Ok(bytes::Bytes::from(sse_body)) });

        let response = build_mock_response(byte_stream);
        let mut event_stream = Box::pin(parse_openai_sse_stream(response));

        let mut texts = Vec::new();
        let mut done = false;

        while let Some(event) = event_stream.next().await {
            match event.unwrap() {
                ProviderStreamEvent::TextDelta { text, .. } => texts.push(text),
                ProviderStreamEvent::Done => {
                    done = true;
                    break;
                }
                _ => {}
            }
        }

        assert_eq!(texts.join(""), "Hello world");
        assert!(done);
    }

    /// Test that tool call deltas are accumulated and emitted correctly
    #[tokio::test]
    async fn test_sse_tool_call_stream() {
        let sse_body = concat!(
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"search\",\"arguments\":\"{\\\"q\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\":\\\"rust\\\"\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );

        let byte_stream =
            futures::stream::once(async move { Ok(bytes::Bytes::from(sse_body)) });
        let response = build_mock_response(byte_stream);
        let mut event_stream = Box::pin(parse_openai_sse_stream(response));

        let mut saw_start = false;
        let mut saw_complete = false;

        while let Some(event) = event_stream.next().await {
            match event.unwrap() {
                ProviderStreamEvent::ToolCallStart { id, name } => {
                    assert_eq!(id, "call_1");
                    assert_eq!(name, "search");
                    saw_start = true;
                }
                ProviderStreamEvent::ToolCallComplete { id, name, input } => {
                    assert_eq!(id, "call_1");
                    assert_eq!(name, "search");
                    assert_eq!(input, serde_json::json!({"q": "rust"}));
                    saw_complete = true;
                }
                _ => {}
            }
        }

        assert!(saw_start, "should have emitted ToolCallStart");
        assert!(saw_complete, "should have emitted ToolCallComplete");
    }

    /// Test that partial TCP chunks are handled (data split mid-line)
    #[tokio::test]
    async fn test_sse_partial_chunks() {
        // Split a single SSE event across two TCP chunks
        let chunk1 = "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"con";
        let chunk2 = "tent\":\"Hi\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n";

        let byte_stream = futures::stream::iter(vec![
            Ok(bytes::Bytes::from(chunk1)),
            Ok(bytes::Bytes::from(chunk2)),
        ]);

        let response = build_mock_response(byte_stream);
        let mut event_stream = Box::pin(parse_openai_sse_stream(response));

        let mut texts = Vec::new();
        while let Some(event) = event_stream.next().await {
            if let Ok(ProviderStreamEvent::TextDelta { text, .. }) = event {
                texts.push(text);
            }
        }

        assert_eq!(texts.join(""), "Hi");
    }

    #[test]
    fn test_build_messages_with_system() {
        let request = CompletionRequest::new()
            .with_system("You are helpful.")
            .add_user_message("Hello");

        let messages = OpenAIProvider::build_messages(&request);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content.as_deref(), Some("You are helpful."));
        assert_eq!(messages[1].role, "user");
    }

    #[test]
    fn test_build_messages_without_system() {
        let request = CompletionRequest::new().add_user_message("Hello");

        let messages = OpenAIProvider::build_messages(&request);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Build a mock reqwest::Response from a bytes stream for testing SSE parsing.
    ///
    /// Uses http::Response to construct a response with the given body stream.
    fn build_mock_response<S>(byte_stream: S) -> reqwest::Response
    where
        S: futures::Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>>
            + Send
            + 'static,
    {
        let body = reqwest::Body::wrap_stream(byte_stream);
        let http_resp = http::Response::builder()
            .status(200)
            .header("content-type", "text/event-stream")
            .body(body)
            .expect("failed to build mock response");
        reqwest::Response::from(http_resp)
    }
}
