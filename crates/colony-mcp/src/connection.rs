//! Individual MCP server connection management
//!
//! Wraps rmcp client to provide a clean interface for connecting to
//! MCP servers, listing tools, and calling tools.

use std::borrow::Cow;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientInfo, Implementation, ProtocolVersion, Tool,
};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use serde_json::Value;
use tracing::{debug, error, info, warn};

use colony_config::McpServer;

use crate::error::{McpError, Result};

/// Handler for MCP client events (uses defaults for all callbacks)
struct ColonyClientHandler;

impl rmcp::handler::client::ClientHandler for ColonyClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo {
            meta: None,
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: Default::default(),
            client_info: Implementation {
                name: "colony-shell".to_string(),
                title: None,
                version: env!("CARGO_PKG_VERSION").to_string(),
                description: None,
                icons: None,
                website_url: None,
            },
        }
    }
}

/// State of an MCP connection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not yet connected
    Disconnected,
    /// Connected and ready
    Connected,
    /// Connection was lost, attempting reconnect
    Reconnecting,
    /// Connection failed permanently
    Failed(String),
}

/// Cached tool information from a connected MCP server
#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// Tool definition from the server
    pub tool: Tool,
    /// Server this tool belongs to
    pub server_name: String,
}

/// A connection to a single MCP server
pub struct McpConnection {
    /// Server configuration
    config: McpServer,
    /// Running rmcp client service (None when disconnected)
    service: Option<RunningService<RoleClient, ColonyClientHandler>>,
    /// Cached tools from this server
    tools: Vec<ToolInfo>,
    /// Current connection state
    state: ConnectionState,
    /// Number of reconnection attempts made
    reconnect_attempts: u32,
}

impl McpConnection {
    /// Maximum reconnection attempts before giving up
    const MAX_RECONNECT_ATTEMPTS: u32 = 3;

    /// Create a new connection (not yet connected)
    pub fn new(config: McpServer) -> Self {
        Self {
            config,
            service: None,
            tools: Vec::new(),
            state: ConnectionState::Disconnected,
            reconnect_attempts: 0,
        }
    }

    /// Server name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Current connection state
    pub fn state(&self) -> &ConnectionState {
        &self.state
    }

    /// Cached tools from this server
    pub fn tools(&self) -> &[ToolInfo] {
        &self.tools
    }

    /// Server configuration
    pub fn config(&self) -> &McpServer {
        &self.config
    }

    /// Connect to the MCP server via stdio transport
    ///
    /// Spawns the server process, initializes the MCP protocol,
    /// and introspects available tools.
    pub async fn connect(&mut self) -> Result<()> {
        info!(
            server = %self.config.name,
            command = %self.config.command,
            "connecting to MCP server"
        );

        let mut command = tokio::process::Command::new(&self.config.command);
        command.args(&self.config.args);

        for (key, value) in &self.config.env {
            command.env(key, value);
        }

        let transport = TokioChildProcess::new(command).map_err(|e| McpError::SpawnFailed {
            name: self.config.name.clone(),
            source: e,
        })?;

        let service = rmcp::service::serve_client(ColonyClientHandler, transport)
            .await
            .map_err(|e| McpError::InitFailed {
                name: self.config.name.clone(),
                message: e.to_string(),
            })?;

        info!(
            server = %self.config.name,
            "MCP server connected, introspecting tools"
        );

        let tools_result =
            service
                .peer()
                .list_tools(None)
                .await
                .map_err(|e| McpError::ListToolsFailed {
                    name: self.config.name.clone(),
                    message: e.to_string(),
                })?;

        self.tools = tools_result
            .tools
            .into_iter()
            .map(|tool| ToolInfo {
                tool,
                server_name: self.config.name.clone(),
            })
            .collect();

        info!(
            server = %self.config.name,
            tool_count = self.tools.len(),
            "MCP server ready with {} tools",
            self.tools.len()
        );

        self.service = Some(service);
        self.state = ConnectionState::Connected;
        self.reconnect_attempts = 0;

        Ok(())
    }

