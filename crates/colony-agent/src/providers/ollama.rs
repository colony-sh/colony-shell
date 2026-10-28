//! Ollama local model provider with NDJSON streaming
//!
//! FREE FALLBACK: Ollama runs models locally at no cost.
//! Supports Llama 3, Mistral, CodeLlama, and 100+ other open models.
//!
//! Installation: brew install ollama (macOS) or see ollama.com
//! Start server: ollama serve
//! Pull model: ollama pull llama3
//!
//! ## Streaming format
//!
//! Ollama's `/api/chat` with `stream: true` returns newline-delimited JSON:
//! ```text
//! {"message":{"role":"assistant","content":"Hello"},"done":false}
//! {"message":{"role":"assistant","content":" world"},"done":false}
//! {"message":{"role":"assistant","content":""},"done":true}
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

/// Ollama provider for local models with NDJSON streaming
pub struct OllamaProvider {
    client: Client,
    model: String,
    base_url: String,
}

impl OllamaProvider {
    /// Create provider with model name
    ///
    /// Default base_url: http://localhost:11434
    /// Common models: llama3, mistral, codellama, phi3
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            model: model.into(),
            base_url: "http://localhost:11434".to_string(),
        }
    }

    /// Use custom Ollama server URL
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// List available models
    pub async fn list_models(&self) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct ModelList {
            models: Vec<ModelInfo>,
        }

        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let response: ModelList = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| AgentError::Protocol(format!("Ollama request failed: {e}")))?
            .json()
            .await
            .map_err(|e| AgentError::Protocol(format!("Ollama response parse failed: {e}")))?;

        Ok(response.models.into_iter().map(|m| m.name).collect())
    }

    /// Build the messages array, prepending system prompt if present
    fn build_messages(request: &CompletionRequest) -> Vec<OllamaMessage> {
        let mut messages: Vec<OllamaMessage> = Vec::new();

        if let Some(ref system) = request.system_prompt {
            messages.push(OllamaMessage {
                role: "system".to_string(),
                content: system.clone(),
            });
        }

        for msg in &request.messages {
            messages.push(OllamaMessage {
                role: format!("{:?}", msg.role).to_lowercase(),
                content: match &msg.content {
                    crate::provider::MessageContent::Text(t) => t.clone(),
                    crate::provider::MessageContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b {
                            crate::provider::ContentBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                },
            });
        }

        messages
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream> {
        debug!("Ollama provider streaming with model: {}", self.model);

        let ollama_request = OllamaRequest {
            model: self.model.clone(),
            messages: Self::build_messages(&request),
            stream: Some(true),
            options: Some(OllamaOptions {
                temperature: request.temperature,
            }),
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| AgentError::Protocol(format!("Ollama request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(AgentError::Protocol(format!(
                "Ollama API error HTTP {status}: {body}"
            )));
        }

        let ndjson_stream = parse_ollama_ndjson_stream(response);
        Ok(Box::pin(ndjson_stream))
    }

    fn max_context_tokens(&self) -> usize {
        // Most Ollama models: 4k-32k context
        // Llama 3: 8k default, 32k extended
        32_000
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn health_check(&self) -> Result<ProviderHealth> {
        let start = std::time::Instant::now();

        // Check if Ollama server is running
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                // Check if model is pulled
                if let Ok(models) = self.list_models().await {
                    let model_available = models.iter().any(|m| m.starts_with(&self.model));

                    if model_available {
                        Ok(ProviderHealth {
                            available: true,
                            latency_ms: Some(start.elapsed().as_millis() as u64),
                            error: None,
                        })
                    } else {
                        Ok(ProviderHealth {
                            available: false,
                            latency_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(format!(
                                "Model '{}' not found. Run: ollama pull {}",
                                self.model, self.model
                            )),
                        })
                    }
                } else {
                    Ok(ProviderHealth {
                        available: false,
                        latency_ms: Some(start.elapsed().as_millis() as u64),
                        error: Some("Failed to list models".to_string()),
                    })
                }
            }
            Ok(resp) => Ok(ProviderHealth {
                available: false,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("Ollama server error: HTTP {}", resp.status())),
            }),
            Err(e) => Ok(ProviderHealth {
                available: false,
                latency_ms: None,
                error: Some(format!(
                    "Ollama not running. Install: brew install ollama, Start: ollama serve. Error: {e}"
                )),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// NDJSON stream parsing
// ---------------------------------------------------------------------------

/// Internal state for the NDJSON unfold loop.
struct NdjsonState<S> {
    inner: S,
    buffer: String,
}

/// Parse an NDJSON byte stream from Ollama into `ProviderStreamEvent` values.
///
/// Each line is a complete JSON object. The stream ends when a line
/// has `"done": true`.
///
/// Handles partial TCP chunks by buffering until a complete newline-terminated
/// line is available.
fn parse_ollama_ndjson_stream(
    response: reqwest::Response,
) -> impl futures::Stream<Item = Result<ProviderStreamEvent>> + Send {
    let byte_stream = response.bytes_stream();

    stream::unfold(
        NdjsonState {
            inner: Box::pin(byte_stream),
            buffer: String::new(),
        },
        |mut state| async move {
            loop {
                // Try to extract a complete line from the buffer
                while let Some(line_end) = state.buffer.find('\n') {
                    let line = state.buffer[..line_end].trim_end_matches('\r').to_string();
                    state.buffer = state.buffer[line_end + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    let chunk: OllamaStreamChunk = match serde_json::from_str(&line) {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Failed to parse Ollama NDJSON line: {e}");
                            continue;
                        }
                    };

                    // Terminal chunk
                    if chunk.done {
                        return Some((Ok(ProviderStreamEvent::Done), state));
                    }

                    // Emit text delta
                    let content = chunk.message.content;
                    if !content.is_empty() {
                        return Some((
                            Ok(ProviderStreamEvent::TextDelta {
                                text: content,
                                index: 0,
                            }),
                            state,
                        ));
                    }
                }

                // Need more data from the byte stream
                match state.inner.next().await {
                    Some(Ok(bytes)) => match std::str::from_utf8(&bytes) {
                        Ok(text) => state.buffer.push_str(text),
                        Err(e) => {
                            return Some((
                                Err(AgentError::Protocol(format!(
                                    "Ollama NDJSON: invalid UTF-8: {e}"
                                ))),
                                state,
                            ));
                        }
                    },
                    Some(Err(e)) => {
                        return Some((
                            Err(AgentError::Protocol(format!(
                                "Ollama NDJSON read error: {e}"
                            ))),
                            state,
                        ));
                    }
                    None => {
                        // Stream ended without done:true
                        return None;
                    }
                }
            }
        },
    )
}

// ---------------------------------------------------------------------------
// Ollama API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize, Default)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

/// Non-streaming response (kept for compatibility)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaResponse {
    message: OllamaMessage,
}

/// A single NDJSON line from Ollama streaming
#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    message: OllamaMessage,
    done: bool,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OllamaModelList {
    models: Vec<OllamaModelInfo>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OllamaModelInfo {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    #[test]
    fn test_provider_creation() {
        let provider = OllamaProvider::new("llama3");
        assert_eq!(provider.provider_name(), "ollama");
        assert_eq!(provider.model_name(), "llama3");
        assert!(provider.supports_streaming());
    }

    #[test]
    fn test_custom_base_url() {
        let provider = OllamaProvider::new("llama3").with_base_url("http://remote-gpu:11434");
        assert_eq!(provider.base_url, "http://remote-gpu:11434");
    }

    #[test]
    fn test_parse_ndjson_chunk() {
        let json = r#"{"message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let chunk: OllamaStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.message.content, "Hello");
        assert!(!chunk.done);
    }

    #[test]
    fn test_parse_ndjson_done_chunk() {
        let json = r#"{"message":{"role":"assistant","content":""},"done":true}"#;
        let chunk: OllamaStreamChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.done);
        assert!(chunk.message.content.is_empty());
    }

    /// Test NDJSON parsing by simulating a byte stream
    #[tokio::test]
    async fn test_ndjson_text_stream() {
        let ndjson_body = concat!(
            "{\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\" world\"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"!\"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true}\n",
        );

        let byte_stream = futures::stream::once(async move { Ok(bytes::Bytes::from(ndjson_body)) });
        let response = build_mock_response(byte_stream);
        let mut event_stream = Box::pin(parse_ollama_ndjson_stream(response));

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

        assert_eq!(texts.join(""), "Hello world!");
        assert!(done);
    }

    /// Test that partial TCP chunks are handled correctly
    #[tokio::test]
    async fn test_ndjson_partial_chunks() {
        // Split a single NDJSON line across two TCP chunks
        let chunk1 = "{\"message\":{\"role\":\"assistant\",\"con";
        let chunk2 = concat!(
            "tent\":\"Hi\"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true}\n",
        );

        let byte_stream = futures::stream::iter(vec![
            Ok(bytes::Bytes::from(chunk1)),
            Ok(bytes::Bytes::from(chunk2)),
        ]);

        let response = build_mock_response(byte_stream);
        let mut event_stream = Box::pin(parse_ollama_ndjson_stream(response));

        let mut texts = Vec::new();
        while let Some(event) = event_stream.next().await {
            if let Ok(ProviderStreamEvent::TextDelta { text, .. }) = event {
                texts.push(text);
            }
        }

        assert_eq!(texts.join(""), "Hi");
    }

    /// Test multiple NDJSON lines delivered one per chunk
    #[tokio::test]
    async fn test_ndjson_one_line_per_chunk() {
        let lines = vec![
            Ok(bytes::Bytes::from(
                "{\"message\":{\"role\":\"assistant\",\"content\":\"A\"},\"done\":false}\n",
            )),
            Ok(bytes::Bytes::from(
                "{\"message\":{\"role\":\"assistant\",\"content\":\"B\"},\"done\":false}\n",
            )),
            Ok(bytes::Bytes::from(
                "{\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true}\n",
            )),
        ];

        let byte_stream = futures::stream::iter(lines);
        let response = build_mock_response(byte_stream);
        let mut event_stream = Box::pin(parse_ollama_ndjson_stream(response));

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

        assert_eq!(texts, vec!["A", "B"]);
        assert!(done);
    }

    #[test]
    fn test_build_messages_with_system() {
        let request = CompletionRequest::new()
            .with_system("Be helpful.")
            .add_user_message("Hi");

        let messages = OllamaProvider::build_messages(&request);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, "Be helpful.");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, "Hi");
    }

    #[test]
    fn test_build_messages_without_system() {
        let request = CompletionRequest::new().add_user_message("Hello");

        let messages = OllamaProvider::build_messages(&request);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
    }

    #[tokio::test]
    async fn test_health_check_when_not_running() {
        let provider = OllamaProvider::new("llama3");
        let health = provider.health_check().await.unwrap();
        // When Ollama is not running, this will fail -- that is expected
        if !health.available {
            assert!(health.error.is_some());
        }
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Build a mock reqwest::Response from a bytes stream for testing NDJSON parsing.
    fn build_mock_response<S>(byte_stream: S) -> reqwest::Response
    where
        S: futures::Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>>
            + Send
            + 'static,
    {
        let body = reqwest::Body::wrap_stream(byte_stream);
        let http_resp = http::Response::builder()
            .status(200)
            .header("content-type", "application/x-ndjson")
            .body(body)
            .expect("failed to build mock response");
        reqwest::Response::from(http_resp)
    }
}
