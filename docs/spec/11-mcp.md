# Part 11: MCP Integration

## 11.1 Overview

Colony integrates with the Model Context Protocol (MCP) to connect to external tool servers. This allows agents and commands to access tools, resources, and prompts from MCP servers.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Colony Shell                                   │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐                                                        │
│  │   Agent     │───┐                                                    │
│  └─────────────┘   │                                                    │
│  ┌─────────────┐   │    ┌─────────────┐    ┌─────────────────────────┐ │
│  │   Command   │───┼───▶│ MCP Manager │───▶│   MCP Servers           │ │
│  └─────────────┘   │    └─────────────┘    │  ┌─────────────────────┐│ │
│  ┌─────────────┐   │                       │  │ filesystem-server   ││ │
│  │   Script    │───┘                       │  │ git-server          ││ │
│  └─────────────┘                           │  │ database-server     ││ │
│                                            │  │ web-search-server   ││ │
│                                            │  └─────────────────────┘│ │
│                                            └─────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

## 11.2 MCP Client

```rust
// crates/colony-mcp/src/client.rs

use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// MCP client for communicating with a server
pub struct McpClient {
    /// Server name
    name: String,
    /// Transport
    transport: McpTransport,
    /// Server capabilities
    capabilities: ServerCapabilities,
    /// Available tools
    tools: Vec<ToolDefinition>,
    /// Available resources
    resources: Vec<ResourceDefinition>,
    /// Request ID counter
    request_id: AtomicU64,
}

/// Transport mechanism
enum McpTransport {
    /// Stdio transport (local process)
    Stdio {
        child: Child,
        stdin: tokio::process::ChildStdin,
        stdout: BufReader<tokio::process::ChildStdout>,
    },
    /// HTTP transport
    Http {
        client: reqwest::Client,
        base_url: String,
    },
    /// Server-Sent Events transport
    Sse {
        client: reqwest::Client,
        base_url: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
    pub resources: Option<ResourcesCapability>,
    pub prompts: Option<PromptsCapability>,
    pub logging: Option<LoggingCapability>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolsCapability {
    pub list_changed: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourcesCapability {
    pub subscribe: Option<bool>,
    pub list_changed: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptsCapability {
    pub list_changed: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoggingCapability {}

impl McpClient {
    /// Connect to a stdio-based MCP server
    pub async fn connect_stdio(
        name: &str,
        command: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<Self, McpError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());
        
        for (key, value) in env {
            cmd.env(key, value);
        }
        
        let mut child = cmd.spawn()
            .map_err(|e| McpError::SpawnFailed(command.to_string(), e))?;
        
        let stdin = child.stdin.take()
            .ok_or(McpError::StdinNotAvailable)?;
        let stdout = child.stdout.take()
            .ok_or(McpError::StdoutNotAvailable)?;
        
        let mut client = Self {
            name: name.to_string(),
            transport: McpTransport::Stdio {
                child,
                stdin,
                stdout: BufReader::new(stdout),
            },
            capabilities: ServerCapabilities::default(),
            tools: Vec::new(),
            resources: Vec::new(),
            request_id: AtomicU64::new(1),
        };
        
        // Initialize the connection
        client.initialize().await?;
        
        Ok(client)
    }
    
    /// Connect to an HTTP-based MCP server
    pub async fn connect_http(name: &str, base_url: &str) -> Result<Self, McpError> {
        let client = reqwest::Client::new();
        
        let mut mcp_client = Self {
            name: name.to_string(),
            transport: McpTransport::Http {
                client,
                base_url: base_url.to_string(),
            },
            capabilities: ServerCapabilities::default(),
            tools: Vec::new(),
            resources: Vec::new(),
            request_id: AtomicU64::new(1),
        };
        
        mcp_client.initialize().await?;
        
        Ok(mcp_client)
    }
    
    /// Initialize the MCP connection
    async fn initialize(&mut self) -> Result<(), McpError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id(),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "roots": { "listChanged": true },
                    "sampling": {}
                },
                "clientInfo": {
                    "name": "colony-shell",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        };
        
        let response: InitializeResponse = self.send_request(request).await?;
        
        self.capabilities = response.capabilities;
        
        // Send initialized notification
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "notifications/initialized".to_string(),
            params: None,
        };
        
        self.send_notification(notification).await?;
        
        // List tools if supported
        if self.capabilities.tools.is_some() {
            self.refresh_tools().await?;
        }
        
        // List resources if supported
        if self.capabilities.resources.is_some() {
            self.refresh_resources().await?;
        }
        
        Ok(())
    }
    
    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }
    
    /// Send a JSON-RPC request and wait for response
    async fn send_request<T: DeserializeOwned>(&mut self, request: JsonRpcRequest) -> Result<T, McpError> {
        let request_json = serde_json::to_string(&request)?;
        
        match &mut self.transport {
            McpTransport::Stdio { stdin, stdout, .. } => {
                // Write request
                stdin.write_all(request_json.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                stdin.flush().await?;
                
                // Read response
                let mut line = String::new();
                stdout.read_line(&mut line).await?;
                
                let response: JsonRpcResponse<T> = serde_json::from_str(&line)?;
                
                if let Some(error) = response.error {
                    return Err(McpError::ServerError(error.code, error.message));
                }
                
                response.result.ok_or(McpError::NoResult)
            }
            
            McpTransport::Http { client, base_url } => {
                let response = client
                    .post(base_url)
                    .json(&request)
                    .send()
                    .await?;
                
                let rpc_response: JsonRpcResponse<T> = response.json().await?;
                
                if let Some(error) = rpc_response.error {
                    return Err(McpError::ServerError(error.code, error.message));
                }
                
                rpc_response.result.ok_or(McpError::NoResult)
            }
            
            McpTransport::Sse { .. } => {
                // SSE typically uses POST for requests, GET for events
                unimplemented!("SSE request handling")
            }
        }
    }
    
    /// Send a notification (no response expected)
    async fn send_notification(&mut self, notification: JsonRpcNotification) -> Result<(), McpError> {
        let notification_json = serde_json::to_string(&notification)?;
        
        match &mut self.transport {
            McpTransport::Stdio { stdin, .. } => {
                stdin.write_all(notification_json.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                stdin.flush().await?;
                Ok(())
            }
            McpTransport::Http { client, base_url } => {
                client
                    .post(base_url)
                    .json(&notification)
                    .send()
                    .await?;
                Ok(())
            }
            McpTransport::Sse { .. } => {
                unimplemented!()
            }
        }
    }
    
    /// Refresh the list of available tools
    pub async fn refresh_tools(&mut self) -> Result<(), McpError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id(),
            method: "tools/list".to_string(),
            params: None,
        };
        
        let response: ToolsListResponse = self.send_request(request).await?;
        self.tools = response.tools;
        
        Ok(())
    }
    
    /// Refresh the list of available resources
    pub async fn refresh_resources(&mut self) -> Result<(), McpError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id(),
            method: "resources/list".to_string(),
            params: None,
        };
        
        let response: ResourcesListResponse = self.send_request(request).await?;
        self.resources = response.resources;
        
        Ok(())
    }
    
    /// Get available tools
    pub fn tools(&self) -> &[ToolDefinition] {
        &self.tools
    }
    
    /// Get available resources
    pub fn resources(&self) -> &[ResourceDefinition] {
        &self.resources
    }
    
    /// Call a tool
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, McpError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id(),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": name,
                "arguments": arguments
            })),
        };
        
        let response: ToolCallResponse = self.send_request(request).await?;
        
        // Convert content to Value
        let result = if response.content.len() == 1 {
            content_to_value(&response.content[0])
        } else {
            Value::List(response.content.iter().map(content_to_value).collect())
        };
        
        Ok(result)
    }
    
    /// Read a resource
    pub async fn read_resource(&mut self, uri: &str) -> Result<ResourceContent, McpError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id(),
            method: "resources/read".to_string(),
            params: Some(serde_json::json!({
                "uri": uri
            })),
        };
        
        let response: ResourceReadResponse = self.send_request(request).await?;
        
        Ok(response.contents.into_iter().next().unwrap_or_default())
    }
    
    /// Subscribe to resource updates
    pub async fn subscribe_resource(&mut self, uri: &str) -> Result<(), McpError> {
        if self.capabilities.resources.as_ref().and_then(|r| r.subscribe).unwrap_or(false) {
            let request = JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: self.next_id(),
                method: "resources/subscribe".to_string(),
                params: Some(serde_json::json!({
                    "uri": uri
                })),
            };
            
            let _response: EmptyResponse = self.send_request(request).await?;
        }
        
        Ok(())
    }
    
    /// Disconnect from the server
    pub async fn disconnect(&mut self) -> Result<(), McpError> {
        match &mut self.transport {
            McpTransport::Stdio { child, .. } => {
                child.kill().await?;
            }
            _ => {}
        }
        Ok(())
    }
}

fn content_to_value(content: &ToolContent) -> Value {
    match content {
        ToolContent::Text { text } => Value::Str(text.clone()),
        ToolContent::Image { data, mime_type } => {
            let mut d = IndexMap::new();
            d.insert("type".to_string(), Value::Str("image".to_string()));
            d.insert("data".to_string(), Value::Str(data.clone()));
            d.insert("mimeType".to_string(), Value::Str(mime_type.clone()));
            Value::Dict(d)
        }
        ToolContent::Resource { uri, text, blob } => {
            let mut d = IndexMap::new();
            d.insert("type".to_string(), Value::Str("resource".to_string()));
            d.insert("uri".to_string(), Value::Str(uri.clone()));
            if let Some(t) = text {
                d.insert("text".to_string(), Value::Str(t.clone()));
            }
            if let Some(b) = blob {
                d.insert("blob".to_string(), Value::Str(b.clone()));
            }
            Value::Dict(d)
        }
    }
}

// JSON-RPC types

#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

#[derive(Deserialize)]
struct InitializeResponse {
    #[allow(dead_code)]
    protocol_version: String,
    capabilities: ServerCapabilities,
    #[allow(dead_code)]
    server_info: ServerInfo,
}

#[derive(Deserialize)]
struct ServerInfo {
    name: String,
    version: Option<String>,
}

#[derive(Deserialize)]
struct ToolsListResponse {
    tools: Vec<ToolDefinition>,
}

#[derive(Deserialize)]
struct ResourcesListResponse {
    resources: Vec<ResourceDefinition>,
}

#[derive(Deserialize)]
struct ToolCallResponse {
    content: Vec<ToolContent>,
    #[serde(rename = "isError")]
    is_error: Option<bool>,
}

#[derive(Deserialize)]
struct ResourceReadResponse {
    contents: Vec<ResourceContent>,
}

#[derive(Deserialize)]
struct EmptyResponse {}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("Failed to spawn server {0}: {1}")]
    SpawnFailed(String, std::io::Error),
    #[error("Stdin not available")]
    StdinNotAvailable,
    #[error("Stdout not available")]
    StdoutNotAvailable,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Server error ({0}): {1}")]
    ServerError(i32, String),
    #[error("No result in response")]
    NoResult,
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
}
```

