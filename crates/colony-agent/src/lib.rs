//! Multi-provider LLM integration and natural language detection
//!
//! This crate provides the agent layer for Colony Shell with support for:
//! - Anthropic (Claude models via our Rust SDK)
//! - OpenAI (GPT models)
//! - Ollama (local models - free fallback)
//!
//! Features:
//! - Auto-detection: Tries providers in order based on API keys
//! - Model router: Switch providers at runtime
//! - Tool classification: PROGRAMMATIC vs USER_FACING
//! - Streaming responses via OSC 2337

pub mod agent;
pub mod detector;
pub mod provider;
pub mod providers;
pub mod router;
pub mod tool;

pub use agent::{Agent, AgentConfig};
pub use detector::NaturalLanguageDetector;
pub use provider::{CompletionRequest, ModelProvider, StreamEvent};
#[cfg(feature = "cloud")]
pub use providers::CloudProvider;
pub use providers::{AnthropicProvider, OllamaProvider, OpenAIProvider};
pub use router::{ModelRouter, ProviderType, RouterConfig};
pub use tool::{RegisteredTool, ToolClassification, ToolRegistry};

/// Agent error types
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Claude API error: {0}")]
    ClaudeApi(#[from] claude_sdk::Error),

    #[error("tool execution failed: {0}")]
    ToolExecution(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("conversation error: {0}")]
    Conversation(String),
}

pub type Result<T> = std::result::Result<T, AgentError>;
