//! MCP (Model Context Protocol) integration for Colony Shell
//!
//! This crate provides:
//! - **Connection Pool** (`McpConnectionPool`): Manages connections to multiple MCP servers
//!   via stdio transport using rmcp. Handles lifecycle, tool introspection, and reconnection.
//! - **TypeScript Codegen** (`McpCodegen`): Generates typed TypeScript client classes from
//!   MCP server tool schemas using Tera templates. Caches by schema content hash.
//! - **Executor Session** (`McpSession`): Execution context for MCP tool calls with
//!   tuple space integration and tool approval workflow.
//!
//! # Architecture
//!
//! ```text
//! .colonyrc [mcp] config
//!        │
//!        ▼
//!  McpConnectionPool ── connects ──► MCP Server (stdio)
//!        │                               │
//!        │ list_tools()                  │ tool schemas
//!        ▼                               ▼
//!   McpCodegen ── generates ──► ~/.colony/mcp-clients/<server>/client.ts
//!        │
//!        ▼
//!   McpSession ── tuple space ──► tool call requests/responses
//! ```
//!
//! # Example
//!
//! ```no_run
//! use colony_mcp::{McpConnectionPool, McpCodegen};
//! use colony_config::McpConfig;
//!
//! # async fn example() -> colony_mcp::Result<()> {
//! // Connect to configured servers
//! let config = McpConfig::default();
//! let mut pool = McpConnectionPool::new();
//! pool.connect_all(&config).await?;
//!
//! // Generate TypeScript clients
//! let mut codegen = McpCodegen::new()?;
//! for server in pool.connected_servers() {
//!     if let Some(tools) = pool.server_tools(server) {
//!         codegen.generate(server, tools)?;
//!     }
//! }
//!
//! // Call a tool
//! let result = pool.call_tool("read_file", None).await?;
//! # Ok(())
//! # }
//! ```

pub mod codegen;
pub mod connection;
pub mod error;
pub mod pool;
pub mod session;

pub use codegen::McpCodegen;
pub use connection::{ConnectionState, McpConnection, ToolInfo};
pub use error::{McpError, Result};
pub use pool::McpConnectionPool;
pub use session::{McpSession, ToolCallRequest, ToolCallResponse, ToolClassification};
