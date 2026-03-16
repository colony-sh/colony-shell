//! MCP executor session for tool call execution
//!
//! Receives tool call tasks via tuple space, generates TypeScript scripts,
//! executes them, and returns results through tuple space.

use std::path::PathBuf;

use chrono::Utc;
use colony_channels::ToolMessage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, error};

use colony_tuple::TupleSpace;

use crate::codegen::McpCodegen;
use crate::error::{McpError, Result};
use crate::pool::McpConnectionPool;

/// Tool classification for approval workflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolClassification {
    /// Auto-execute: memory ops, tuple space, read-only queries, safe computations
    Programmatic,
    /// Require approval: file system writes, external API calls, destructive ops
    UserFacing,
}

/// A tool call request received via tuple space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    /// Unique request ID
    pub id: String,
    /// Server name (or empty for auto-routing)
    pub server: String,
    /// Tool name to invoke
    pub tool: String,
    /// Tool arguments as JSON
    pub arguments: Option<serde_json::Map<String, Value>>,
    /// Whether this request has been approved (for user-facing tools)
    pub approved: bool,
    /// Classification override (if not set, defaults from server manifest)
    pub classification: Option<ToolClassification>,
}

/// Result of a tool call returned via tuple space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    /// Matching request ID
    pub id: String,
    /// Whether the call succeeded
    pub success: bool,
    /// Result content (text or structured)
    pub content: Value,
    /// Error message if failed
    pub error: Option<String>,
}

/// MCP executor session managing tool call lifecycle
///
/// The session:
/// 1. Listens for tool call requests on the tuple space
/// 2. Checks tool classification for approval requirements
/// 3. Generates TypeScript scripts via codegen
/// 4. Executes tool calls through the connection pool
/// 5. Returns results on the tuple space
pub struct McpSession {
    /// Connection pool for making tool calls
    pool: McpConnectionPool,
    /// Code generator for TypeScript clients
    codegen: McpCodegen,
    /// Tuple space for receiving tasks and returning results
    tuple_space: TupleSpace,
    /// Tool classification overrides from config
    classifications: std::collections::HashMap<String, ToolClassification>,
    /// Channel writer for tool call logging (fd 5 / stdtool)
    tool_channel_tx: Option<mpsc::UnboundedSender<ToolMessage>>,
}

impl McpSession {
    /// Create a new MCP session
    pub fn new(
        pool: McpConnectionPool,
        codegen: McpCodegen,
        tuple_space: TupleSpace,
    ) -> Self {
        Self {
            pool,
            codegen,
            tuple_space,
            classifications: std::collections::HashMap::new(),
            tool_channel_tx: None,
        }
    }