## 11.3 Tool and Resource Definitions

```rust
// crates/colony-mcp/src/types.rs

/// Tool definition from MCP server
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    /// Convert to format suitable for LLM
    pub fn to_llm_format(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "parameters": self.input_schema
        })
    }
}

/// Resource definition from MCP server
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceDefinition {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// Tool content in response
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, #[serde(rename = "mimeType")] mime_type: String },
    #[serde(rename = "resource")]
    Resource {
        uri: String,
        text: Option<String>,
        blob: Option<String>,
    },
}

/// Resource content
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub blob: Option<String>,
}
```

## 11.4 MCP Manager

```rust
// crates/colony-mcp/src/manager.rs

/// Manages multiple MCP server connections
pub struct McpManager {
    clients: RwLock<IndexMap<String, McpClient>>,
    config: McpConfig,
}

#[derive(Clone, Debug, Default)]
pub struct McpConfig {
    /// Auto-connect servers on startup
    pub auto_connect: Vec<McpServerConfig>,
    /// Default timeout for operations
    pub timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct McpServerConfig {
    pub name: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub url: Option<String>,
    pub transport: McpTransportType,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum McpTransportType {
    #[default]
    Stdio,
    Http,
    Sse,
}

impl McpManager {
    pub fn new(config: McpConfig) -> Arc<Self> {
        Arc::new(Self {
            clients: RwLock::new(IndexMap::new()),
            config,
        })
    }
    
    /// Initialize and connect to configured servers
    pub async fn initialize(self: &Arc<Self>) -> Result<(), McpError> {
        for server_config in &self.config.auto_connect {
            if let Err(e) = self.connect(server_config).await {
                eprintln!("Warning: Failed to connect to MCP server '{}': {}", 
                    server_config.name, e);
            }
        }
        Ok(())
    }
    
    /// Connect to an MCP server
    pub async fn connect(&self, config: &McpServerConfig) -> Result<(), McpError> {
        let client = match config.transport {
            McpTransportType::Stdio => {
                let command = config.command.as_ref()
                    .ok_or(McpError::ConfigError("command required for stdio".to_string()))?;
                McpClient::connect_stdio(
                    &config.name,
                    command,
                    &config.args,
                    &config.env,
                ).await?
            }
            McpTransportType::Http | McpTransportType::Sse => {
                let url = config.url.as_ref()
                    .ok_or(McpError::ConfigError("url required for http/sse".to_string()))?;
                McpClient::connect_http(&config.name, url).await?
            }
        };
        
        let mut clients = self.clients.write().await;
        clients.insert(config.name.clone(), client);
        
        Ok(())
    }
    
    /// Disconnect from a server
    pub async fn disconnect(&self, name: &str) -> Result<(), McpError> {
        let mut clients = self.clients.write().await;
        if let Some(mut client) = clients.remove(name) {
            client.disconnect().await?;
        }
        Ok(())
    }
    
    /// List connected servers
    pub async fn list_servers(&self) -> Vec<String> {
        let clients = self.clients.read().await;
        clients.keys().cloned().collect()
    }
    
    /// Get tools from a specific server
    pub async fn get_tools(&self, server: &str) -> Result<Vec<ToolDefinition>, McpError> {
        let clients = self.clients.read().await;
        let client = clients.get(server)
            .ok_or(McpError::ServerNotConnected(server.to_string()))?;
        Ok(client.tools().to_vec())
    }
    
    /// Get all tools from all servers
    pub async fn get_all_tools(&self) -> Vec<(String, ToolDefinition)> {
        let clients = self.clients.read().await;
        let mut tools = Vec::new();
        
        for (server, client) in clients.iter() {
            for tool in client.tools() {
                tools.push((server.clone(), tool.clone()));
            }
        }
        
        tools
    }
    
    /// Find a tool by name (searches all servers)
    pub async fn find_tool(&self, name: &str) -> Option<(String, ToolDefinition)> {
        let clients = self.clients.read().await;
        
        for (server, client) in clients.iter() {
            if let Some(tool) = client.tools().iter().find(|t| t.name == name) {
                return Some((server.clone(), tool.clone()));
            }
        }
        
        None
    }
    
    /// Call a tool
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Option<Value>, McpError> {
        // Find which server has this tool
        let (server, _tool) = self.find_tool(name).await
            .ok_or(McpError::ToolNotFound(name.to_string()))?;
        
        let mut clients = self.clients.write().await;
        let client = clients.get_mut(&server)
            .ok_or(McpError::ServerNotConnected(server.clone()))?;
        
        let result = client.call_tool(name, arguments).await?;
        Ok(Some(result))
    }
    
    /// Get resources from a specific server
    pub async fn get_resources(&self, server: &str) -> Result<Vec<ResourceDefinition>, McpError> {
        let clients = self.clients.read().await;
        let client = clients.get(server)
            .ok_or(McpError::ServerNotConnected(server.to_string()))?;
        Ok(client.resources().to_vec())
    }
    
    /// Read a resource
    pub async fn read_resource(&self, server: &str, uri: &str) -> Result<ResourceContent, McpError> {
        let mut clients = self.clients.write().await;
        let client = clients.get_mut(server)
            .ok_or(McpError::ServerNotConnected(server.to_string()))?;
        
        client.read_resource(uri).await
    }
}

impl McpError {
    fn ConfigError(msg: String) -> Self {
        // Would add a ConfigError variant
        McpError::ServerError(-1, msg)
    }
    
    fn ServerNotConnected(name: String) -> Self {
        McpError::ServerError(-2, format!("Server not connected: {}", name))
    }
}
```

