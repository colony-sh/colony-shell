//! Colony Cloud provider
//!
//! Provides access to Colony Cloud's managed LLM service.
//! Uses the colony-cloud-sdk for typed API access.
//! The API follows OpenAI's format for compatibility.

use crate::provider::{
    CompletionRequest, CompletionStream, ModelProvider, ProviderHealth,
    StreamEvent as ProviderStreamEvent,
};
use crate::{AgentError, Result};
use async_trait::async_trait;
use colony_cloud_sdk::{ChatFunction, ChatMessage, ChatRequest, ChatTool, ColonyClient};
use futures::stream;
use futures::stream::StreamExt;
use tracing::debug;

/// Colony Cloud provider using the colony-cloud-sdk
pub struct CloudProvider {
    client: ColonyClient,
    model: String,
}

impl CloudProvider {
    /// Create provider from a Colony Cloud client
    pub fn new(client: ColonyClient, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
        }
    }

    /// Create provider from COLONY_API_KEY env var
    pub fn from_env(model: impl Into<String>) -> Result<Self> {
        let client = ColonyClient::from_env()
            .map_err(|e| AgentError::Protocol(format!("Colony Cloud auth failed: {}", e)))?;
        Ok(Self::new(client, model))
    }

    fn build_chat_request(&self, request: &CompletionRequest) -> ChatRequest {
        let mut messages: Vec<ChatMessage> = Vec::new();

        if let Some(ref system) = request.system_prompt {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: system.clone(),
            });
        }

        for msg in &request.messages {
            let role = format!("{:?}", msg.role).to_lowercase();
            let content = match &msg.content {
                crate::provider::MessageContent::Text(t) => t.clone(),
                crate::provider::MessageContent::Blocks(_) => String::new(),
            };
            messages.push(ChatMessage { role, content });
        }

        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(
                request
                    .tools
                    .iter()
                    .map(|t| ChatTool {
                        tool_type: "function".to_string(),
                        function: ChatFunction {
                            name: t.name.clone(),
                            description: Some(t.description.clone()),
                            parameters: t.input_schema.clone(),
                        },
                    })
                    .collect(),
            )
        };

        ChatRequest {
            model: self.model.clone(),
            messages,
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
            stream: None,
            tools,
            cache_system_prompt: if request.enable_caching {
                Some(true)
            } else {
                None
            },
        }
    }
}

#[async_trait]
impl ModelProvider for CloudProvider {
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream> {
        debug!("Colony Cloud completing with model: {}", self.model);

        let chat_request = self.build_chat_request(&request);

        let sse_stream = self
            .client
            .chat_stream(&chat_request)
            .await
            .map_err(|e| AgentError::Protocol(format!("Colony Cloud stream error: {}", e)))?;

        let mapped = stream::unfold(sse_stream, |mut sse| async move {
            loop {
                let chunk_result = sse.next().await?;

                match chunk_result {
                    Ok(chunk) => {
                        for choice in &chunk.choices {
                            if let Some(ref content) = choice.delta.content {
                                let event = ProviderStreamEvent::TextDelta {
                                    text: content.clone(),
                                    index: choice.index as usize,
                                };
                                return Some((Ok(event), sse));
                            }
                            if choice.finish_reason.is_some() {
                                return Some((Ok(ProviderStreamEvent::Done), sse));
                            }
                        }
                    }
                    Err(e) => {
                        return Some((
                            Err(AgentError::Protocol(format!("Stream chunk error: {}", e))),
                            sse,
                        ));
                    }
                }
            }
        });

        Ok(Box::pin(mapped))
    }

    fn max_context_tokens(&self) -> usize {
        200_000
    }

    fn provider_name(&self) -> &str {
        "colony"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn health_check(&self) -> Result<ProviderHealth> {
        let start = std::time::Instant::now();
        match self.client.health().await {
            Ok(true) => Ok(ProviderHealth {
                available: true,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: None,
            }),
            Ok(false) => Ok(ProviderHealth {
                available: false,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: Some("Colony Cloud API unreachable".to_string()),
            }),
            Err(e) => Ok(ProviderHealth {
                available: false,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(e.to_string()),
            }),
        }
    }
}