    /// Set the tool channel writer for forwarding tool calls to fd 5 (stdtool).
    ///
    /// Returns the receiving end so the caller can drain messages into a `ChannelManager`.
    pub fn set_tool_channel(&mut self) -> mpsc::UnboundedReceiver<ToolMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.tool_channel_tx = Some(tx);
        rx
    }

    /// Send a tool message to the tool channel if a writer is configured.
    fn emit_to_tool_channel(&self, msg: ToolMessage) {
        if let Some(ref tx) = self.tool_channel_tx {
            let _ = tx.send(msg);
        }
    }

    /// Set tool classifications from config
    pub fn set_classifications(
        &mut self,
        classifications: std::collections::HashMap<String, ToolClassification>,
    ) {
        self.classifications = classifications;
    }

    /// Get the classification for a tool
    ///
    /// Checks config overrides first, then defaults to `UserFacing`.
    pub fn classify_tool(&self, tool_name: &str) -> ToolClassification {
        self.classifications
            .get(tool_name)
            .copied()
            .unwrap_or(ToolClassification::UserFacing)
    }

    /// Execute a tool call request
    ///
    /// For programmatic tools, executes immediately.
    /// For user-facing tools, checks the `approved` flag first.
    pub async fn execute_request(&mut self, request: &ToolCallRequest) -> ToolCallResponse {
        let classification = request
            .classification
            .unwrap_or_else(|| self.classify_tool(&request.tool));

        if classification == ToolClassification::UserFacing && !request.approved {
            return ToolCallResponse {
                id: request.id.clone(),
                success: false,
                content: Value::Null,
                error: Some(format!(
                    "tool '{}' requires user approval (classification: UserFacing)",
                    request.tool
                )),
            };
        }

        debug!(
            tool = %request.tool,
            server = %request.server,
            classification = ?classification,
            "executing tool call"
        );

        // Emit tool request to ToolChannel (fd 5)
        let request_start = std::time::Instant::now();
        self.emit_to_tool_channel(ToolMessage::Request(colony_channels::ToolRequest {
            id: request.id.clone(),
            tool_name: request.tool.clone(),
            arguments: serde_json::to_value(&request.arguments).unwrap_or(Value::Null),
            timestamp: Utc::now(),
        }));

        let response = match self
            .pool
            .call_tool(&request.tool, request.arguments.clone())
            .await
        {
            Ok(result) => {
                let is_error = result.is_error.unwrap_or(false);
                let content = result
                    .structured_content
                    .unwrap_or_else(|| {
                        Value::Array(
                            result
                                .content
                                .iter()
                                .map(|c| serde_json::to_value(c).unwrap_or(Value::Null))
                                .collect(),
                        )
                    });

                ToolCallResponse {
                    id: request.id.clone(),
                    success: !is_error,
                    content,
                    error: if is_error {
                        Some("tool reported error".to_string())
                    } else {
                        None
                    },
                }
            }
            Err(e) => {
                error!(
                    tool = %request.tool,
                    error = %e,
                    "tool call failed"
                );
                ToolCallResponse {
                    id: request.id.clone(),
                    success: false,
                    content: Value::Null,
                    error: Some(e.to_string()),
                }
            }
        };

        // Emit tool result to ToolChannel (fd 5)
        let duration_ms = request_start.elapsed().as_millis() as u64;
        self.emit_to_tool_channel(ToolMessage::Result(colony_channels::ToolResult {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: request.id.clone(),
            output: response.content.clone(),
            success: response.success,
            error: response.error.clone(),
            duration_ms,
            timestamp: Utc::now(),
        }));

        response
    }

    /// Generate TypeScript client for a server
    ///
    /// Uses the codegen module to create typed TypeScript clients
    /// from the server's tool schemas.
    pub fn generate_client(&mut self, server_name: &str) -> Result<PathBuf> {
        let tools = self
            .pool
            .server_tools(server_name)
            .ok_or_else(|| McpError::NotConnected {
                name: server_name.to_string(),
            })?;

        self.codegen.generate(server_name, tools)
    }

    /// Generate TypeScript clients for all connected servers
    pub fn generate_all_clients(&mut self) -> Vec<(String, Result<PathBuf>)> {
        let server_names: Vec<String> = self
            .pool
            .connected_servers()
            .iter()
            .map(|s| s.to_string())
            .collect();

        server_names
            .into_iter()
            .map(|name| {
                let result = self.generate_client(&name);
                (name, result)
            })
            .collect()
    }

    /// Post a tool call result to the tuple space
    pub fn post_response(&self, response: &ToolCallResponse) {
        let json = serde_json::to_string(response).unwrap_or_default();
        self.tuple_space.out(colony_tuple::Tuple::new(vec![
            colony_tuple::Value::Str("mcp_response".to_string()),
            colony_tuple::Value::Str(response.id.clone()),
            colony_tuple::Value::Str(json),
        ]));
    }

    /// Post a tool call request to the tuple space (for approval workflow)
    pub fn post_approval_request(&self, request: &ToolCallRequest) {
        let json = serde_json::to_string(request).unwrap_or_default();
        self.tuple_space.out(colony_tuple::Tuple::new(vec![
            colony_tuple::Value::Str("mcp_approval_request".to_string()),
            colony_tuple::Value::Str(request.id.clone()),
            colony_tuple::Value::Str(json),
        ]));
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &McpConnectionPool {
        &self.pool
    }

    /// Get a mutable reference to the connection pool
    pub fn pool_mut(&mut self) -> &mut McpConnectionPool {
        &mut self.pool
    }

    /// Get a reference to the codegen instance
    pub fn codegen(&self) -> &McpCodegen {
        &self.codegen
    }
}

impl std::fmt::Debug for McpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpSession")
            .field("pool", &self.pool)
            .field("codegen", &self.codegen)
            .field("classifications", &self.classifications.len())
            .finish()
    }
}