## 11.5 MCP Commands

```colony
# Connect to an MCP server
mcp:connect filesystem --command "npx" --args ["-y", "@anthropic/mcp-filesystem"]
mcp:connect git --command "mcp-git-server"
mcp:connect database --url "http://localhost:3000/mcp"

# List connected servers
mcp:servers

# List tools from a server
mcp:tools filesystem
mcp:tools --all  # From all servers

# List resources
mcp:resources filesystem

# Call a tool directly
mcp:call filesystem:read_file path="/etc/hosts"
mcp:call git:status

# Shorthand (auto-discovers server)
mcp read_file path="/etc/hosts"
mcp git_status

# Read a resource
mcp:read filesystem "file:///home/user/project/README.md"

# Use in pipeline
mcp:call search:query q="rust async" -> jq '.results'

# Disconnect
mcp:disconnect filesystem

# Server info
mcp:info filesystem

# Refresh tool/resource lists
mcp:refresh filesystem
mcp:refresh --all
```

## 11.6 MCP Configuration

```toml
# ~/.config/colony/mcp.toml

# Auto-connect servers
[[servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@anthropic/mcp-filesystem", "/home/user"]
transport = "stdio"

[[servers]]
name = "git"
command = "mcp-git-server"
args = []
env = [["GIT_DIR", "/home/user/project"]]
transport = "stdio"

[[servers]]
name = "database"
url = "http://localhost:3000/mcp"
transport = "http"

[[servers]]
name = "web-search"
url = "http://search.local/sse"
transport = "sse"

[defaults]
timeout = 30  # seconds
auto_refresh = true
```

