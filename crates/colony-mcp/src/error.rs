//! Error types for MCP operations

use thiserror::Error;

/// Errors from MCP connection and tool operations
#[derive(Debug, Error)]
pub enum McpError {
    #[error("failed to spawn MCP server process '{name}': {source}")]
    SpawnFailed {
        name: String,
        source: std::io::Error,
    },

    #[error("MCP server '{name}' initialization failed: {message}")]
    InitFailed { name: String, message: String },

    #[error("MCP server '{name}' is not connected")]
    NotConnected { name: String },

    #[error("tool '{tool}' not found on server '{server}'")]
    ToolNotFound { server: String, tool: String },

    #[error("tool call failed on server '{server}', tool '{tool}': {message}")]
    ToolCallFailed {
        server: String,
        tool: String,
        message: String,
    },

    #[error("failed to list tools from server '{name}': {message}")]
    ListToolsFailed { name: String, message: String },

    #[error("server '{name}' disconnected unexpectedly")]
    Disconnected { name: String },

    #[error("reconnection to server '{name}' failed after {attempts} attempts: {message}")]
    ReconnectFailed {
        name: String,
        attempts: u32,
        message: String,
    },

    #[error("no MCP servers configured")]
    NoServersConfigured,

    #[error("codegen failed for server '{server}': {message}")]
    CodegenFailed { server: String, message: String },

    #[error("template error: {0}")]
    TemplateError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, McpError>;
