//! Anthropic Claude provider
//!
//! Wraps our native Rust claude-sdk to provide Claude models via the
//! ModelProvider trait. Supports Claude 3.5 Sonnet, Opus, Haiku, and
//! future models.

use crate::provider::{
    CompletionRequest, CompletionStream, ContentBlock, MessageContent, MessageRole, ModelProvider,
    ProviderHealth, StreamEvent as ProviderStreamEvent, ToolDefinition,
};
use crate::{AgentError, Result};
use async_trait::async_trait;
use claude_sdk::{ClaudeClient, ConversationBuilder, StreamEvent as ClaudeStreamEvent};
use futures::StreamExt;
use tracing::debug;

/// Anthropic provider using claude-sdk
pub struct AnthropicProvider {
    client: ClaudeClient,
    model: String,
    enable_caching: bool,
}

impl AnthropicProvider {
    /// Create provider with API key
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: ClaudeClient::anthropic(api_key),
            model: model.into(),
            enable_caching: false,
        }
    }

    /// Enable prompt caching for system prompts
    pub fn with_caching(mut self, enable: bool) -> Self {
        self.enable_caching = enable;
        self
    }

    /// Convert our unified request to Claude SDK format
    fn build_conversation(&self, request: &CompletionRequest) -> ConversationBuilder {
        let mut builder = ConversationBuilder::new();

        // System prompt
        if let Some(ref system) = request.system_prompt {
            builder = if request.enable_caching {
                builder.with_cached_system(system)
            } else {
                builder.with_system(system)
            };
        }

        // Convert messages
        for msg in &request.messages {
            match msg.role {
                MessageRole::User => {
                    let text = match &msg.content {
                        MessageContent::Text(t) => t.clone(),
                        MessageContent::Blocks(blocks) => {
                            // Extract text from blocks
                            blocks
                                .iter()
                                .filter_map(|b| match b {
                                    ContentBlock::Text { text } => Some(text.clone()),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        }
                    };
                    builder.add_user_message(&text);
                }
                MessageRole::Assistant => {
                    let text = match &msg.content {
                        MessageContent::Text(t) => t.clone(),
                        MessageContent::Blocks(_) => continue, // Skip tool blocks for now
                    };
                    builder.add_assistant_message(&text);
                }
                MessageRole::System => {
                    // System messages handled separately
                }
            }
        }

        builder
    }

    /// Convert Claude SDK tools to our format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<claude_sdk::Tool> {
        tools
            .iter()
            .map(|t| claude_sdk::Tool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
                cache_control: None,
                // The Messages API refuses fields it does not know on a custom tool
                // ("Extra inputs are not permitted"); None keeps them out of the JSON.
                disable_user_input: None,
                input_examples: None,
            })
            .collect()
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream> {
        debug!("Anthropic provider completing with model: {}", self.model);

        let conversation = self.build_conversation(&request);
        let tools = self.convert_tools(&request.tools);
        let sdk_request = conversation
            .with_tools(tools)
            .build(&self.model, request.max_tokens);
        let stream = self.client.send_streaming(sdk_request).await?;

        let unified_stream = stream.scan(ToolCalls::default(), |calls, event_result| {
            let event = event_result
                .map_err(AgentError::from)
                .and_then(|event| calls.convert(event));
            futures::future::ready(Some(event))
        });

        Ok(Box::pin(unified_stream))
    }

    fn max_context_tokens(&self) -> usize {
        // Claude 3.5 Sonnet: 200k context
        200_000
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn health_check(&self) -> Result<ProviderHealth> {
        // For now, assume healthy if we have an API key
        // TODO: Make actual API health check request
        Ok(ProviderHealth {
            available: true,
            latency_ms: None,
            error: None,
        })
    }
}

/// Tool calls being streamed, by content-block index. A tool's input arrives as pieces of
/// JSON (`input_json_delta`) between its block's start and stop; the call is complete, and
/// reported as [`ProviderStreamEvent::ToolCallComplete`], only at the stop.
#[derive(Default)]
struct ToolCalls {
    open: std::collections::HashMap<usize, (String, String, String)>,
}

impl ToolCalls {
    /// Convert one SDK stream event. Events with nothing to report become an empty text delta.
    fn convert(&mut self, event: ClaudeStreamEvent) -> Result<ProviderStreamEvent> {
        let nothing = |index| ProviderStreamEvent::TextDelta {
            text: String::new(),
            index,
        };
        match event {
            ClaudeStreamEvent::ContentBlockStart {
                content_block: claude_sdk::ContentBlock::ToolUse { id, name, .. },
                index,
            } => {
                self.open
                    .insert(index, (id.clone(), name.clone(), String::new()));
                Ok(ProviderStreamEvent::ToolCallStart { id, name })
            }
            ClaudeStreamEvent::ContentBlockDelta { delta, index } => {
                if let Some(text) = delta.text() {
                    return Ok(ProviderStreamEvent::TextDelta {
                        text: text.to_string(),
                        index,
                    });
                }
                if let (Some(json), Some((_, _, input))) =
                    (delta.partial_json(), self.open.get_mut(&index))
                {
                    input.push_str(json);
                }
                // Thinking and signature deltas are not shown.
                Ok(nothing(index))
            }
            ClaudeStreamEvent::ContentBlockStop { index } => {
                let Some((id, name, input)) = self.open.remove(&index) else {
                    return Ok(nothing(index));
                };
                // A tool with no parameters streams no JSON at all.
                let input = if input.trim().is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str(&input).map_err(|e| {
                        AgentError::Protocol(format!("tool {name}: input is not JSON: {e}"))
                    })?
                };
                Ok(ProviderStreamEvent::ToolCallComplete { id, name, input })
            }
            ClaudeStreamEvent::MessageStop => Ok(ProviderStreamEvent::Done),
            ClaudeStreamEvent::Error { error } => Ok(ProviderStreamEvent::Error {
                message: format!("{error:?}"),
            }),
            _ => Ok(nothing(0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sse(json: serde_json::Value) -> ClaudeStreamEvent {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn a_streamed_tool_call_completes_with_its_assembled_input() {
        let mut calls = ToolCalls::default();
        let events = [
            sse(
                serde_json::json!({"type": "content_block_start", "index": 1,
                "content_block": {"type": "tool_use", "id": "t1", "name": "file_read", "input": {}}}),
            ),
            sse(
                serde_json::json!({"type": "content_block_delta", "index": 1,
                "delta": {"type": "input_json_delta", "partial_json": "{\"path\": \"REA"}}),
            ),
            sse(
                serde_json::json!({"type": "content_block_delta", "index": 1,
                "delta": {"type": "input_json_delta", "partial_json": "DME.md\"}"}}),
            ),
            sse(serde_json::json!({"type": "content_block_stop", "index": 1})),
        ];
        let out: Vec<_> = events
            .into_iter()
            .map(|e| calls.convert(e).unwrap())
            .collect();
        assert!(
            matches!(&out[0], ProviderStreamEvent::ToolCallStart { name, .. } if name == "file_read")
        );
        let ProviderStreamEvent::ToolCallComplete { id, name, input } = &out[3] else {
            panic!("no completed call: {out:?}");
        };
        assert_eq!((id.as_str(), name.as_str()), ("t1", "file_read"));
        assert_eq!(input, &serde_json::json!({"path": "README.md"}));
    }

    #[test]
    fn a_tool_call_with_no_input_completes_with_an_empty_object() {
        let mut calls = ToolCalls::default();
        calls
            .convert(sse(
                serde_json::json!({"type": "content_block_start", "index": 0,
                "content_block": {"type": "tool_use", "id": "t", "name": "search", "input": {}}}),
            ))
            .unwrap();
        let done = calls
            .convert(sse(
                serde_json::json!({"type": "content_block_stop", "index": 0}),
            ))
            .unwrap();
        assert!(
            matches!(done, ProviderStreamEvent::ToolCallComplete { input, .. } if input == serde_json::json!({}))
        );
    }

    #[test]
    fn text_blocks_still_stream_as_text() {
        let mut calls = ToolCalls::default();
        let ev = calls
            .convert(sse(
                serde_json::json!({"type": "content_block_delta", "index": 0,
                "delta": {"type": "text_delta", "text": "PASS: ok"}}),
            ))
            .unwrap();
        assert!(matches!(ev, ProviderStreamEvent::TextDelta { text, .. } if text == "PASS: ok"));
        let stop = calls
            .convert(sse(
                serde_json::json!({"type": "content_block_stop", "index": 0}),
            ))
            .unwrap();
        assert!(matches!(stop, ProviderStreamEvent::TextDelta { text, .. } if text.is_empty()));
    }

    #[test]
    fn tools_carry_only_fields_the_messages_api_accepts() {
        let provider = AnthropicProvider::new("k", "m");
        let tools = provider.convert_tools(&[ToolDefinition {
            name: "file_read".into(),
            description: "read".into(),
            input_schema: serde_json::json!({"type": "object"}),
        }]);
        let json = serde_json::to_value(&tools[0]).unwrap();
        let mut keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["description", "input_schema", "name"]);
    }

    #[test]
    fn test_provider_creation() {
        let provider = AnthropicProvider::new("test-key", "claude-3-5-sonnet-20241022");
        assert_eq!(provider.provider_name(), "anthropic");
        assert_eq!(provider.model_name(), "claude-3-5-sonnet-20241022");
        assert!(provider.supports_streaming());
        assert_eq!(provider.max_context_tokens(), 200_000);
    }
}