## 11.7 Tool Registry Integration

```rust
// crates/colony-agents/src/tools.rs

/// Unified tool registry (builtin + MCP)
pub struct ToolRegistry {
    /// Builtin tools
    builtin: IndexMap<String, Arc<dyn Tool>>,
    /// MCP manager
    mcp: Arc<McpManager>,
}

impl ToolRegistry {
    pub fn new(mcp: Arc<McpManager>) -> Self {
        let mut registry = Self {
            builtin: IndexMap::new(),
            mcp,
        };
        
        // Register builtin tools
        registry.register_builtin("shell", Arc::new(ShellTool));
        registry.register_builtin("read_file", Arc::new(ReadFileTool));
        registry.register_builtin("write_file", Arc::new(WriteFileTool));
        registry.register_builtin("http", Arc::new(HttpTool));
        
        registry
    }
    
    fn register_builtin(&mut self, name: &str, tool: Arc<dyn Tool>) {
        self.builtin.insert(name.to_string(), tool);
    }
    
    /// Get a tool by name
    pub async fn get(&self, name: &str) -> Option<ToolHandle> {
        // Check builtin first
        if let Some(tool) = self.builtin.get(name) {
            return Some(ToolHandle::Builtin(tool.clone()));
        }
        
        // Check MCP
        if let Some((server, tool_def)) = self.mcp.find_tool(name).await {
            return Some(ToolHandle::Mcp {
                server,
                definition: tool_def,
                mcp: self.mcp.clone(),
            });
        }
        
        None
    }
    
    /// Get all available tools
    pub async fn all_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = Vec::new();
        
        // Builtin tools
        for (name, tool) in &self.builtin {
            tools.push(ToolDefinition {
                name: name.clone(),
                description: Some(tool.description().to_string()),
                input_schema: tool.input_schema(),
            });
        }
        
        // MCP tools
        for (_, tool) in self.mcp.get_all_tools().await {
            tools.push(tool);
        }
        
        tools
    }
    
    /// Match tools by pattern
    pub async fn match_pattern(&self, pattern: &str) -> Vec<ToolDefinition> {
        let all = self.all_tools().await;
        
        if pattern == "*" {
            return all;
        }
        
        let regex = glob_to_regex(pattern);
        all.into_iter()
            .filter(|t| regex.is_match(&t.name))
            .collect()
    }
}

pub enum ToolHandle {
    Builtin(Arc<dyn Tool>),
    Mcp {
        server: String,
        definition: ToolDefinition,
        mcp: Arc<McpManager>,
    },
}

impl ToolHandle {
    pub async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        match self {
            ToolHandle::Builtin(tool) => tool.execute(args).await,
            ToolHandle::Mcp { server, definition, mcp } => {
                mcp.call_tool(&definition.name, args).await
                    .map_err(|e| ToolError::McpError(e.to_string()))?
                    .ok_or(ToolError::NoResult)
            }
        }
    }
    
    pub fn definition(&self) -> ToolDefinition {
        match self {
            ToolHandle::Builtin(tool) => ToolDefinition {
                name: tool.name().to_string(),
                description: Some(tool.description().to_string()),
                input_schema: tool.input_schema(),
            },
            ToolHandle::Mcp { definition, .. } => definition.clone(),
        }
    }
}

/// Trait for builtin tools
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: Value) -> Result<Value, ToolError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),
    #[error("Invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("MCP error: {0}")]
    McpError(String),
    #[error("No result")]
    NoResult,
}
```

---

*Continue to Part 12: Special Files*