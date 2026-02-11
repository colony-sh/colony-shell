//! Extended channels system for Colony Shell
//!
//! Implements semantic channels beyond stdin/stdout/stderr:
//! - stdcontext (fd 3): Session context, observations, decisions
//! - stdllm (fd 4): LLM conversation history
//! - stdtool (fd 5): Tool call logging
//! - stddiag (fd 6): Diagnostics
//!
//! Phase 1: Parent-side channels (builtins can write/read)
//! Phase 2: Pipe-based redirects (external commands can write)

mod buffer;
mod channel;
mod context;
mod diag;
mod llm;
mod manager;
pub mod pipe;
pub mod protocol;
mod tool;

pub use buffer::InternalBuffer;
pub use channel::{
    Channel, ChannelId, STDCONTEXT, STDDIAG, STDERR, STDIN, STDLLM, STDOUT, STDTOOL,
};
pub use context::ContextChannel;
pub use diag::{DiagChannel, DiagEntry, Severity};
pub use llm::{LlmChannel, LlmMessage, LlmRole};
pub use manager::ChannelManager;
pub use pipe::{ChannelPipes, PipePair};
pub use protocol::{ChannelCodec, Frame, FrameFlags, ProtocolError, StreamAssembler, TypeTag};
pub use tool::{ToolChannel, ToolEntry, ToolMessage, ToolRequest, ToolResult};

/// Channel error types
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Channel not found: {0}")]
    NotFound(u8),

    #[error("Invalid channel operation: {0}")]
    InvalidOperation(String),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),
}

pub type Result<T> = std::result::Result<T, ChannelError>;
