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
//! - Tool framework: Built-in tools with parameter validation
//! - Tool approval: OSC 2337 widget + stderr fallback
//! - Conversation persistence: SQLite-backed history
//! - Token accounting: Usage tracking with configurable budgets
//! - Context management: Automatic pruning and summarization
//! - Streaming cancellation: CancellationToken for Ctrl+C support
//! - Provider fallback: ProviderChain with health tracking
//! - Memory integration: colony-memory for persistent context

pub mod agent;
pub mod approval;
pub mod cancellation;
pub mod context;
pub mod detector;
pub mod memory_integration;
pub mod persistence;
pub mod provider;
pub mod provider_chain;
pub mod providers;
pub mod router;
pub mod token_accounting;
pub mod tool;

pub use agent::{Agent, AgentConfig};
pub use approval::{ApprovalConfig, ApprovalManager, ApprovalResponse};
pub use cancellation::{CancellableStreamResult, CancellationToken};
pub use context::{ContextConfig, ContextManager};
pub use detector::NaturalLanguageDetector;
pub use persistence::{ConversationMeta, ConversationStore};
pub use provider::{CompletionRequest, ModelProvider, StreamEvent};
pub use provider_chain::{ProviderChain, ProviderChainConfig};
#[cfg(feature = "cloud")]
pub use providers::CloudProvider;
pub use providers::{AnthropicProvider, OllamaProvider, OpenAIProvider};
pub use router::{ModelRouter, ProviderType, RouterConfig};
pub use token_accounting::{BudgetWarning, RequestUsage, TokenBudget, TokenTracker};
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

    #[error("cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, AgentError>;
