//! MCP connection pool managing multiple server connections
//!
//! The pool handles connecting to all configured MCP servers,
//! routing tool calls to the correct server, and reconnection.

use std::collections::HashMap;

use serde_json::Value;
use tracing::{debug, error, info, warn};

use colony_config::McpConfig;
use rmcp::model::CallToolResult;

use crate::connection::{ConnectionState, McpConnection, ToolInfo};
use crate::error::{McpError, Result};

/// Manages connections to multiple MCP servers
///
/// The pool provides a unified interface for tool discovery and invocation
/// across all connected MCP servers.
///
/// # Example
///
/// ```no_run
/// use colony_mcp::McpConnectionPool;
/// use colony_config::McpConfig;
///
/// # async fn example() -> colony_mcp::Result<()> {
/// let config = McpConfig::default();
/// let mut pool = McpConnectionPool::new();
/// pool.connect_all(&config).await?;
///
/// // List all tools across all servers
/// let tools = pool.all_tools();
///
/// // Call a tool (routed to the correct server)
/// let result = pool.call_tool("read_file", None).await?;
/// # Ok(())
/// # }
/// ```
pub struct McpConnectionPool {
    /// Active connections keyed by server name
    connections: HashMap<String, McpConnection>,
}

impl McpConnectionPool {
    /// Create a new empty connection pool
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    /// Connect to all configured MCP servers
    ///
    /// Attempts to connect to each server in the config. Failures for
    /// individual servers are logged but don't prevent other connections.
    ///
    /// Returns `Ok(())` even if some (or all) auto-start servers fail to
    /// connect. Callers should check [`all_tools()`](Self::all_tools) to see
    /// which servers are actually available.
    pub async fn connect_all(&mut self, config: &McpConfig) -> Result<()> {
        if config.servers.is_empty() {
            return Err(McpError::NoServersConfigured);
        }

        for server_config in &config.servers {
            if !server_config.auto_start {
                debug!(
                    server = %server_config.name,
                    "skipping server (auto_start=false)"
                );
                continue;
            }

            let mut connection = McpConnection::new(server_config.clone());

            match connection.connect().await {
                Ok(()) => {
                    info!(
                        server = %server_config.name,
                        tools = connection.tools().len(),
                        "server connected"
                    );
                    self.connections
                        .insert(server_config.name.clone(), connection);
                }
                Err(e) => {
                    error!(
                        server = %server_config.name,
                        error = %e,
                        "failed to connect to server, skipping"
                    );
                }
            }
        }

        info!(
            connected = self.connections.len(),
            total_tools = self.all_tools().len(),
            "MCP connection pool ready"
        );

        Ok(())
    }

    /// Connect to a single server by configuration
    pub async fn connect_server(&mut self, server_config: &colony_config::McpServer) -> Result<()> {
        let mut connection = McpConnection::new(server_config.clone());
        connection.connect().await?;
        self.connections
            .insert(server_config.name.clone(), connection);
        Ok(())
    }

    /// Disconnect from a specific server
    pub async fn disconnect_server(&mut self, name: &str) -> Result<()> {
        let Some(mut connection) = self.connections.remove(name) else {
            return Err(McpError::NotConnected {
                name: name.to_string(),
            });
        };
        connection.disconnect().await;
        Ok(())
    }

    /// Disconnect from all servers
    pub async fn disconnect_all(&mut self) {
        for (name, mut connection) in self.connections.drain() {
            debug!(server = %name, "disconnecting");
            connection.disconnect().await;
        }
    }

    /// Get all tools across all connected servers
    pub fn all_tools(&self) -> Vec<&ToolInfo> {
        self.connections
            .values()
            .flat_map(|conn| conn.tools())
            .collect()
    }

    /// Get tools from a specific server
    pub fn server_tools(&self, server_name: &str) -> Option<&[ToolInfo]> {
        self.connections.get(server_name).map(|c| c.tools())
    }

    /// Find which server provides a given tool
    pub fn find_tool_server(&self, tool_name: &str) -> Option<&str> {
        self.connections
            .values()
            .find(|conn| {
                conn.tools()
                    .iter()
                    .any(|t| t.tool.name.as_ref() == tool_name)
            })
            .map(|conn| conn.name())
    }

    /// Find a tool by name across all servers
    pub fn find_tool(&self, tool_name: &str) -> Option<&ToolInfo> {
        self.connections.values().find_map(|conn| {
            conn.tools()
                .iter()
                .find(|t| t.tool.name.as_ref() == tool_name)
        })
    }

