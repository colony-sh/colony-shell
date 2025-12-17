# Part 9: Agent System

## 9.1 Overview

Agents are first-class citizens in Colony, designed for AI-assisted workflows. An agent combines:
- A prompt template
- Tool capabilities
- Input/output schema
- Hooks for customization
- Session management

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Agent Execution                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                 │
│  │   Input     │───▶│   Context   │───▶│   Prompt    │                 │
│  │  (params)   │    │  Building   │    │  Assembly   │                 │
│  └─────────────┘    └─────────────┘    └──────┬──────┘                 │
│                                               │                         │
│                                               ▼                         │
│                     ┌────────────────────────────────────┐             │
│                     │           LLM Call                  │             │
│                     │  ┌──────────────────────────────┐  │             │
│                     │  │    Tool Loop                 │  │             │
│                     │  │  ┌─────┐  ┌──────┐  ┌─────┐ │  │             │
│                     │  │  │Call │─▶│Result│─▶│Next │ │  │             │
│                     │  │  └─────┘  └──────┘  └─────┘ │  │             │
│                     │  └──────────────────────────────┘  │             │
│                     └─────────────────┬──────────────────┘             │
│                                       │                                 │
│                                       ▼                                 │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                 │
│  │   Output    │◀───│   Hooks     │◀───│  Response   │                 │
│  │  (result)   │    │ (on_complete)│    │  Parsing   │                 │
│  └─────────────┘    └─────────────┘    └─────────────┘                 │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## 9.2 Agent Definition

```rust
// crates/colony-agents/src/definition.rs

use colony_core::types::{Value, TypeAnnotation, PipelineType};
use std::sync::Arc;

/// Complete agent definition
#[derive(Clone, Debug)]
pub struct AgentDef {
    /// Agent identifier
    pub name: String,
    /// Human-readable description
    pub description: Option<String>,
    /// Version string
    pub version: Option<String>,
    /// Agent capabilities
    pub capabilities: AgentCapabilities,
    /// Input parameters
    pub input_params: Vec<AgentParam>,
    /// Expected output type
    pub output_type: Option<TypeAnnotation>,
    /// The prompt template
    pub prompt: PromptTemplate,
    /// Lifecycle hooks
    pub hooks: AgentHooks,
    /// Source location (for error reporting)
    pub source: Option<SourceLocation>,
}

/// Agent capabilities and constraints
#[derive(Clone, Debug, Default)]
pub struct AgentCapabilities {
    /// Available tools (by name or pattern)
    pub tools: Vec<ToolSpec>,
    /// MCP servers to connect
    pub mcp_servers: Vec<McpServerSpec>,
    /// Maximum tokens for response
    pub max_tokens: Option<u32>,
    /// Temperature for sampling
    pub temperature: Option<f32>,
    /// Specific model to use
    pub model: Option<String>,
    /// Context access permissions
    pub context: ContextCapabilities,
    /// Maximum tool call iterations
    pub max_iterations: Option<u32>,
    /// Timeout in seconds
    pub timeout_secs: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ToolSpec {
    /// Tool name (can include wildcards)
    pub pattern: String,
    /// Tool description override
    pub description: Option<String>,
    /// Whether tool requires confirmation
    pub requires_confirmation: bool,
}

#[derive(Clone, Debug)]
pub struct McpServerSpec {
    /// Server name
    pub name: String,
    /// Server command
    pub command: Option<String>,
    /// Server arguments
    pub args: Vec<String>,
    /// Environment variables
    pub env: Vec<(String, String)>,
    /// Server URL (for remote servers)
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ContextCapabilities {
    /// Can read from context channel
    pub read: bool,
    /// Can write to context channel
    pub write: bool,
    /// Allowed context types
    pub types: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AgentParam {
    /// Parameter name
    pub name: String,
    /// Type annotation
    pub type_annotation: Option<TypeAnnotation>,
    /// Default value
    pub default: Option<Value>,
    /// Description (for help)
    pub description: Option<String>,
    /// Whether required
    pub required: bool,
}

/// Prompt template with variable interpolation
#[derive(Clone, Debug)]
pub enum PromptTemplate {
    /// Literal string
    Literal(String),
    /// Template with {variable} placeholders
    Template {
        template: String,
        variables: Vec<String>,
    },
    /// Load from file
    File(String),
    /// Multiple prompt components
    Composite {
        system: Option<Box<PromptTemplate>>,
        user: Box<PromptTemplate>,
        prefix: Option<Box<PromptTemplate>>,
    },
}

impl PromptTemplate {
    /// Render template with given values
    pub fn render(&self, values: &IndexMap<String, Value>) -> Result<String, PromptError> {
        match self {
            PromptTemplate::Literal(s) => Ok(s.clone()),
            
            PromptTemplate::Template { template, variables } => {
                let mut result = template.clone();
                
                for var in variables {
                    let placeholder = format!("{{{}}}", var);
                    let value = values.get(var)
                        .ok_or_else(|| PromptError::MissingVariable(var.clone()))?;
                    result = result.replace(&placeholder, &value.to_string());
                }
                
                Ok(result)
            }
            
            PromptTemplate::File(path) => {
                std::fs::read_to_string(path)
                    .map_err(|e| PromptError::FileRead(path.clone(), e))
            }
            
            PromptTemplate::Composite { system, user, prefix } => {
                let mut parts = Vec::new();
                
                if let Some(sys) = system {
                    parts.push(format!("[SYSTEM]\n{}", sys.render(values)?));
                }
                
                parts.push(user.render(values)?);
                
                if let Some(pre) = prefix {
                    parts.push(pre.render(values)?);
                }
                
                Ok(parts.join("\n\n"))
            }
        }
    }
}

/// Agent lifecycle hooks
#[derive(Clone, Debug, Default)]
pub struct AgentHooks {
    /// Called when agent starts
    pub on_start: Option<Arc<HookFn>>,
    /// Called before each tool call
    pub on_tool_call: Option<Arc<ToolCallHookFn>>,
    /// Called after each LLM message
    pub on_message: Option<Arc<MessageHookFn>>,
    /// Called when agent completes successfully
    pub on_complete: Option<Arc<CompleteHookFn>>,
    /// Called when agent encounters an error
    pub on_error: Option<Arc<ErrorHookFn>>,
}

// Hook function types
pub type HookFn = dyn Fn(&mut AgentContext) -> Result<(), AgentError> + Send + Sync;
pub type ToolCallHookFn = dyn Fn(&mut AgentContext, &str, &Value) -> Result<HookAction, AgentError> + Send + Sync;
pub type MessageHookFn = dyn Fn(&mut AgentContext, &Message) -> Result<(), AgentError> + Send + Sync;
pub type CompleteHookFn = dyn Fn(&mut AgentContext, &Value) -> Result<Value, AgentError> + Send + Sync;
pub type ErrorHookFn = dyn Fn(&mut AgentContext, &AgentError) -> Result<ErrorAction, AgentError> + Send + Sync;

/// Action to take after tool call hook
#[derive(Clone, Debug)]
pub enum HookAction {
    /// Continue with the tool call
    Continue,
    /// Skip this tool call
    Skip,
    /// Abort the agent
    Abort(String),
    /// Replace tool arguments
    Replace(Value),
}

/// Action to take after error hook
#[derive(Clone, Debug)]
pub enum ErrorAction {
    /// Propagate the error
    Propagate,
    /// Retry the operation
    Retry,
    /// Return a default value
    Default(Value),
    /// Ignore and continue
    Ignore,
}

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("Missing variable: {0}")]
    MissingVariable(String),
    #[error("Failed to read file {0}: {1}")]
    FileRead(String, std::io::Error),
}
```

