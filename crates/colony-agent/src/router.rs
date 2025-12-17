//! Model router with auto-detection and provider selection
//!
//! The ModelRouter abstracts provider selection and provides:
//! - Auto-detection: Try Anthropic → OpenAI → Ollama (free fallback)
//! - Environment variables: COLONY_MODEL_PROVIDER, COLONY_MODEL_NAME
//! - Runtime switching: `llm use anthropic:claude-3.5-sonnet`
//! - Health checks: Verify API keys and model availability

use crate::provider::{CompletionRequest, CompletionStream, ModelProvider, ProviderHealth};
use crate::providers::{AnthropicProvider, OllamaProvider, OpenAIProvider};
use crate::Result;
use async_trait::async_trait;
use std::env;
use tracing::{info, warn};

/// Model router with provider auto-detection
pub struct ModelRouter {
    provider: Box<dyn ModelProvider>,
    config: RouterConfig,
}

/// Router configuration
#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub provider_type: ProviderType,
    pub model_name: String,
}

/// Supported provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderType {
    Anthropic,
    OpenAI,
    Ollama,
}

impl ProviderType {
    /// Parse from string (case-insensitive)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "anthropic" | "claude" => Some(Self::Anthropic),
            "openai" | "gpt" => Some(Self::OpenAI),
            "ollama" | "local" => Some(Self::Ollama),
            _ => None,
        }
    }

    /// Get default model for this provider
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Anthropic => "claude-3-5-sonnet-20241022",
            Self::OpenAI => "gpt-4-turbo",
            Self::Ollama => "llama3",
        }
    }

    /// Get environment variable name for API key
    pub fn api_key_env_var(&self) -> Option<&'static str> {
        match self {
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::OpenAI => Some("OPENAI_API_KEY"),
            Self::Ollama => None, // Local, no API key needed
        }
    }
}

impl ModelRouter {
    /// Create router with auto-detected provider
    ///
    /// Detection order:
    /// 1. Check COLONY_MODEL_PROVIDER env var
    /// 2. Try Anthropic (if ANTHROPIC_API_KEY set)
    /// 3. Try OpenAI (if OPENAI_API_KEY set)
    /// 4. Fall back to Ollama (free local)
    pub async fn auto_detect() -> Result<Self> {
        info!("Auto-detecting available model provider...");

        // Check explicit provider env var
        if let Ok(provider_str) = env::var("COLONY_MODEL_PROVIDER") {
            if let Some(provider_type) = ProviderType::from_str(&provider_str) {
                info!("Using provider from COLONY_MODEL_PROVIDER: {:?}", provider_type);
                let model_name = env::var("COLONY_MODEL_NAME")
                    .unwrap_or_else(|_| provider_type.default_model().to_string());

                return Self::from_config(RouterConfig {
                    provider_type,
                    model_name,
                })
                .await;
            }
        }

        // Try Anthropic
        if let Ok(api_key) = env::var("ANTHROPIC_API_KEY") {
            info!("Found ANTHROPIC_API_KEY, using Anthropic provider");
            let model = env::var("COLONY_MODEL_NAME")
                .unwrap_or_else(|_| ProviderType::Anthropic.default_model().to_string());

            let provider = AnthropicProvider::new(api_key, &model).with_caching(true);

            return Ok(Self {
                provider: Box::new(provider),
                config: RouterConfig {
                    provider_type: ProviderType::Anthropic,
                    model_name: model,
                },
            });
        }

        // Try OpenAI
        if let Ok(api_key) = env::var("OPENAI_API_KEY") {
            info!("Found OPENAI_API_KEY, using OpenAI provider");
            let model = env::var("COLONY_MODEL_NAME")
                .unwrap_or_else(|_| ProviderType::OpenAI.default_model().to_string());

            let provider = OpenAIProvider::new(api_key, &model);

            return Ok(Self {
                provider: Box::new(provider),
                config: RouterConfig {
                    provider_type: ProviderType::OpenAI,
                    model_name: model,
                },
            });
        }

        // Fall back to Ollama (free local)
        warn!("No API keys found, falling back to Ollama (local, free)");
        warn!("Install: brew install ollama (macOS) or visit ollama.com");
        warn!("Start: ollama serve");
        warn!("Pull model: ollama pull llama3");

        let model = env::var("COLONY_MODEL_NAME")
            .unwrap_or_else(|_| ProviderType::Ollama.default_model().to_string());

        let provider = OllamaProvider::new(&model);

        Ok(Self {
            provider: Box::new(provider),
            config: RouterConfig {
                provider_type: ProviderType::Ollama,
                model_name: model,
            },
        })
    }