    /// Call a tool, automatically routing to the correct server
    ///
    /// The pool looks up which server provides the requested tool
    /// and routes the call accordingly. If the call fails due to
    /// disconnection, one reconnection attempt is made.
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
    ) -> Result<CallToolResult> {
        let server_name = self
            .find_tool_server(tool_name)
            .ok_or_else(|| {
                let available: Vec<String> = self
                    .all_tools()
                    .iter()
                    .map(|t| t.tool.name.to_string())
                    .collect();
                McpError::ToolNotFound {
                    server: format!("(any of {} servers)", self.connections.len()),
                    tool: format!(
                        "{} (available: {})",
                        tool_name,
                        if available.is_empty() {
                            "none".to_string()
                        } else {
                            available.join(", ")
                        }
                    ),
                }
            })?
            .to_string();

        let connection = self
            .connections
            .get(&server_name)
            .expect("server must exist since find_tool_server returned it");

        match connection.call_tool(tool_name, arguments.clone()).await {
            Ok(result) => Ok(result),
            Err(McpError::ToolCallFailed { server, tool, .. }) => {
                warn!(
                    server = %server,
                    tool = %tool,
                    "tool call failed, attempting reconnection"
                );

                let connection = self
                    .connections
                    .get_mut(&server_name)
                    .expect("server must exist");
                connection.reconnect().await?;
                connection.call_tool(tool_name, arguments).await
            }
            Err(e) => Err(e),
        }
    }

    /// Get the state of a specific server connection
    pub fn connection_state(&self, server_name: &str) -> Option<&ConnectionState> {
        self.connections.get(server_name).map(|c| c.state())
    }

    /// Get all connected server names
    pub fn connected_servers(&self) -> Vec<&str> {
        self.connections
            .values()
            .filter(|c| matches!(c.state(), ConnectionState::Connected))
            .map(|c| c.name())
            .collect()
    }

    /// Get all server names (regardless of state)
    pub fn server_names(&self) -> Vec<&str> {
        self.connections.keys().map(|s| s.as_str()).collect()
    }

    /// Number of connected servers
    pub fn connected_count(&self) -> usize {
        self.connections
            .values()
            .filter(|c| matches!(c.state(), ConnectionState::Connected))
            .count()
    }

    /// Total number of tools available across all servers
    pub fn tool_count(&self) -> usize {
        self.connections.values().map(|c| c.tools().len()).sum()
    }

    /// Refresh tools from all connected servers
    pub async fn refresh_all_tools(&mut self) {
        for (name, connection) in &mut self.connections {
            if matches!(connection.state(), ConnectionState::Connected) {
                if let Err(e) = connection.refresh_tools().await {
                    warn!(
                        server = %name,
                        error = %e,
                        "failed to refresh tools"
                    );
                }
            }
        }
    }

    /// Get a reference to a specific connection
    pub fn connection(&self, server_name: &str) -> Option<&McpConnection> {
        self.connections.get(server_name)
    }
}

impl Default for McpConnectionPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_config::McpServer;

    fn test_server(name: &str) -> McpServer {
        McpServer {
            name: name.to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: Default::default(),
            auto_start: true,
        }
    }

    #[test]
    fn new_pool_is_empty() {
        let pool = McpConnectionPool::new();
        assert_eq!(pool.connected_count(), 0);
        assert_eq!(pool.tool_count(), 0);
        assert!(pool.all_tools().is_empty());
        assert!(pool.server_names().is_empty());
    }

    #[test]
    fn default_is_same_as_new() {
        let pool = McpConnectionPool::default();
        assert_eq!(pool.connected_count(), 0);
    }

    #[test]
    fn find_tool_returns_none_on_empty_pool() {
        let pool = McpConnectionPool::new();
        assert!(pool.find_tool("nonexistent").is_none());
        assert!(pool.find_tool_server("nonexistent").is_none());
    }

    #[test]
    fn connection_state_returns_none_for_unknown_server() {
        let pool = McpConnectionPool::new();
        assert!(pool.connection_state("unknown").is_none());
    }

    #[test]
    fn server_tools_returns_none_for_unknown_server() {
        let pool = McpConnectionPool::new();
        assert!(pool.server_tools("unknown").is_none());
    }

    #[tokio::test]
    async fn connect_all_errors_on_empty_config() {
        let mut pool = McpConnectionPool::new();
        let config = McpConfig::default();
        let result = pool.connect_all(&config).await;
        assert!(result.is_err());
    }

    #[test]
    fn debug_format_shows_pool_state() {
        let pool = McpConnectionPool::new();
        let debug = format!("{:?}", pool);
        assert!(debug.contains("McpConnectionPool"));
        assert!(debug.contains("servers"));
        assert!(debug.contains("connected"));
        assert!(debug.contains("tools"));
    }
}

impl std::fmt::Debug for McpConnectionPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpConnectionPool")
            .field("servers", &self.connections.len())
            .field("connected", &self.connected_count())
            .field("tools", &self.tool_count())
            .finish()
    }
}