## 9.3 Agent Executor

```rust
// crates/colony-agents/src/executor.rs

use tokio::time::{timeout, Duration};

/// Executes agents
pub struct AgentExecutor {
    /// LLM client
    llm_client: Arc<dyn LlmClient>,
    /// Tool registry
    tool_registry: Arc<ToolRegistry>,
    /// MCP client manager
    mcp_manager: Arc<McpManager>,
    /// Channel manager reference
    channels: Arc<RwLock<ChannelManager>>,
}

impl AgentExecutor {
    pub fn new(
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<ToolRegistry>,
        mcp_manager: Arc<McpManager>,
        channels: Arc<RwLock<ChannelManager>>,
    ) -> Self {
        Self {
            llm_client,
            tool_registry,
            mcp_manager,
            channels,
        }
    }
    
    /// Execute an agent with given inputs
    pub async fn execute(
        &self,
        agent: &AgentDef,
        inputs: IndexMap<String, Value>,
    ) -> Result<AgentResult, AgentError> {
        // Create execution context
        let mut ctx = AgentContext::new(agent, inputs.clone());
        
        // Apply timeout if specified
        let timeout_duration = agent.capabilities.timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(300)); // 5 min default
        
        let result = timeout(timeout_duration, self.execute_inner(agent, &mut ctx))
            .await
            .map_err(|_| AgentError::Timeout)?;
        
        result
    }
    
    async fn execute_inner(
        &self,
        agent: &AgentDef,
        ctx: &mut AgentContext,
    ) -> Result<AgentResult, AgentError> {
        // Call on_start hook
        if let Some(ref hook) = agent.hooks.on_start {
            hook(ctx)?;
        }
        
        // Build context from channels
        let session_context = self.build_context(agent, ctx).await?;
        ctx.session_context = session_context;
        
        // Render prompt
        let prompt = agent.prompt.render(&ctx.inputs)?;
        
        // Build tool list
        let tools = self.resolve_tools(agent).await?;
        
        // Initialize conversation
        ctx.conversation.add_user(&prompt);
        
        // Write to LLM channel
        self.write_llm_channel(Role::User, &prompt, Some(&agent.name)).await;
        
        // Tool loop
        let max_iterations = agent.capabilities.max_iterations.unwrap_or(10);
        let mut iteration = 0;
        
        loop {
            iteration += 1;
            if iteration > max_iterations {
                return Err(AgentError::MaxIterationsExceeded(max_iterations));
            }
            
            // Call LLM
            let request = LlmRequest {
                messages: ctx.conversation.messages_for_api(),
                tools: tools.clone(),
                max_tokens: agent.capabilities.max_tokens,
                temperature: agent.capabilities.temperature,
                model: agent.capabilities.model.clone(),
            };
            
            let response = self.llm_client.complete(&request).await?;
            
            // Process response
            ctx.conversation.add_assistant(&response.content, response.tool_calls.clone());
            
            // Write to LLM channel
            self.write_llm_channel(Role::Assistant, &response.content, Some(&agent.name)).await;
            
            // Call on_message hook
            if let Some(ref hook) = agent.hooks.on_message {
                let msg = Message {
                    role: Role::Assistant,
                    content: response.content.clone(),
                    ..Default::default()
                };
                hook(ctx, &msg)?;
            }
            
            // Handle tool calls
            if response.tool_calls.is_empty() {
                // No more tool calls, we're done
                break;
            }
            
            for tool_call in &response.tool_calls {
                let result = self.execute_tool_call(agent, ctx, tool_call).await?;
                ctx.conversation.add_tool_result(&tool_call.id, result);
            }
        }
        
        // Parse final result
        let result_value = self.parse_result(&response.content, &agent.output_type)?;
        
        // Call on_complete hook
        let final_value = if let Some(ref hook) = agent.hooks.on_complete {
            hook(ctx, &result_value)?
        } else {
            result_value
        };
        
        // Write to context channel
        self.write_context(
            ContextType::Action,
            &format!("Agent '{}' completed", agent.name),
            &agent.name,
        ).await;
        
        Ok(AgentResult {
            value: final_value,
            conversation: ctx.conversation.clone(),
            tool_calls: ctx.tool_calls.clone(),
            context_entries: ctx.context_entries.clone(),
        })
    }
    
    async fn build_context(&self, agent: &AgentDef, ctx: &mut AgentContext) -> Result<String, AgentError> {
        if !agent.capabilities.context.read {
            return Ok(String::new());
        }
        
        let channels = self.channels.read().await;
        let context_channel = channels.context()
            .ok_or(AgentError::ChannelNotAvailable("context".to_string()))?;
        
        // Filter context by allowed types
        let filter = ContextFilter {
            entry_types: if agent.capabilities.context.types.is_empty() {
                None
            } else {
                Some(agent.capabilities.context.types.iter()
                    .map(|t| ContextType::from_str(t))
                    .collect())
            },
            ..Default::default()
        };
        
        let entries = context_channel.query(&filter);
        
        // Format for LLM
        let formatted = entries.iter()
            .map(|e| format!("[{}] {}", e.entry_type.as_str(), e.content))
            .collect::<Vec<_>>()
            .join("\n");
        
        Ok(formatted)
    }
    
    async fn resolve_tools(&self, agent: &AgentDef) -> Result<Vec<ToolDefinition>, AgentError> {
        let mut tools = Vec::new();
        
        // Add from tool registry
        for spec in &agent.capabilities.tools {
            let matching = self.tool_registry.match_pattern(&spec.pattern);
            for tool in matching {
                let mut def = tool.clone();
                if let Some(ref desc) = spec.description {
                    def.description = desc.clone();
                }
                tools.push(def);
            }
        }
        
        // Add from MCP servers
        for server_spec in &agent.capabilities.mcp_servers {
            let server_tools = self.mcp_manager.get_tools(&server_spec.name).await?;
            tools.extend(server_tools);
        }
        
        Ok(tools)
    }
    
    async fn execute_tool_call(
        &self,
        agent: &AgentDef,
        ctx: &mut AgentContext,
        tool_call: &ToolCall,
    ) -> Result<Value, AgentError> {
        // Call on_tool_call hook
        let mut args = tool_call.arguments.clone();
        
        if let Some(ref hook) = agent.hooks.on_tool_call {
            match hook(ctx, &tool_call.name, &args)? {
                HookAction::Continue => {}
                HookAction::Skip => return Ok(Value::Null),
                HookAction::Abort(msg) => return Err(AgentError::Aborted(msg)),
                HookAction::Replace(new_args) => args = new_args,
            }
        }
        
        // Write to tool channel
        let call_id = self.write_tool_call(&tool_call.name, &args, Some(&agent.name)).await;
        
        let start = std::time::Instant::now();
        
        // Execute the tool
        let result = if let Some(tool) = self.tool_registry.get(&tool_call.name) {
            tool.execute(args.clone()).await
        } else if let Some(result) = self.mcp_manager.call_tool(&tool_call.name, args.clone()).await? {
            Ok(result)
        } else {
            Err(ToolError::NotFound(tool_call.name.clone()))
        };
        
        let duration = start.elapsed();
        
        // Update tool channel
        match &result {
            Ok(value) => {
                self.write_tool_result(&call_id, value.clone(), duration.as_millis() as u64).await;
            }
            Err(e) => {
                self.write_tool_error(&call_id, &e.to_string(), duration.as_millis() as u64).await;
            }
        }
        
        // Track in context
        ctx.tool_calls.push(ToolCallRecord {
            id: tool_call.id.clone(),
            timestamp: Utc::now(),
            agent: Some(agent.name.clone()),
            tool_name: tool_call.name.clone(),
            arguments: args,
            result: result.as_ref().ok().cloned(),
            error: result.as_ref().err().map(|e| e.to_string()),
            duration_ms: duration.as_millis() as u64,
            status: if result.is_ok() { ToolCallStatus::Success } else { ToolCallStatus::Failed },
        });
        
        result.map_err(AgentError::ToolError)
    }
    
    fn parse_result(&self, content: &str, output_type: &Option<TypeAnnotation>) -> Result<Value, AgentError> {
        // Try to extract structured data based on output type
        if let Some(ref type_ann) = output_type {
            // Try JSON extraction
            if let Some(json_start) = content.find('{') {
                if let Some(json_end) = content.rfind('}') {
                    let json_str = &content[json_start..=json_end];
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                        return Ok(json_to_value(json));
                    }
                }
            }
            
            // Try code block extraction
            if content.contains("```json") {
                let start = content.find("```json").unwrap() + 7;
                if let Some(end) = content[start..].find("```") {
                    let json_str = content[start..start+end].trim();
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                        return Ok(json_to_value(json));
                    }
                }
            }
        }
        
        // Default to string
        Ok(Value::Str(content.to_string()))
    }
    
    // Channel writing helpers
    async fn write_llm_channel(&self, role: Role, content: &str, agent: Option<&str>) {
        let mut channels = self.channels.write().await;
        if let Some(llm) = channels.llm_mut() {
            llm.add_message_with_agent(role, content, agent);
        }
    }
    
    async fn write_context(&self, entry_type: ContextType, content: &str, agent: &str) {
        let mut channels = self.channels.write().await;
        if let Some(ctx) = channels.context_mut() {
            ctx.add_with_source(entry_type, content, ContextSource {
                agent: Some(agent.to_string()),
                ..Default::default()
            });
        }
    }
    
    async fn write_tool_call(&self, tool: &str, args: &Value, agent: Option<&str>) -> String {
        let mut channels = self.channels.write().await;
        if let Some(tool_channel) = channels.get_mut(STDTOOL) {
            if let Some(tc) = tool_channel.as_any_mut().downcast_mut::<ToolChannel>() {
                return tc.record_call(tool, args.clone(), agent);
            }
        }
        String::new()
    }
    
    async fn write_tool_result(&self, call_id: &str, result: Value, duration_ms: u64) {
        let mut channels = self.channels.write().await;
        if let Some(tool_channel) = channels.get_mut(STDTOOL) {
            if let Some(tc) = tool_channel.as_any_mut().downcast_mut::<ToolChannel>() {
                tc.record_result(call_id, result, duration_ms);
            }
        }
    }
    
    async fn write_tool_error(&self, call_id: &str, error: &str, duration_ms: u64) {
        let mut channels = self.channels.write().await;
        if let Some(tool_channel) = channels.get_mut(STDTOOL) {
            if let Some(tc) = tool_channel.as_any_mut().downcast_mut::<ToolChannel>() {
                tc.record_error(call_id, error, duration_ms);
            }
        }
    }
}