    /// Call a tool on this server
    ///
    /// # Errors
    ///
    /// Returns `McpError::NotConnected` if the server is not connected,
    /// `McpError::ToolNotFound` if the tool doesn't exist on this server,
    /// or `McpError::ToolCallFailed` if the call fails.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
    ) -> Result<CallToolResult> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| McpError::NotConnected {
                name: self.config.name.clone(),
            })?;

        let tool_exists = self.tools.iter().any(|t| t.tool.name.as_ref() == tool_name);

        if !tool_exists {
            return Err(McpError::ToolNotFound {
                server: self.config.name.clone(),
                tool: tool_name.to_string(),
            });
        }

        debug!(
            server = %self.config.name,
            tool = %tool_name,
            "calling MCP tool"
        );

        let params = CallToolRequestParams {
            meta: None,
            name: Cow::Owned(tool_name.to_string()),
            arguments,
            task: None,
        };

        service
            .peer()
            .call_tool(params)
            .await
            .map_err(|e| McpError::ToolCallFailed {
                server: self.config.name.clone(),
                tool: tool_name.to_string(),
                message: e.to_string(),
            })
    }

    /// Attempt to reconnect after a disconnection
    pub async fn reconnect(&mut self) -> Result<()> {
        if self.reconnect_attempts >= Self::MAX_RECONNECT_ATTEMPTS {
            let msg = format!(
                "exceeded max reconnect attempts ({})",
                Self::MAX_RECONNECT_ATTEMPTS
            );
            self.state = ConnectionState::Failed(msg.clone());
            return Err(McpError::ReconnectFailed {
                name: self.config.name.clone(),
                attempts: self.reconnect_attempts,
                message: msg,
            });
        }

        self.reconnect_attempts += 1;
        self.state = ConnectionState::Reconnecting;

        warn!(
            server = %self.config.name,
            attempt = self.reconnect_attempts,
            max = Self::MAX_RECONNECT_ATTEMPTS,
            "attempting reconnection"
        );

        self.disconnect().await;

        match self.connect().await {
            Ok(()) => {
                info!(
                    server = %self.config.name,
                    "reconnection successful"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    server = %self.config.name,
                    error = %e,
                    "reconnection failed"
                );
                self.state = ConnectionState::Disconnected;
                Err(e)
            }
        }
    }

    /// Disconnect from the server
    pub async fn disconnect(&mut self) {
        if let Some(service) = self.service.take() {
            info!(server = %self.config.name, "disconnecting from MCP server");
            let _ = service.cancel().await;
        }
        self.tools.clear();
        self.state = ConnectionState::Disconnected;
    }

    /// Refresh the tool list from the server
    pub async fn refresh_tools(&mut self) -> Result<()> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| McpError::NotConnected {
                name: self.config.name.clone(),
            })?;

        let tools_result =
            service
                .peer()
                .list_tools(None)
                .await
                .map_err(|e| McpError::ListToolsFailed {
                    name: self.config.name.clone(),
                    message: e.to_string(),
                })?;

        self.tools = tools_result
            .tools
            .into_iter()
            .map(|tool| ToolInfo {
                tool,
                server_name: self.config.name.clone(),
            })
            .collect();

        debug!(
            server = %self.config.name,
            tool_count = self.tools.len(),
            "refreshed tool list"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn new_connection_is_disconnected() {
        let conn = McpConnection::new(test_server("test"));
        assert_eq!(*conn.state(), ConnectionState::Disconnected);
        assert!(conn.tools().is_empty());
        assert_eq!(conn.name(), "test");
    }

    #[test]
    fn connection_state_equality() {
        assert_eq!(ConnectionState::Disconnected, ConnectionState::Disconnected);
        assert_eq!(ConnectionState::Connected, ConnectionState::Connected);
        assert_ne!(ConnectionState::Connected, ConnectionState::Disconnected);
        assert_eq!(
            ConnectionState::Failed("err".to_string()),
            ConnectionState::Failed("err".to_string()),
        );
        assert_ne!(
            ConnectionState::Failed("a".to_string()),
            ConnectionState::Failed("b".to_string()),
        );
    }

    #[test]
    fn connection_state_clone() {
        let state = ConnectionState::Failed("timeout".to_string());
        let cloned = state.clone();
        assert_eq!(state, cloned);
    }

    #[test]
    fn new_connection_has_zero_reconnect_attempts() {
        let conn = McpConnection::new(test_server("test"));
        // reconnect_attempts is private but we can verify via debug output
        let debug = format!("{:?}", conn);
        assert!(debug.contains("reconnect_attempts: 0"));
    }

    #[test]
    fn config_accessible_after_creation() {
        let server = test_server("myserver");
        let conn = McpConnection::new(server.clone());
        assert_eq!(conn.config().name, "myserver");
        assert_eq!(conn.config().command, "echo");
    }

    #[test]
    fn debug_format_shows_connection_info() {
        let conn = McpConnection::new(test_server("debug-test"));
        let debug = format!("{:?}", conn);
        assert!(debug.contains("McpConnection"));
        assert!(debug.contains("debug-test"));
        assert!(debug.contains("Disconnected"));
    }
}

impl std::fmt::Debug for McpConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpConnection")
            .field("name", &self.config.name)
            .field("state", &self.state)
            .field("tools", &self.tools.len())
            .field("reconnect_attempts", &self.reconnect_attempts)
            .finish()
    }
}
