//! Model provider abstraction for multi-LLM support
//!
//! Provides a unified interface for different LLM providers:
//! - Anthropic (Claude via our Rust SDK port)
//! - OpenAI (GPT models)
//! - Ollama (local models - free fallback)
//! - Future: AWS Bedrock, Azure OpenAI, custom endpoints
//!
//! The ModelProvider trait abstracts away provider differences while
//! preserving provider-specific features like caching.

use crate::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Provider-agnostic completion request
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Conversation messages
    pub messages: Vec<Message>,

    /// System prompt (optional, provider-specific handling)
    pub system_prompt: Option<String>,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// Available tools for function calling
    pub tools: Vec<ToolDefinition>,

    /// Sampling temperature (0.0-1.0)
    pub temperature: Option<f32>,

    /// Enable prompt caching if provider supports it
    pub enable_caching: bool,
}

/// Unified message format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// Tool definition in provider-agnostic format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Stream events from model provider
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text delta (streaming text generation)
    TextDelta { text: String, index: usize },

    /// Tool call started
    ToolCallStart { id: String, name: String },

    /// Tool call completed with input
    ToolCallComplete {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Stream completed
    Done,

    /// Error occurred
    Error { message: String },
}

/// Type alias for completion stream
pub type CompletionStream = Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>;

/// Model provider trait
///
/// Implementors provide LLM completion capabilities with streaming support.
/// Each provider handles its own authentication, rate limiting, and error handling.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Send completion request and get streaming response
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream>;

    /// Check if this provider supports streaming responses
    fn supports_streaming(&self) -> bool {
        true // Most modern LLM APIs support streaming
    }

    /// Maximum context window size in tokens
    fn max_context_tokens(&self) -> usize;

    /// Provider name for display/logging
    fn provider_name(&self) -> &str;

    /// Model name/ID being used
    fn model_name(&self) -> &str;

    /// Check if provider is available (auth, network, etc.)
    async fn health_check(&self) -> Result<ProviderHealth>;
}

/// Provider health status
#[derive(Debug, Clone)]
pub struct ProviderHealth {
    pub available: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Provider metadata for display
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub model: String,
    pub context_window: usize,
    pub supports_tools: bool,
    pub supports_caching: bool,
    pub cost_per_million_tokens: Option<f64>,
}

impl CompletionRequest {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: None,
            max_tokens: 4096,
            tools: Vec::new(),
            temperature: None,
            enable_caching: false,
        }
    }

    pub fn with_system(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = tokens;
        self
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn enable_caching(mut self) -> Self {
        self.enable_caching = true;
        self
    }

    pub fn add_user_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message {
            role: MessageRole::User,
            content: MessageContent::Text(content.into()),
        });
        self
    }

    pub fn add_assistant_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message {
            role: MessageRole::Assistant,
            content: MessageContent::Text(content.into()),
        });
        self
    }
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self::new()
    }
}
