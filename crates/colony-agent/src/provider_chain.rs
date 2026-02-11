//! Provider auto-fallback with health tracking
//!
//! ProviderChain tries providers in order. On failure (timeout, rate limit,
//! error), it falls through to the next provider. Providers marked unhealthy
//! are retried after a configurable cooldown.

use crate::provider::{CompletionRequest, CompletionStream, ModelProvider, ProviderHealth};
use crate::router::{ProviderType, RouterConfig};
use crate::{AgentError, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Provider health state for fallback decisions
#[derive(Debug, Clone)]
pub struct ProviderHealthState {
    pub healthy: bool,
    pub last_failure: Option<Instant>,
    pub failure_count: u32,
    pub last_error: Option<String>,
}

impl Default for ProviderHealthState {
    fn default() -> Self {
        Self {
            healthy: true,
            last_failure: None,
            failure_count: 0,
            last_error: None,
        }
    }
}

/// Configuration for a provider in the chain
#[derive(Debug, Clone)]
pub struct ProviderChainEntry {
    pub config: RouterConfig,
    pub priority: u32,
}

/// Configuration for the provider chain
#[derive(Debug, Clone)]
pub struct ProviderChainConfig {
    pub cooldown_duration: Duration,
    pub max_failures_before_unhealthy: u32,
}

impl Default for ProviderChainConfig {
    fn default() -> Self {
        Self {
            cooldown_duration: Duration::from_secs(60),
            max_failures_before_unhealthy: 3,
        }
    }
}

/// Provider chain with ordered fallback and health tracking
pub struct ProviderChain {
    providers: Vec<(RouterConfig, Box<dyn ModelProvider>)>,
    health_states: HashMap<String, ProviderHealthState>,
    config: ProviderChainConfig,
    active_index: usize,
}

impl ProviderChain {
    /// Create a new provider chain from an ordered list of providers
    pub fn new(
        providers: Vec<(RouterConfig, Box<dyn ModelProvider>)>,
        config: ProviderChainConfig,
    ) -> Self {
        let mut health_states = HashMap::new();
        for (rc, _) in &providers {
            health_states.insert(provider_key(rc), ProviderHealthState::default());
        }

        Self {
            providers,
            health_states,
            config,
            active_index: 0,
        }
    }

    /// Create a chain from provider configs (auto-initializes providers)
    pub async fn from_configs(
        configs: Vec<RouterConfig>,
        chain_config: ProviderChainConfig,
    ) -> Result<Self> {
        let mut providers = Vec::new();

        for config in configs {
            match crate::router::ModelRouter::from_config(config.clone()).await {
                Ok(router) => {
                    providers.push((config, Box::new(router) as Box<dyn ModelProvider>));
                }
                Err(e) => {
                    warn!(
                        "Failed to initialize provider {:?}: {}, skipping",
                        config.provider_type, e
                    );
                }
            }
        }

        if providers.is_empty() {
            return Err(AgentError::Protocol(
                "no providers could be initialized".to_string(),
            ));
        }

        Ok(Self::new(providers, chain_config))
    }

    /// Build a default chain: Anthropic -> OpenAI -> Ollama
    pub fn default_chain_configs() -> Vec<RouterConfig> {
        vec![
            RouterConfig {
                provider_type: ProviderType::Anthropic,
                model_name: ProviderType::Anthropic.default_model().to_string(),
            },
            RouterConfig {
                provider_type: ProviderType::OpenAI,
                model_name: ProviderType::OpenAI.default_model().to_string(),
            },
            RouterConfig {
                provider_type: ProviderType::Ollama,
                model_name: ProviderType::Ollama.default_model().to_string(),
            },
        ]
    }

    /// Check if a provider is available (healthy or past cooldown)
    fn is_available(&self, key: &str) -> bool {
        let Some(state) = self.health_states.get(key) else {
            return true;
        };

        if state.healthy {
            return true;
        }

        let Some(last_failure) = state.last_failure else {
            return true;
        };

        last_failure.elapsed() >= self.config.cooldown_duration
    }

    /// Mark a provider as failed
    fn mark_failed(&mut self, key: &str, error: &str) {
        let state = self
            .health_states
            .entry(key.to_string())
            .or_default();

        state.failure_count += 1;
        state.last_failure = Some(Instant::now());
        state.last_error = Some(error.to_string());

        if state.failure_count >= self.config.max_failures_before_unhealthy {
            state.healthy = false;
            warn!(
                "Provider '{}' marked unhealthy after {} failures",
                key, state.failure_count
            );
        }
    }

    /// Mark a provider as healthy (successful request)
    fn mark_healthy(&mut self, key: &str) {
        let state = self
            .health_states
            .entry(key.to_string())
            .or_default();

        state.healthy = true;
        state.failure_count = 0;
        state.last_error = None;
    }

    /// Get the active provider's name
    pub fn active_provider_name(&self) -> &str {
        self.providers
            .get(self.active_index)
            .map(|(_, p)| p.provider_name())
            .unwrap_or("none")
    }

    /// Get health states for all providers
    pub fn health_states(&self) -> &HashMap<String, ProviderHealthState> {
        &self.health_states
    }

    /// Get the number of providers in the chain
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

#[async_trait]
impl ModelProvider for ProviderChain {
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream> {
        let mut last_error = None;

        for i in 0..self.providers.len() {
            let key = provider_key(&self.providers[i].0);

            if !self.is_available(&key) {
                continue;
            }

            let (_, ref mut provider) = self.providers[i];

            info!(
                "Trying provider: {} (model: {})",
                provider.provider_name(),
                provider.model_name()
            );

            match provider.complete(request.clone()).await {
                Ok(stream) => {
                    self.mark_healthy(&key);
                    self.active_index = i;
                    return Ok(stream);
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    warn!(
                        "Provider {} failed: {}, trying next",
                        self.providers[i].1.provider_name(),
                        error_msg
                    );
                    self.mark_failed(&key, &error_msg);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AgentError::Protocol("all providers exhausted".to_string())
        }))
    }

    fn supports_streaming(&self) -> bool {
        self.providers
            .get(self.active_index)
            .map(|(_, p)| p.supports_streaming())
            .unwrap_or(false)
    }

    fn max_context_tokens(&self) -> usize {
        self.providers
            .get(self.active_index)
            .map(|(_, p)| p.max_context_tokens())
            .unwrap_or(0)
    }

    fn provider_name(&self) -> &str {
        self.active_provider_name()
    }

    fn model_name(&self) -> &str {
        self.providers
            .get(self.active_index)
            .map(|(_, p)| p.model_name())
            .unwrap_or("none")
    }

    async fn health_check(&self) -> Result<ProviderHealth> {
        for (_, provider) in &self.providers {
            let health = provider.health_check().await?;
            if health.available {
                return Ok(health);
            }
        }

        Ok(ProviderHealth {
            available: false,
            latency_ms: None,
            error: Some("no healthy providers".to_string()),
        })
    }
}

fn provider_key(config: &RouterConfig) -> String {
    format!("{:?}:{}", config.provider_type, config.model_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_health_state_default() {
        let state = ProviderHealthState::default();
        assert!(state.healthy);
        assert_eq!(state.failure_count, 0);
        assert!(state.last_failure.is_none());
        assert!(state.last_error.is_none());
    }

    #[test]
    fn test_provider_chain_config_default() {
        let config = ProviderChainConfig::default();
        assert_eq!(config.cooldown_duration, Duration::from_secs(60));
        assert_eq!(config.max_failures_before_unhealthy, 3);
    }

    #[test]
    fn test_default_chain_configs() {
        let configs = ProviderChain::default_chain_configs();
        assert_eq!(configs.len(), 3);
        assert_eq!(configs[0].provider_type, ProviderType::Anthropic);
        assert_eq!(configs[1].provider_type, ProviderType::OpenAI);
        assert_eq!(configs[2].provider_type, ProviderType::Ollama);
    }

    #[test]
    fn test_provider_key() {
        let config = RouterConfig {
            provider_type: ProviderType::Anthropic,
            model_name: "claude-3".to_string(),
        };
        let key = provider_key(&config);
        assert!(key.contains("Anthropic"));
        assert!(key.contains("claude-3"));
    }

    #[test]
    fn test_is_available_healthy() {
        let chain = ProviderChain {
            providers: Vec::new(),
            health_states: {
                let mut map = HashMap::new();
                map.insert("test".to_string(), ProviderHealthState::default());
                map
            },
            config: ProviderChainConfig::default(),
            active_index: 0,
        };

        assert!(chain.is_available("test"));
    }

    #[test]
    fn test_is_available_unhealthy_within_cooldown() {
        let chain = ProviderChain {
            providers: Vec::new(),
            health_states: {
                let mut map = HashMap::new();
                map.insert(
                    "test".to_string(),
                    ProviderHealthState {
                        healthy: false,
                        last_failure: Some(Instant::now()),
                        failure_count: 3,
                        last_error: Some("error".to_string()),
                    },
                );
                map
            },
            config: ProviderChainConfig::default(),
            active_index: 0,
        };

        assert!(!chain.is_available("test"));
    }

    #[test]
    fn test_is_available_unhealthy_past_cooldown() {
        let chain = ProviderChain {
            providers: Vec::new(),
            health_states: {
                let mut map = HashMap::new();
                map.insert(
                    "test".to_string(),
                    ProviderHealthState {
                        healthy: false,
                        last_failure: Some(Instant::now() - Duration::from_secs(120)),
                        failure_count: 3,
                        last_error: Some("error".to_string()),
                    },
                );
                map
            },
            config: ProviderChainConfig {
                cooldown_duration: Duration::from_secs(60),
                ..Default::default()
            },
            active_index: 0,
        };

        assert!(chain.is_available("test"));
    }

    #[test]
    fn test_mark_failed_increments() {
        let mut chain = ProviderChain {
            providers: Vec::new(),
            health_states: HashMap::new(),
            config: ProviderChainConfig {
                max_failures_before_unhealthy: 3,
                ..Default::default()
            },
            active_index: 0,
        };

        chain.mark_failed("test", "err1");
        assert!(chain.health_states["test"].healthy);
        assert_eq!(chain.health_states["test"].failure_count, 1);

        chain.mark_failed("test", "err2");
        assert!(chain.health_states["test"].healthy);

        chain.mark_failed("test", "err3");
        assert!(!chain.health_states["test"].healthy);
    }

    #[test]
    fn test_mark_healthy_resets() {
        let mut chain = ProviderChain {
            providers: Vec::new(),
            health_states: {
                let mut map = HashMap::new();
                map.insert(
                    "test".to_string(),
                    ProviderHealthState {
                        healthy: false,
                        failure_count: 5,
                        last_failure: Some(Instant::now()),
                        last_error: Some("was broken".to_string()),
                    },
                );
                map
            },
            config: ProviderChainConfig::default(),
            active_index: 0,
        };

        chain.mark_healthy("test");
        assert!(chain.health_states["test"].healthy);
        assert_eq!(chain.health_states["test"].failure_count, 0);
        assert!(chain.health_states["test"].last_error.is_none());
    }

    #[test]
    fn test_is_available_unknown_key() {
        let chain = ProviderChain {
            providers: Vec::new(),
            health_states: HashMap::new(),
            config: ProviderChainConfig::default(),
            active_index: 0,
        };

        assert!(chain.is_available("unknown"));
    }
}
