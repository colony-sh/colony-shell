//! Ollama local model provider
//!
//! FREE FALLBACK: Ollama runs models locally at no cost.
//! Supports Llama 3, Mistral, CodeLlama, and 100+ other open models.
//!
//! Installation: brew install ollama (macOS) or see ollama.com
//! Start server: ollama serve
//! Pull model: ollama pull llama3

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

/// Ollama provider for local models
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

    /// Check if Ollama server is running
    pub async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
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
            .map_err(|e| AgentError::Protocol(format!("Ollama request failed: {}", e)))?
            .json()
            .await
            .map_err(|e| AgentError::Protocol(format!("Ollama response parse failed: {}", e)))?;

        Ok(response.models.into_iter().map(|m| m.name).collect())
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream> {
        debug!("Ollama provider completing with model: {}", self.model);

        // Build Ollama request
        let ollama_request = OllamaRequest {
            model: self.model.clone(),
            messages: request
                .messages
                .iter()
                .map(|m| OllamaMessage {
                    role: format!("{:?}", m.role).to_lowercase(),
                    content: match &m.content {
                        crate::provider::MessageContent::Text(t) => t.clone(),
                        crate::provider::MessageContent::Blocks(_) => {
                            "".to_string() // TODO: Handle blocks
                        }
                    },
                })
                .collect(),
            stream: Some(false), // Non-streaming for initial implementation
            options: Some(OllamaOptions {
                temperature: request.temperature,
                ..Default::default()
            }),
        };

        // Add system prompt to messages if present
        let mut messages = ollama_request.messages;
        if let Some(system) = request.system_prompt {
            messages.insert(
                0,
                OllamaMessage {
                    role: "system".to_string(),
                    content: system,
                },
            );
        }

        let ollama_request = OllamaRequest {
            messages,
            ..ollama_request
        };

        // Send request
        let response: OllamaResponse = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| AgentError::Protocol(format!("Ollama request failed: {}", e)))?
            .json()
            .await
            .map_err(|e| AgentError::Protocol(format!("Ollama response parse failed: {}", e)))?;

        // Convert to stream (single event for now)
        let text = response.message.content;

        let events = vec![
            Ok(ProviderStreamEvent::TextDelta { text, index: 0 }),
            Ok(ProviderStreamEvent::Done),
        ];

        Ok(Box::pin(stream::iter(events)))
    }

    fn supports_streaming(&self) -> bool {
        true // Ollama supports streaming via newline-delimited JSON
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
                error: Some(format!("Ollama not running. Install: brew install ollama, Start: ollama serve. Error: {}", e)),
            }),
        }
    }
}

// Ollama API types
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

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
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

    #[test]
    fn test_provider_creation() {
        let provider = OllamaProvider::new("llama3");
        assert_eq!(provider.provider_name(), "ollama");
        assert_eq!(provider.model_name(), "llama3");
        assert!(provider.supports_streaming());
    }

    #[tokio::test]
    async fn test_health_check_when_not_running() {
        let provider = OllamaProvider::new("llama3");
        let health = provider.health_check().await.unwrap();
        // When Ollama isn't running, this will fail - that's expected
        if !health.available {
            assert!(health.error.is_some());
        }
    }
}
