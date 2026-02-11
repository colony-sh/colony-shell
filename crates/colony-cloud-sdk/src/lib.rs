//! Colony Cloud SDK — Rust client for the Colony Cloud managed LLM service
//!
//! Provides typed API access to Colony Cloud endpoints:
//! - `POST /v1/chat/completions` — Chat completions (streaming + non-streaming)
//! - `GET /v1/models` — List available models
//! - `POST /v1/recipes` — Submit recipes to the registry
//!
//! # Authentication
//!
//! Auth is resolved in order:
//! 1. Explicit API key via `ColonyClient::new("key")`
//! 2. `COLONY_API_KEY` environment variable
//! 3. `~/.colony/config.toml` under `[cloud]` section
//!
//! # Example
//!
//! ```no_run
//! use colony_cloud_sdk::{ColonyClient, ChatRequest, ChatMessage};
//!
//! # async fn example() -> colony_cloud_sdk::Result<()> {
//! let client = ColonyClient::from_env()?;
//!
//! let models = client.models().await?;
//! for model in &models {
//!     println!("{}: {}", model.id, model.owned_by);
//! }
//!
//! let request = ChatRequest {
//!     model: "claude-3.5-sonnet".to_string(),
//!     messages: vec![ChatMessage {
//!         role: "user".to_string(),
//!         content: "Hello!".to_string(),
//!     }],
//!     max_tokens: Some(1024),
//!     temperature: None,
//!     stream: None,
//!     tools: None,
//!     cache_system_prompt: None,
//! };
//!
//! let response = client.chat(&request).await?;
//! println!("{}", response.choices[0].message.content);
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod config;
pub mod error;
pub mod models;
pub mod rate_limit;
pub mod retry;
pub mod streaming;
pub mod timeout;

pub use client::ColonyClient;
pub use error::{CloudError, Result};
pub use models::{
    ChatChoice, ChatFunction, ChatMessage, ChatRequest, ChatResponse, ChatStreamChunk, ChatTool,
    Model, ModelsResponse, RecipeResponse, RecipeSubmission, UsageInfo,
};
pub use rate_limit::{RateLimitConfig, RateLimitInfo, RateLimiter};
pub use retry::{with_retry, RetryConfig};
pub use streaming::{
    ExtendedStreamChunk, ExtendedStreamChoice, ExtendedStreamDelta, StreamAssembler,
    StreamContent, StreamToolCall, StreamToolFunction, ToolCallDelta, ToolFunctionDelta,
};
pub use timeout::{
    with_timeout, OperationLog, OperationTracker, OperationType, OperationStatus, RequestId,
    TimeoutConfig,
};