/// Context available during agent execution
pub struct AgentContext {
    /// Agent definition
    pub agent: AgentDef,
    /// Input values
    pub inputs: IndexMap<String, Value>,
    /// Session context (from context channel)
    pub session_context: String,
    /// Conversation history
    pub conversation: Conversation,
    /// Tool calls made
    pub tool_calls: Vec<ToolCallRecord>,
    /// Context entries created
    pub context_entries: Vec<ContextEntry>,
    /// Custom state
    pub state: IndexMap<String, Value>,
}

impl AgentContext {
    pub fn new(agent: &AgentDef, inputs: IndexMap<String, Value>) -> Self {
        Self {
            agent: agent.clone(),
            inputs,
            session_context: String::new(),
            conversation: Conversation::new(),
            tool_calls: Vec::new(),
            context_entries: Vec::new(),
            state: IndexMap::new(),
        }
    }
}

/// Result of agent execution
#[derive(Clone, Debug)]
pub struct AgentResult {
    pub value: Value,
    pub conversation: Conversation,
    pub tool_calls: Vec<ToolCallRecord>,
    pub context_entries: Vec<ContextEntry>,
}

/// Agent errors
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Agent timed out")]
    Timeout,
    #[error("Max iterations exceeded: {0}")]
    MaxIterationsExceeded(u32),
    #[error("Aborted: {0}")]
    Aborted(String),
    #[error("Prompt error: {0}")]
    Prompt(#[from] PromptError),
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),
    #[error("Tool error: {0}")]
    ToolError(#[from] ToolError),
    #[error("Channel not available: {0}")]
    ChannelNotAvailable(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}
```

## 9.4 Agent Session Management

```rust
// crates/colony-agents/src/session.rs

/// Manages persistent agent sessions
pub struct SessionManager {
    sessions: IndexMap<String, AgentSession>,
    storage_path: PathBuf,
}

/// A persistent agent session
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub agent_name: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub conversation: Conversation,
    pub state: IndexMap<String, Value>,
    pub metadata: SessionMetadata,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub total_messages: u32,
    pub total_tool_calls: u32,
    pub total_tokens: u32,
}

