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
                disable_user_input: Some(false),
                input_examples: Some(vec![]),
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

        let unified_stream = stream.map(|event_result| {
            event_result
                .map_err(AgentError::from)
                .and_then(Self::convert_stream_event)
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

impl AnthropicProvider {
    /// Convert Claude SDK stream events to unified format
    fn convert_stream_event(event: ClaudeStreamEvent) -> Result<ProviderStreamEvent> {
        match event {
            ClaudeStreamEvent::ContentBlockDelta { delta, index, .. } => {
                if let Some(text) = delta.text() {
                    Ok(ProviderStreamEvent::TextDelta {
                        text: text.to_string(),
                        index,
                    })
                } else {
                    // Skip non-text deltas
                    Ok(ProviderStreamEvent::TextDelta {
                        text: String::new(),
                        index: 0,
                    })
                }
            }

            ClaudeStreamEvent::ContentBlockStart {
                content_block,
                index,
            } => {
                if let claude_sdk::ContentBlock::ToolUse { id, name, .. } = content_block {
                    Ok(ProviderStreamEvent::ToolCallStart {
                        id: id.clone(),
                        name: name.clone(),
                    })
                } else {
                    Ok(ProviderStreamEvent::TextDelta {
                        text: String::new(),
                        index,
                    })
                }
            }

            ClaudeStreamEvent::ContentBlockStop { index } => {
                // Block complete - no-op for now
                Ok(ProviderStreamEvent::TextDelta {
                    text: String::new(),
                    index,
                })
            }

            ClaudeStreamEvent::MessageStop => Ok(ProviderStreamEvent::Done),

            _ => {
                // Other events (ping, metadata) - ignore
                Ok(ProviderStreamEvent::TextDelta {
                    text: String::new(),
                    index: 0,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = AnthropicProvider::new("test-key", "claude-3-5-sonnet-20241022");
        assert_eq!(provider.provider_name(), "anthropic");
        assert_eq!(provider.model_name(), "claude-3-5-sonnet-20241022");
        assert!(provider.supports_streaming());
        assert_eq!(provider.max_context_tokens(), 200_000);
    }
}
