//! OpenAI GPT provider
//!
//! Provides access to GPT-4, GPT-3.5, and other OpenAI models via
//! the ChatCompletions API. Supports streaming and function calling.

use crate::provider::{
    CompletionRequest, CompletionStream, ModelProvider, ProviderHealth,
    StreamEvent as ProviderStreamEvent,
};
use crate::{AgentError, Result};
use async_trait::async_trait;
use futures::stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// OpenAI provider
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
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream> {
        debug!("OpenAI provider completing with model: {}", self.model);

        // Build OpenAI request
        let openai_request = OpenAIRequest {
            model: self.model.clone(),
            messages: request
                .messages
                .iter()
                .map(|m| OpenAIMessage {
                    role: format!("{:?}", m.role).to_lowercase(),
                    content: match &m.content {
                        crate::provider::MessageContent::Text(t) => t.clone(),
                        crate::provider::MessageContent::Blocks(_) => {
                            "".to_string() // TODO: Handle blocks
                        }
                    },
                })
                .collect(),
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
            stream: Some(true),
            tools: if request.tools.is_empty() {
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
            },
        };

        // Send request (non-streaming for initial implementation)
        // TODO: Implement actual streaming via SSE
        let response: OpenAIResponse = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| AgentError::Protocol(format!("OpenAI request failed: {}", e)))?
            .json()
            .await
            .map_err(|e| AgentError::Protocol(format!("OpenAI response parse failed: {}", e)))?;

        // Convert to stream (single event for now)
        let text = response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let events = vec![
            Ok(ProviderStreamEvent::TextDelta { text, index: 0 }),
            Ok(ProviderStreamEvent::Done),
        ];

        Ok(Box::pin(stream::iter(events)))
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
        // Ping the models endpoint
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

// OpenAI API types
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
    content: String,
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

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}
