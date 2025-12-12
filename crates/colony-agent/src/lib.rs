//! Claude ADK integration and natural language detection
//!
//! This crate provides the agent layer for Colony Shell, integrating with
//! Claude via our native Rust SDK port. Handles tool classification,
//! programmatic tool execution, and streaming responses to the terminal.

pub mod tool;
pub mod agent;
pub mod detector;

pub use agent::{Agent, AgentConfig};
pub use tool::{ToolRegistry, ToolClassification, RegisteredTool};
pub use detector::NaturalLanguageDetector;

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