    /// Create router from explicit configuration
    pub async fn from_config(config: RouterConfig) -> Result<Self> {
        let provider: Box<dyn ModelProvider> = match config.provider_type {
            ProviderType::Anthropic => {
                let api_key = env::var("ANTHROPIC_API_KEY").map_err(|_| {
                    crate::AgentError::Protocol(
                        "ANTHROPIC_API_KEY not set. Get key at console.anthropic.com".to_string(),
                    )
                })?;

                Box::new(
                    AnthropicProvider::new(api_key, &config.model_name).with_caching(true),
                )
            }

            ProviderType::OpenAI => {
                let api_key = env::var("OPENAI_API_KEY").map_err(|_| {
                    crate::AgentError::Protocol(
                        "OPENAI_API_KEY not set. Get key at platform.openai.com".to_string(),
                    )
                })?;

                Box::new(OpenAIProvider::new(api_key, &config.model_name))
            }

            ProviderType::Ollama => Box::new(OllamaProvider::new(&config.model_name)),
        };

        Ok(Self { provider, config })
    }

    /// Switch to different provider at runtime
    pub async fn switch_provider(&mut self, new_config: RouterConfig) -> Result<()> {
        info!(
            "Switching from {:?} to {:?}",
            self.config.provider_type, new_config.provider_type
        );

        let new_router = Self::from_config(new_config.clone()).await?;
        self.provider = new_router.provider;
        self.config = new_config;

        Ok(())
    }

    /// Get current provider info
    pub fn current_provider(&self) -> &ProviderType {
        &self.config.provider_type
    }

    /// Get current model name
    pub fn current_model(&self) -> &str {
        &self.config.model_name
    }

    /// Get provider health status
    pub async fn health(&self) -> Result<ProviderHealth> {
        self.provider.health_check().await
    }

    /// List available providers (based on API keys, Ollama installation)
    pub async fn list_available_providers() -> Vec<ProviderInfo> {
        let mut available = Vec::new();

        // Check Anthropic
        if env::var("ANTHROPIC_API_KEY").is_ok() {
            available.push(ProviderInfo {
                provider_type: ProviderType::Anthropic,
                model: ProviderType::Anthropic.default_model().to_string(),
                available: true,
                reason: None,
            });
        }

        // Check OpenAI
        if env::var("OPENAI_API_KEY").is_ok() {
            available.push(ProviderInfo {
                provider_type: ProviderType::OpenAI,
                model: ProviderType::OpenAI.default_model().to_string(),
                available: true,
                reason: None,
            });
        }

        // Check Ollama
        let ollama = OllamaProvider::new(ProviderType::Ollama.default_model());
        let ollama_health = ollama.health_check().await.ok();

        available.push(ProviderInfo {
            provider_type: ProviderType::Ollama,
            model: ProviderType::Ollama.default_model().to_string(),
            available: ollama_health.as_ref().map(|h| h.available).unwrap_or(false),
            reason: ollama_health.and_then(|h| h.error),
        });

        available
    }
}

/// Provider availability information
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub provider_type: ProviderType,
    pub model: String,
    pub available: bool,
    pub reason: Option<String>,
}

#[async_trait]
impl ModelProvider for ModelRouter {
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream> {
        self.provider.complete(request).await
    }

    fn supports_streaming(&self) -> bool {
        self.provider.supports_streaming()
    }

    fn max_context_tokens(&self) -> usize {
        self.provider.max_context_tokens()
    }

    fn provider_name(&self) -> &str {
        self.provider.provider_name()
    }

    fn model_name(&self) -> &str {
        self.provider.model_name()
    }

    async fn health_check(&self) -> Result<ProviderHealth> {
        self.provider.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_parsing() {
        assert_eq!(
            ProviderType::from_str("anthropic"),
            Some(ProviderType::Anthropic)
        );
        assert_eq!(ProviderType::from_str("CLAUDE"), Some(ProviderType::Anthropic));
        assert_eq!(ProviderType::from_str("openai"), Some(ProviderType::OpenAI));
        assert_eq!(ProviderType::from_str("GPT"), Some(ProviderType::OpenAI));
        assert_eq!(ProviderType::from_str("ollama"), Some(ProviderType::Ollama));
        assert_eq!(ProviderType::from_str("local"), Some(ProviderType::Ollama));
        assert_eq!(ProviderType::from_str("invalid"), None);
    }

    #[test]
    fn test_default_models() {
        assert_eq!(
            ProviderType::Anthropic.default_model(),
            "claude-3-5-sonnet-20241022"
        );
        assert_eq!(ProviderType::OpenAI.default_model(), "gpt-4-turbo");
        assert_eq!(ProviderType::Ollama.default_model(), "llama3");
    }

    #[tokio::test]
    async fn test_auto_detect_fallback_to_ollama() {
        // When no API keys are set, should fall back to Ollama
        // (This test assumes no ANTHROPIC_API_KEY or OPENAI_API_KEY in test env)
        let router = ModelRouter::auto_detect().await;

        // Should succeed (create router) even if Ollama isn't running
        assert!(router.is_ok());

        if let Ok(r) = router {
            // Should have selected some provider (likely Ollama as fallback)
            assert!(!r.current_model().is_empty());
        }
    }
}
