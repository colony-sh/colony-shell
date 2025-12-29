//! Extended channels system for Colony Shell
//!
//! Implements semantic channels beyond stdin/stdout/stderr:
//! - stdcontext (fd 3): Session context, observations, decisions
//! - stdllm (fd 4): LLM conversation history
//! - stdtool (fd 5): Tool call logging
//! - stdtrace (fd 6): Execution trace
//!
//! Phase 1: Parent-side channels (builtins can write/read)
//! Phase 2: Pipe-based redirects (external commands can write)

mod buffer;
mod channel;
mod context;
mod manager;

pub use buffer::InternalBuffer;
pub use channel::{
    Channel, ChannelId, STDCONTEXT, STDERR, STDIN, STDLLM, STDOUT, STDTOOL, STDTRACE,
};
pub use context::ContextChannel;
pub use manager::ChannelManager;

/// Channel error types
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Channel not found: {0}")]
    NotFound(u8),

    #[error("Invalid channel operation: {0}")]
    InvalidOperation(String),
}

pub type Result<T> = std::result::Result<T, ChannelError>;