impl SessionManager {
    pub fn new() -> Self {
        let storage_path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("colony")
            .join("sessions");
        
        Self {
            sessions: IndexMap::new(),
            storage_path,
        }
    }
    
    /// Create a new session
    pub fn create(&mut self, agent_name: &str) -> String {
        let id = generate_id("sess");
        let session = AgentSession {
            id: id.clone(),
            agent_name: agent_name.to_string(),
            created_at: Utc::now(),
            last_active: Utc::now(),
            conversation: Conversation::new(),
            state: IndexMap::new(),
            metadata: SessionMetadata::default(),
        };
        
        self.sessions.insert(id.clone(), session);
        id
    }
    
    /// Get a session
    pub fn get(&self, id: &str) -> Option<&AgentSession> {
        self.sessions.get(id)
    }
    
    /// Get mutable session
    pub fn get_mut(&mut self, id: &str) -> Option<&mut AgentSession> {
        self.sessions.get_mut(id)
    }
    
    /// List sessions for an agent
    pub fn list_for_agent(&self, agent_name: &str) -> Vec<&AgentSession> {
        self.sessions.values()
            .filter(|s| s.agent_name == agent_name)
            .collect()
    }
    
    /// Update session activity
    pub fn touch(&mut self, id: &str) {
        if let Some(session) = self.sessions.get_mut(id) {
            session.last_active = Utc::now();
        }
    }
    
    /// Save session to disk
    pub fn save(&self, id: &str) -> io::Result<()> {
        let session = self.sessions.get(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Session not found"))?;
        
        let path = self.storage_path
            .join(&session.agent_name)
            .join(format!("{}.json", id));
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let json = serde_json::to_string_pretty(session)?;
        fs::write(path, json)?;
        
        Ok(())
    }
    
    /// Load session from disk
    pub fn load(&mut self, agent_name: &str, id: &str) -> io::Result<()> {
        let path = self.storage_path
            .join(agent_name)
            .join(format!("{}.json", id));
        
        let json = fs::read_to_string(&path)?;
        let session: AgentSession = serde_json::from_str(&json)?;
        
        self.sessions.insert(id.to_string(), session);
        
        Ok(())
    }
    
    /// Delete a session
    pub fn delete(&mut self, id: &str) -> io::Result<()> {
        if let Some(session) = self.sessions.remove(id) {
            let path = self.storage_path
                .join(&session.agent_name)
                .join(format!("{}.json", id));
            
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}
```

## 9.5 AGENT.md Parser

```rust
// crates/colony-agents/src/parser.rs

use pulldown_cmark::{Parser, Event, Tag};

/// Parses AGENT.md files into AgentDef
pub struct AgentMarkdownParser;

impl AgentMarkdownParser {
    /// Parse an AGENT.md file
    pub fn parse(content: &str) -> Result<Vec<AgentDef>, ParseError> {
        let parser = Parser::new(content);
        let mut agents = Vec::new();
        let mut current_agent: Option<AgentBuilder> = None;
        let mut current_section = Section::None;
        let mut in_code_block = false;
        let mut code_content = String::new();
        let mut code_lang = String::new();
        let mut table_state = TableState::None;
        
        for event in parser {
            match event {
                Event::Start(Tag::Heading(level, _, _)) if level == 1 || level == 2 => {
                    // New agent definition
                    if let Some(builder) = current_agent.take() {
                        agents.push(builder.build()?);
                    }
                    current_agent = Some(AgentBuilder::new());
                }
                
                Event::Text(text) if current_agent.is_some() => {
                    let builder = current_agent.as_mut().unwrap();
                    
                    if in_code_block {
                        code_content.push_str(&text);
                    } else {
                        match current_section {
                            Section::Name => builder.name = Some(text.to_string()),
                            Section::Description => builder.description = Some(text.to_string()),
                            Section::Prompt => builder.prompt_text.push_str(&text),
                            _ => {}
                        }
                    }
                }
                
                Event::Start(Tag::CodeBlock(kind)) => {
                    in_code_block = true;
                    code_lang = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                        _ => String::new(),
                    };
                }
                
                Event::End(Tag::CodeBlock(_)) => {
                    in_code_block = false;
                    if let Some(builder) = current_agent.as_mut() {
                        match current_section {
                            Section::Prompt => {
                                builder.prompt_text = code_content.clone();
                            }
                            Section::Hooks => {
                                builder.hooks_code = Some((code_lang.clone(), code_content.clone()));
                            }
                            _ => {}
                        }
                    }
                    code_content.clear();
                }
                
                Event::Start(Tag::Table(_)) => {
                    table_state = TableState::Header;
                }
                
                Event::End(Tag::Table(_)) => {
                    table_state = TableState::None;
                }
                
                Event::Start(Tag::TableRow) => {
                    if table_state == TableState::Header {
                        table_state = TableState::Body;
                    }
                }
                
                // Parse table rows for parameters, tools, etc.
                Event::TableCell(text) if table_state == TableState::Body => {
                    if let Some(builder) = current_agent.as_mut() {
                        match current_section {
                            Section::Parameters => {
                                // Parse parameter from table row
                            }
                            Section::Tools => {
                                // Parse tool from table row
                            }
                            _ => {}
                        }
                    }
                }
                
                // Section headers
                Event::Start(Tag::Heading(level, _, _)) if level == 3 => {
                    current_section = Section::None;
                }
                
                Event::Text(text) if matches!(current_section, Section::None) => {
                    let lower = text.to_lowercase();
                    current_section = match lower.as_str() {
                        "description" => Section::Description,
                        "parameters" | "inputs" => Section::Parameters,
                        "prompt" => Section::Prompt,
                        "tools" => Section::Tools,
                        "capabilities" => Section::Capabilities,
                        "hooks" => Section::Hooks,
                        _ => Section::None,
                    };
                }
                
                _ => {}
            }
        }
        
        // Finalize last agent
        if let Some(builder) = current_agent {
            agents.push(builder.build()?);
        }
        
        Ok(agents)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    None,
    Name,
    Description,
    Parameters,
    Prompt,
    Tools,
    Capabilities,
    Hooks,
}

#[derive(Clone, Copy, PartialEq)]
enum TableState {
    None,
    Header,
    Body,
}

struct AgentBuilder {
    name: Option<String>,
    description: Option<String>,
    params: Vec<AgentParam>,
    prompt_text: String,
    tools: Vec<ToolSpec>,
    capabilities: AgentCapabilities,
    hooks_code: Option<(String, String)>,
}

impl AgentBuilder {
    fn new() -> Self {
        Self {
            name: None,
            description: None,
            params: Vec::new(),
            prompt_text: String::new(),
            tools: Vec::new(),
            capabilities: AgentCapabilities::default(),
            hooks_code: None,
        }
    }
    
    fn build(self) -> Result<AgentDef, ParseError> {
        let name = self.name.ok_or(ParseError::MissingName)?;
        
        Ok(AgentDef {
            name,
            description: self.description,
            version: None,
            capabilities: self.capabilities,
            input_params: self.params,
            output_type: None,
            prompt: PromptTemplate::Literal(self.prompt_text),
            hooks: AgentHooks::default(),
            source: None,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Agent name is required")]
    MissingName,
    #[error("Invalid parameter definition: {0}")]
    InvalidParameter(String),
    #[error("Invalid tool definition: {0}")]
    InvalidTool(String),
}
```

## 9.6 Agent Commands

```colony
# Define an agent inline
agent code-reviewer {
    description: "Reviews code for issues and improvements"
    
    param file: Str = ""
    param focus: Str? = null
    
    tools: [read_file, search, write_file]
    
    prompt: """
    Review the following code for:
    - Bugs and potential issues
    - Performance improvements
    - Code style and best practices
    
    File: {file}
    Focus areas: {focus}
    """
    
    on_complete(result) {
        context --type decision "Code review complete: $result.summary"
    }
}

# Invoke an agent
agent:run code-reviewer file="src/main.rs"

# Run with session (persistent conversation)
agent:run code-reviewer file="src/main.rs" --session my-review

# Continue a session
agent:continue my-review "Now check the error handling"

# List available agents
agent:list

# Show agent definition
agent:show code-reviewer

# List sessions
agent:sessions
agent:sessions code-reviewer

# Delete session
agent:session:delete my-review

# Export agent conversation
agent:export my-review > review.md
agent:export my-review --format json > review.json

# Background agent execution
agent:spawn analyzer file="large.log" &
var handle = $!

# Wait for agent
agent:wait $handle
var result = agent:result $handle

# Cancel agent
agent:cancel $handle
```

## 9.7 Multi-Agent Orchestration

```colony
# Define a pipeline of agents
proc analyze-and-fix() [str -> str] {
    # First agent analyzes
    var analysis = agent:run analyzer input=$stdin
    
    # Second agent fixes based on analysis  
    agent:run fixer {
        issues: $analysis.issues
        file: $analysis.file
    }
}

# Parallel agent execution
proc parallel-review(files: List[Str]) {
    var handles = []
    
    for file in $files {
        handles += agent:spawn reviewer file=$file &
    }
    
    # Collect results
    var results = []
    for h in $handles {
        agent:wait $h
        results += agent:result $h
    }
    
    return $results
}

# Agent collaboration via context
proc collaborative-task() {
    # First agent writes to context
    agent:run planner task="Build feature X"
    
    # Second agent reads context and acts
    agent:run implementer --context-types decision,action
    
    # Third agent reviews everything
    agent:run reviewer --context-types all
}

# Supervisory agent pattern
agent supervisor {
    tools: [agent:spawn, agent:wait, agent:result, context:read]
    
    prompt: """
    You are a supervisor agent. Break down the following task
    and delegate to worker agents:
    
    Available workers: {workers}
    Task: {task}
    """
}
```

## 9.8 Tool Classification

Tools are classified by their safety profile for agent execution:

```rust
// crates/colony-agent/src/tools.rs

/// Tool classification for agent auto-execution
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolClassification {
    /// Safe for automatic execution without user approval.
    /// Examples: read_file, search, list_directory
    Programmatic,

    /// Requires user approval before execution.
    /// Examples: write_file, delete_file, execute_command
    UserFacing,
}

/// Tool definition with classification
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub classification: ToolClassification,
}

/// Tool registry with classification-based dispatch
pub struct ToolRegistry {
    tools: IndexMap<String, (ToolDefinition, Arc<dyn Tool>)>,
}

impl ToolRegistry {
    /// Register a tool with classification
    pub fn register<T: Tool + 'static>(
        &mut self,
        name: &str,
        description: &str,
        classification: ToolClassification,
        tool: T,
    ) {
        let def = ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: T::schema(),
            classification,
        };
        self.tools.insert(name.to_string(), (def, Arc::new(tool)));
    }

    /// Check if a tool requires user approval
    pub fn requires_approval(&self, name: &str) -> bool {
        self.tools.get(name)
            .map(|(def, _)| def.classification == ToolClassification::UserFacing)
            .unwrap_or(true) // Unknown tools require approval
    }

    /// Get all programmatic tools (safe for auto-execution)
    pub fn programmatic_tools(&self) -> Vec<&ToolDefinition> {
        self.tools.values()
            .filter(|(def, _)| def.classification == ToolClassification::Programmatic)
            .map(|(def, _)| def)
            .collect()
    }
}
```

### 9.8.1 Default Classifications

| Tool | Classification | Rationale |
|------|---------------|-----------|
| `read_file` | Programmatic | Read-only, safe |
| `search` | Programmatic | Read-only, safe |
| `list_directory` | Programmatic | Read-only, safe |
| `write_file` | UserFacing | Modifies filesystem |
| `delete_file` | UserFacing | Destructive |
| `execute_command` | UserFacing | Arbitrary code execution |
| `git_commit` | UserFacing | Modifies repository |
| `send_email` | UserFacing | External communication |

### 9.8.2 Approval Flow

```
Agent requests tool call
         │
         ▼
    ┌────────────┐
    │ Check      │
    │ Classification│
    └────┬───────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
Programmatic  UserFacing
    │              │
    ▼              ▼
Execute     ┌─────────────┐
Immediately │ OSC 2337    │
    │       │ Widget=     │
    │       │ approval    │
    │       └──────┬──────┘
    │              │
    │         ┌────┴────┐
    │         │         │
    │         ▼         ▼
    │      Approved   Denied
    │         │         │
    │         ▼         ▼
    └────────Execute   Skip/Error
```

## 9.9 Natural Language Detection

The shell detects natural language input and routes to agents:

```rust
// crates/colony-agent/src/nl.rs

/// Detects natural language input
pub struct NaturalLanguageDetector;

impl NaturalLanguageDetector {
    /// Check if input should be routed to an agent
    pub fn is_natural_language(input: &str) -> NLDetectionResult {
        let trimmed = input.trim();

        // Explicit prefixes
        if trimmed.starts_with('#') {
            return NLDetectionResult::Agent {
                query: trimmed[1..].trim().to_string(),
                reason: NLReason::HashPrefix,
            };
        }

        if trimmed.starts_with('?') {
            return NLDetectionResult::Agent {
                query: trimmed[1..].trim().to_string(),
                reason: NLReason::QuestionPrefix,
            };
        }

        // Heuristic: starts with question word
        let lower = trimmed.to_lowercase();
        let question_words = ["what", "why", "how", "when", "where", "can", "could", "would", "should"];
        if question_words.iter().any(|w| lower.starts_with(w)) && trimmed.ends_with('?') {
            return NLDetectionResult::Ambiguous {
                query: trimmed.to_string(),
                confidence: 0.8,
            };
        }

        // Try to parse as shell command
        match colony_parser::parse_statement(trimmed) {
            Ok(_) => NLDetectionResult::Shell,
            Err(_) => NLDetectionResult::Ambiguous {
                query: trimmed.to_string(),
                confidence: 0.5,
            },
        }
    }
}

#[derive(Debug)]
pub enum NLDetectionResult {
    /// Definitely a shell command
    Shell,
    /// Definitely for an agent
    Agent { query: String, reason: NLReason },
    /// Could be either, confidence indicates likelihood of NL
    Ambiguous { query: String, confidence: f32 },
}

#[derive(Debug)]
pub enum NLReason {
    /// Input started with #
    HashPrefix,
    /// Input started with ?
    QuestionPrefix,
    /// Failed to parse as shell command
    ParseFailed,
    /// Question pattern detected
    QuestionPattern,
}
```

### 9.9.1 Prefix Conventions

| Prefix | Meaning | Example |
|--------|---------|---------|
| `#` | Route to default agent | `# explain this error` |
| `?` | Ask a question | `? what does SIGPIPE mean` |
| `@agent` | Route to specific agent | `@reviewer check this file` |
| (none) | Try shell first, fallback to agent on failure | `how do I find large files` |

## 9.10 Claude SDK Integration

Colony includes a native Rust implementation of the Claude Agent SDK:

```rust
// crates/colony-agent/src/sdk/mod.rs

pub mod client;
pub mod message;
pub mod tool;
pub mod stream;

/// Claude API client with streaming support
pub struct ClaudeClient {
    api_key: String,
    base_url: String,
    model: String,
}

impl ClaudeClient {
    /// Create client for Anthropic API
    pub fn anthropic(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    /// Create client for AWS Bedrock
    pub fn bedrock(region: &str) -> Self {
        Self {
            api_key: String::new(), // Uses SigV4
            base_url: format!("https://bedrock-runtime.{}.amazonaws.com", region),
            model: "anthropic.claude-3-sonnet".to_string(),
        }
    }

    /// Send a message and receive streaming response
    pub async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<impl Stream<Item = StreamEvent>, ApiError> {
        // Implementation details...
    }
}

/// Streaming event from Claude
#[derive(Debug)]
pub enum StreamEvent {
    /// Text content being generated
    ContentBlockStart { index: usize, content_type: String },
    /// Delta text update
    ContentBlockDelta { index: usize, delta: String },
    /// Content block finished
    ContentBlockStop { index: usize },
    /// Tool use requested
    ToolUse { id: String, name: String, input: serde_json::Value },
    /// Message complete
    MessageStop { stop_reason: StopReason },
}

#[derive(Debug)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}
```

### 9.10.1 Prompt Caching

The SDK supports prompt caching for cost optimization:

```rust
impl ClaudeClient {
    /// Enable prompt caching for system prompt and tools
    pub fn with_caching(mut self, enabled: bool) -> Self {
        self.cache_enabled = enabled;
        self
    }

    /// Cached request marks system prompt for caching
    pub async fn cached_stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<impl Stream<Item = StreamEvent>, ApiError> {
        let request = ApiRequest {
            system: vec![SystemBlock {
                text: system.to_string(),
                cache_control: Some(CacheControl { type_: "ephemeral" }),
            }],
            messages: messages.to_vec(),
            tools: tools.iter().map(|t| ApiTool {
                definition: t.clone(),
                cache_control: Some(CacheControl { type_: "ephemeral" }),
            }).collect(),
            ..Default::default()
        };

        // Cache hit reduces cost by ~90%
        self.send_streaming(request).await
    }
}
```

---

*Continue to Part 10: Coordination*