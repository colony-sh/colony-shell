# Part 7: Extended Channels

## 7.1 Overview

Colony extends the traditional Unix file descriptor model with semantic channels that capture different types of information flowing through a session.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            Colony Process                                │
│                                                                         │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌───────────┐ ┌─────────┐         │
│  │ stdin   │ │ stdout  │ │ stderr  │ │ stdcontext│ │ stdllm  │         │
│  │ fd 0    │ │ fd 1    │ │ fd 2    │ │ fd 3      │ │ fd 4    │         │
│  └────┬────┘ └────┬────┘ └────┬────┘ └─────┬─────┘ └────┬────┘         │
│       │           │           │            │            │               │
└───────┼───────────┼───────────┼────────────┼────────────┼───────────────┘
        │           │           │            │            │
        ▼           ▼           ▼            ▼            ▼
    ┌───────┐   ┌───────┐   ┌───────┐   ┌─────────┐  ┌─────────┐
    │ Input │   │ Main  │   │ Error │   │ Context │  │  LLM    │
    │       │   │Buffer │   │Buffer │   │ Buffer  │  │ Buffer  │
    └───────┘   └───────┘   └───────┘   └─────────┘  └─────────┘
```

## 7.2 Channel Registry

```rust
// crates/colony-channels/src/channel.rs

/// Channel identifier (file descriptor number)
pub type ChannelId = u8;

/// Standard channel constants
pub const STDIN: ChannelId = 0;
pub const STDOUT: ChannelId = 1;
pub const STDERR: ChannelId = 2;
pub const STDCONTEXT: ChannelId = 3;
pub const STDLLM: ChannelId = 4;
pub const STDTOOL: ChannelId = 5;
pub const STDTRACE: ChannelId = 6;

/// Channel trait - all channels implement this
pub trait Channel: Send + Sync {
    /// Get channel ID
    fn id(&self) -> ChannelId;
    
    /// Get channel name
    fn name(&self) -> &str;
    
    /// Write raw bytes
    fn write(&mut self, data: &[u8]) -> io::Result<usize>;
    
    /// Write structured value
    fn write_structured(&mut self, value: &Value) -> io::Result<()>;
    
    /// Read raw bytes
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    
    /// Read structured value (if available)
    fn read_structured(&mut self) -> io::Result<Option<Value>>;
    
    /// Flush pending writes
    fn flush(&mut self) -> io::Result<()>;
    
    /// Check if this is a TTY
    fn is_tty(&self) -> bool;
    
    /// Get associated buffer (if any)
    fn buffer(&self) -> Option<&Buffer>;
    
    /// Get mutable associated buffer
    fn buffer_mut(&mut self) -> Option<&mut Buffer>;
}

/// Channel manager - owns all channels
pub struct ChannelManager {
    channels: IndexMap<ChannelId, Box<dyn Channel>>,
    aliases: IndexMap<String, ChannelId>,
}

impl ChannelManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            channels: IndexMap::new(),
            aliases: IndexMap::new(),
        };
        
        // Register standard channels
        mgr.register(Box::new(StdinChannel::new()));
        mgr.register(Box::new(StdoutChannel::new()));
        mgr.register(Box::new(StderrChannel::new()));
        mgr.register(Box::new(ContextChannel::new()));
        mgr.register(Box::new(LlmChannel::new()));
        mgr.register(Box::new(ToolChannel::new()));
        mgr.register(Box::new(TraceChannel::new()));
        
        // Register aliases
        mgr.alias("ctx", STDCONTEXT);
        mgr.alias("context", STDCONTEXT);
        mgr.alias("llm", STDLLM);
        mgr.alias("tool", STDTOOL);
        mgr.alias("trace", STDTRACE);
        
        mgr
    }
    
    pub fn register(&mut self, channel: Box<dyn Channel>) {
        let id = channel.id();
        self.channels.insert(id, channel);
    }
    
    pub fn alias(&mut self, name: &str, id: ChannelId) {
        self.aliases.insert(name.to_string(), id);
    }
    
    /// Resolve channel reference (number or alias)
    pub fn resolve(&self, reference: &str) -> Option<ChannelId> {
        // Try as number
        if let Ok(id) = reference.parse::<ChannelId>() {
            if self.channels.contains_key(&id) {
                return Some(id);
            }
        }
        
        // Try as alias
        self.aliases.get(reference).copied()
    }
    
    pub fn get(&self, id: ChannelId) -> Option<&dyn Channel> {
        self.channels.get(&id).map(|c| c.as_ref())
    }
    
    pub fn get_mut(&mut self, id: ChannelId) -> Option<&mut dyn Channel> {
        self.channels.get_mut(&id).map(|c| c.as_mut())
    }
    
    /// Get typed channel
    pub fn context(&self) -> Option<&ContextChannel> {
        self.channels.get(&STDCONTEXT)
            .and_then(|c| c.as_any().downcast_ref())
    }
    
    pub fn context_mut(&mut self) -> Option<&mut ContextChannel> {
        self.channels.get_mut(&STDCONTEXT)
            .and_then(|c| c.as_any_mut().downcast_mut())
    }
    
    pub fn llm(&self) -> Option<&LlmChannel> {
        self.channels.get(&STDLLM)
            .and_then(|c| c.as_any().downcast_ref())
    }
    
    pub fn llm_mut(&mut self) -> Option<&mut LlmChannel> {
        self.channels.get_mut(&STDLLM)
            .and_then(|c| c.as_any_mut().downcast_mut())
    }
}
```

## 7.3 Channel Redirection Syntax

```colony
# Traditional redirects
echo "hello" > file.txt
echo "error" >&2

# Channel redirects by number
echo "context note" >&3          # Write to stdcontext
echo "user message" >&4          # Write to stdllm

# Channel redirects by alias
echo "note" >&ctx                # Alias for fd 3
echo "note" >&context            # Also alias for fd 3
echo "message" >&llm             # Alias for fd 4

# Structured writes (via builtins)
context "observation" "User requested analysis"
llm:user "Please explain this code"

# Reading from channels
var ctx = $(&ctx)                # Read context as string
var history = llm:get            # Get LLM history as value
```

## 7.4 Context Channel (stdcontext - fd 3)

### 7.4.1 Purpose

The context channel accumulates semantic information about the session:
- Observations and notes
- Decisions made
- Actions taken
- Errors encountered
- Artifacts created
- Questions and references

This context is automatically available to agents.

### 7.4.2 Implementation

```rust
// crates/colony-channels/src/context.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub struct ContextChannel {
    entries: Vec<ContextEntry>,
    max_entries: usize,
    session_id: String,
    buffer: Buffer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextEntry {
    /// Unique identifier
    pub id: String,
    /// When this entry was created
    pub timestamp: DateTime<Utc>,
    /// Type of context
    pub entry_type: ContextType,
    /// Where this came from
    pub source: ContextSource,
    /// The actual content
    pub content: String,
    /// Additional structured data
    pub metadata: IndexMap<String, Value>,
    /// Session this belongs to
    pub session_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContextType {
    /// Something observed or noted
    Observation,
    /// A decision that was made
    Decision,
    /// An action that was taken
    Action,
    /// A file or output that was created
    Artifact,
    /// An error that occurred
    Error,
    /// A question or uncertainty
    Question,
    /// A reference to external resource
    Reference,
    /// Seed context from CONTEXT.md
    Seed,
    /// Custom type
    Custom(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextSource {
    /// Source file (if known)
    pub file: Option<String>,
    /// Line number (if known)
    pub line: Option<u32>,
    /// Command that generated this
    pub command: Option<String>,
    /// Agent that generated this
    pub agent: Option<String>,
}

impl ContextChannel {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 1000,
            session_id: generate_session_id(),
            buffer: Buffer::new(STDCONTEXT, "context"),
        }
    }
    
    /// Add a new context entry
    pub fn add(&mut self, entry: ContextEntry) {
        // Enforce max entries
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        
        // Update buffer
        self.buffer.append_line(
            &format!("[{:?}] {}", entry.entry_type, entry.content),
            LineStyle::default(),
        );
        
        self.entries.push(entry);
    }
    
    /// Add a simple text entry
    pub fn add_simple(&mut self, entry_type: ContextType, content: &str) {
        self.add(ContextEntry {
            id: generate_id("ctx"),
            timestamp: Utc::now(),
            entry_type,
            source: ContextSource::default(),
            content: content.to_string(),
            metadata: IndexMap::new(),
            session_id: self.session_id.clone(),
        });
    }
    
    /// Add entry with source information
    pub fn add_with_source(&mut self, entry_type: ContextType, content: &str, source: ContextSource) {
        self.add(ContextEntry {
            id: generate_id("ctx"),
            timestamp: Utc::now(),
            entry_type,
            source,
            content: content.to_string(),
            metadata: IndexMap::new(),
            session_id: self.session_id.clone(),
        });
    }
    
    /// Query entries with filter
    pub fn query(&self, filter: &ContextFilter) -> Vec<&ContextEntry> {
        self.entries.iter()
            .filter(|e| filter.matches(e))
            .collect()
    }
    
    /// Get all entries
    pub fn entries(&self) -> &[ContextEntry] {
        &self.entries
    }
    
    /// Get entries since a timestamp
    pub fn since(&self, timestamp: DateTime<Utc>) -> Vec<&ContextEntry> {
        self.entries.iter()
            .filter(|e| e.timestamp >= timestamp)
            .collect()
    }
    
    /// Get entries of specific type
    pub fn by_type(&self, entry_type: &ContextType) -> Vec<&ContextEntry> {
        self.entries.iter()
            .filter(|e| &e.entry_type == entry_type)
            .collect()
    }
    
    /// Search content
    pub fn search(&self, pattern: &str) -> Vec<&ContextEntry> {
        self.entries.iter()
            .filter(|e| e.content.contains(pattern))
            .collect()
    }
    
    /// Export to format
    pub fn export(&self, format: ExportFormat) -> Result<String, ExportError> {
        match format {
            ExportFormat::Json => {
                serde_json::to_string_pretty(&self.entries)
                    .map_err(ExportError::Json)
            }
            ExportFormat::JsonLines => {
                let lines: Vec<String> = self.entries.iter()
                    .map(|e| serde_json::to_string(e).unwrap())
                    .collect();
                Ok(lines.join("\n"))
            }
            ExportFormat::Markdown => Ok(self.to_markdown()),
            ExportFormat::Plain => Ok(self.to_plain()),
        }
    }
    
    fn to_markdown(&self) -> String {
        let mut md = String::from("# Session Context\n\n");
        
        let mut by_type: IndexMap<ContextType, Vec<&ContextEntry>> = IndexMap::new();
        for entry in &self.entries {
            by_type.entry(entry.entry_type.clone())
                .or_default()
                .push(entry);
        }
        
        for (entry_type, entries) in by_type {
            md.push_str(&format!("## {:?}\n\n", entry_type));
            for entry in entries {
                md.push_str(&format!(
                    "- **{}**: {}\n",
                    entry.timestamp.format("%H:%M:%S"),
                    entry.content
                ));
            }
            md.push('\n');
        }
        
        md
    }
    
    fn to_plain(&self) -> String {
        self.entries.iter()
            .map(|e| format!("[{}] {:?}: {}", 
                e.timestamp.format("%H:%M:%S"),
                e.entry_type,
                e.content
            ))
            .collect::<Vec<_>>()
            .join("\n")
    }
    
    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.buffer.clear();
    }
    
    /// Summarize context (for LLM consumption)
    pub fn summarize(&self, max_tokens: usize) -> String {
        // Simple implementation - just truncate
        // Could use LLM to actually summarize
        let full = self.to_plain();
        if full.len() <= max_tokens * 4 {  // Rough char/token ratio
            full
        } else {
            format!("{}... (truncated)", &full[..max_tokens * 4])
        }
    }
}

impl Channel for ContextChannel {
    fn id(&self) -> ChannelId { STDCONTEXT }
    fn name(&self) -> &str { "context" }
    
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(data);
        self.add_simple(ContextType::Observation, &text);
        Ok(data.len())
    }
    
    fn write_structured(&mut self, value: &Value) -> io::Result<()> {
        // Parse structured context entry
        if let Value::Dict(d) = value {
            let entry_type = d.get("type")
                .and_then(|v| v.as_str())
                .map(|s| match s {
                    "observation" => ContextType::Observation,
                    "decision" => ContextType::Decision,
                    "action" => ContextType::Action,
                    "artifact" => ContextType::Artifact,
                    "error" => ContextType::Error,
                    "question" => ContextType::Question,
                    "reference" => ContextType::Reference,
                    other => ContextType::Custom(other.to_string()),
                })
                .unwrap_or(ContextType::Observation);
            
            let content = d.get("content")
                .map(|v| v.to_string())
                .unwrap_or_default();
            
            self.add_simple(entry_type, &content);
        } else {
            self.add_simple(ContextType::Observation, &value.to_string());
        }
        Ok(())
    }
    
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let text = self.to_plain();
        let bytes = text.as_bytes();
        let len = buf.len().min(bytes.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }
    
    fn read_structured(&mut self) -> io::Result<Option<Value>> {
        let entries: Vec<Value> = self.entries.iter()
            .map(|e| {
                let mut d = IndexMap::new();
                d.insert("id".to_string(), Value::Str(e.id.clone()));
                d.insert("timestamp".to_string(), Value::Str(e.timestamp.to_rfc3339()));
                d.insert("type".to_string(), Value::Str(format!("{:?}", e.entry_type)));
                d.insert("content".to_string(), Value::Str(e.content.clone()));
                Value::Dict(d)
            })
            .collect();
        Ok(Some(Value::List(entries)))
    }
    
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
    fn is_tty(&self) -> bool { false }
    
    fn buffer(&self) -> Option<&Buffer> { Some(&self.buffer) }
    fn buffer_mut(&mut self) -> Option<&mut Buffer> { Some(&mut self.buffer) }
}

/// Filter for querying context
#[derive(Clone, Debug, Default)]
pub struct ContextFilter {
    pub entry_types: Option<Vec<ContextType>>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub source_file: Option<String>,
    pub source_agent: Option<String>,
    pub content_contains: Option<String>,
    pub limit: Option<usize>,
}

impl ContextFilter {
    pub fn matches(&self, entry: &ContextEntry) -> bool {
        if let Some(ref types) = self.entry_types {
            if !types.contains(&entry.entry_type) {
                return false;
            }
        }
        
        if let Some(since) = self.since {
            if entry.timestamp < since {
                return false;
            }
        }
        
        if let Some(until) = self.until {
            if entry.timestamp > until {
                return false;
            }
        }
        
        if let Some(ref file) = self.source_file {
            if entry.source.file.as_ref() != Some(file) {
                return false;
            }
        }
        
        if let Some(ref agent) = self.source_agent {
            if entry.source.agent.as_ref() != Some(agent) {
                return false;
            }
        }
        
        if let Some(ref pattern) = self.content_contains {
            if !entry.content.contains(pattern) {
                return false;
            }
        }
        
        true
    }
}
```

### 7.4.3 Context Builtins

```colony
# Add context entries
context "Started working on feature X"
context --type decision "Chose blue-green deployment strategy"
context --type action "Created branch feature/x"
context --type error "Build failed: missing dependency"

# Add with metadata
context:add {
    type: "observation"
    content: "API response time increased by 20%"
    metadata: {
        endpoint: "/api/users"
        avg_ms: 450
    }
}

# Query context
context:show                           # Show all
context:show --last 5                  # Last 5 entries
context:show --type decision           # Only decisions
context:show --since "1 hour ago"      # Recent entries
context:show --agent code-reviewer     # From specific agent

# Search context
context:search "deployment"

# Export context
context:export > context.json
context:export --format markdown > notes.md
context:export --format jsonl >> session.jsonl

# Clear context
context:clear
context:clear --type observation       # Clear only observations
```

## 7.5 LLM Channel (stdllm - fd 4)

### 7.5.1 Purpose

The LLM channel maintains the conversation history with AI assistants:
- User messages
- Assistant responses
- Tool calls and results
- System messages
- Multi-agent conversations

### 7.5.2 Implementation

```rust
// crates/colony-channels/src/llm.rs

pub struct LlmChannel {
    /// Conversations by branch name
    conversations: IndexMap<String, Conversation>,
    /// Current branch
    current_branch: String,
    /// Checkpoints for restore
    checkpoints: IndexMap<String, ConversationSnapshot>,
    /// Associated buffer
    buffer: Buffer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub messages: Vec<Message>,
    pub metadata: ConversationMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    /// Unique identifier
    pub id: String,
    /// Role (system, user, assistant)
    pub role: Role,
    /// Message content
    pub content: String,
    /// Agent that generated this (if any)
    pub agent: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Tool calls made in this message
    pub tool_calls: Vec<ToolCall>,
    /// Tool results (for tool messages)
    pub tool_results: Vec<ToolResult>,
    /// Additional metadata
    pub metadata: IndexMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub result: Value,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationMetadata {
    pub created_at: DateTime<Utc>,
    pub model: Option<String>,
    pub total_tokens: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl LlmChannel {
    pub fn new() -> Self {
        let mut conversations = IndexMap::new();
        conversations.insert("main".to_string(), Conversation {
            messages: Vec::new(),
            metadata: ConversationMetadata {
                created_at: Utc::now(),
                model: None,
                total_tokens: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
        });
        
        Self {
            conversations,
            current_branch: "main".to_string(),
            checkpoints: IndexMap::new(),
            buffer: Buffer::new(STDLLM, "llm"),
        }
    }
    
    /// Add a message to current conversation
    pub fn add_message(&mut self, role: Role, content: &str) {
        self.add_message_with_agent(role, content, None);
    }
    
    /// Add a message from specific agent
    pub fn add_message_with_agent(&mut self, role: Role, content: &str, agent: Option<&str>) {
        let msg = Message {
            id: generate_id("msg"),
            role,
            content: content.to_string(),
            agent: agent.map(|s| s.to_string()),
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
            metadata: IndexMap::new(),
        };
        
        // Update buffer
        let prefix = match role {
            Role::System => "[system]",
            Role::User => "[user]",
            Role::Assistant => "[assistant]",
            Role::Tool => "[tool]",
        };
        let agent_suffix = agent.map(|a| format!(" ({})", a)).unwrap_or_default();
        self.buffer.append_line(
            &format!("{}{}: {}", prefix, agent_suffix, content),
            LineStyle::default(),
        );
        
        let conv = self.conversations.get_mut(&self.current_branch).unwrap();
        conv.messages.push(msg);
    }
    
    /// Add assistant message with tool calls
    pub fn add_assistant_with_tools(&mut self, content: &str, tool_calls: Vec<ToolCall>, agent: Option<&str>) {
        let mut msg = Message {
            id: generate_id("msg"),
            role: Role::Assistant,
            content: content.to_string(),
            agent: agent.map(|s| s.to_string()),
            timestamp: Utc::now(),
            tool_calls,
            tool_results: Vec::new(),
            metadata: IndexMap::new(),
        };
        
        let conv = self.conversations.get_mut(&self.current_branch).unwrap();
        conv.messages.push(msg);
    }
    
    /// Add tool result
    pub fn add_tool_result(&mut self, call_id: &str, result: Value, error: Option<&str>) {
        let msg = Message {
            id: generate_id("msg"),
            role: Role::Tool,
            content: String::new(),
            agent: None,
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
            tool_results: vec![ToolResult {
                call_id: call_id.to_string(),
                result,
                error: error.map(|s| s.to_string()),
            }],
            metadata: IndexMap::new(),
        };
        
        let conv = self.conversations.get_mut(&self.current_branch).unwrap();
        conv.messages.push(msg);
    }
    
    /// Get current conversation messages
    pub fn messages(&self) -> &[Message] {
        &self.conversations.get(&self.current_branch).unwrap().messages
    }
    
    /// Get messages formatted for API call
    pub fn messages_for_api(&self) -> Vec<ApiMessage> {
        self.messages().iter()
            .map(|m| ApiMessage {
                role: m.role.as_str().to_string(),
                content: m.content.clone(),
                tool_calls: if m.tool_calls.is_empty() { None } else { Some(m.tool_calls.clone()) },
                tool_call_id: m.tool_results.first().map(|r| r.call_id.clone()),
            })
            .collect()
    }
    
    /// Create a new branch from current state
    pub fn branch(&mut self, name: &str) {
        let current = self.conversations.get(&self.current_branch).unwrap().clone();
        self.conversations.insert(name.to_string(), current);
        self.current_branch = name.to_string();
    }
    
    /// Switch to existing branch
    pub fn checkout(&mut self, name: &str) -> Result<(), LlmError> {
        if !self.conversations.contains_key(name) {
            return Err(LlmError::BranchNotFound(name.to_string()));
        }
        self.current_branch = name.to_string();
        Ok(())
    }
    
    /// List branches
    pub fn branches(&self) -> Vec<&str> {
        self.conversations.keys().map(|s| s.as_str()).collect()
    }
    
    /// Create checkpoint
    pub fn checkpoint(&mut self, name: &str) {
        let conv = self.conversations.get(&self.current_branch).unwrap();
        self.checkpoints.insert(name.to_string(), ConversationSnapshot {
            branch: self.current_branch.clone(),
            messages: conv.messages.clone(),
            timestamp: Utc::now(),
        });
    }
    
    /// Restore from checkpoint
    pub fn restore(&mut self, name: &str) -> Result<(), LlmError> {
        let snapshot = self.checkpoints.get(name)
            .ok_or_else(|| LlmError::CheckpointNotFound(name.to_string()))?
            .clone();
        
        let conv = self.conversations.get_mut(&snapshot.branch).unwrap();
        conv.messages = snapshot.messages;
        self.current_branch = snapshot.branch;
        Ok(())
    }
    
    /// Truncate to last N messages
    pub fn truncate(&mut self, keep_last: usize) {
        let conv = self.conversations.get_mut(&self.current_branch).unwrap();
        if conv.messages.len() > keep_last {
            conv.messages = conv.messages.split_off(conv.messages.len() - keep_last);
        }
    }
    
    /// Clear current conversation
    pub fn clear(&mut self) {
        let conv = self.conversations.get_mut(&self.current_branch).unwrap();
        conv.messages.clear();
        self.buffer.clear();
    }
    
    /// Export conversation
    pub fn export(&self, format: ExportFormat) -> Result<String, ExportError> {
        let conv = self.conversations.get(&self.current_branch).unwrap();
        
        match format {
            ExportFormat::Json => {
                serde_json::to_string_pretty(conv).map_err(ExportError::Json)
            }
            ExportFormat::Markdown => Ok(self.to_markdown()),
            ExportFormat::ShareGpt => Ok(self.to_sharegpt()),
            _ => Err(ExportError::UnsupportedFormat),
        }
    }
    
    fn to_markdown(&self) -> String {
        let conv = self.conversations.get(&self.current_branch).unwrap();
        let mut md = String::from("# Conversation\n\n");
        
        for msg in &conv.messages {
            let role_label = match msg.role {
                Role::System => "**System**",
                Role::User => "**User**",
                Role::Assistant => "**Assistant**",
                Role::Tool => "**Tool**",
            };
            
            let agent_suffix = msg.agent.as_ref()
                .map(|a| format!(" ({})", a))
                .unwrap_or_default();
            
            md.push_str(&format!("{}{}\n\n{}\n\n---\n\n", 
                role_label, agent_suffix, msg.content));
        }
        
        md
    }
    
    fn to_sharegpt(&self) -> String {
        let conv = self.conversations.get(&self.current_branch).unwrap();
        let entries: Vec<_> = conv.messages.iter()
            .filter(|m| m.role != Role::System && m.role != Role::Tool)
            .map(|m| {
                serde_json::json!({
                    "from": if m.role == Role::User { "human" } else { "gpt" },
                    "value": m.content
                })
            })
            .collect();
        
        serde_json::to_string_pretty(&serde_json::json!({
            "conversations": entries
        })).unwrap()
    }
}

impl Channel for LlmChannel {
    fn id(&self) -> ChannelId { STDLLM }
    fn name(&self) -> &str { "llm" }
    
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(data);
        self.add_message(Role::User, &text);
        Ok(data.len())
    }
    
    fn write_structured(&mut self, value: &Value) -> io::Result<()> {
        if let Value::Dict(d) = value {
            let role = d.get("role")
                .and_then(|v| v.as_str())
                .map(|s| match s {
                    "system" => Role::System,
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    "tool" => Role::Tool,
                    _ => Role::User,
                })
                .unwrap_or(Role::User);
            
            let content = d.get("content")
                .map(|v| v.to_string())
                .unwrap_or_default();
            
            let agent = d.get("agent")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            
            self.add_message_with_agent(role, &content, agent.as_deref());
        } else {
            self.add_message(Role::User, &value.to_string());
        }
        Ok(())
    }
    
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let json = serde_json::to_string(&self.messages_for_api()).unwrap();
        let bytes = json.as_bytes();
        let len = buf.len().min(bytes.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }
    
    fn read_structured(&mut self) -> io::Result<Option<Value>> {
        let messages: Vec<Value> = self.messages().iter()
            .map(|m| {
                let mut d = IndexMap::new();
                d.insert("id".to_string(), Value::Str(m.id.clone()));
                d.insert("role".to_string(), Value::Str(m.role.as_str().to_string()));
                d.insert("content".to_string(), Value::Str(m.content.clone()));
                if let Some(ref agent) = m.agent {
                    d.insert("agent".to_string(), Value::Str(agent.clone()));
                }
                Value::Dict(d)
            })
            .collect();
        Ok(Some(Value::List(messages)))
    }
    
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
    fn is_tty(&self) -> bool { false }
    
    fn buffer(&self) -> Option<&Buffer> { Some(&self.buffer) }
    fn buffer_mut(&mut self) -> Option<&mut Buffer> { Some(&mut self.buffer) }
}

#[derive(Clone, Debug)]
struct ConversationSnapshot {
    branch: String,
    messages: Vec<Message>,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("Branch not found: {0}")]
    BranchNotFound(String),
    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(String),
}
```

### 7.5.3 LLM Builtins

```colony
# Add messages
llm:user "Please explain this code"
llm:system "You are a helpful assistant"
llm:assistant "I'd be happy to help!"  # For injecting responses

# Structured message
llm:message {
    role: "user"
    content: "Analyze this file"
    metadata: { file: "main.rs" }
}

# View conversation
llm:show                    # Pretty print
llm:show --last 5           # Last 5 messages
llm:show --role user        # Only user messages
llm:show --agent reviewer   # Only from specific agent

# Get as data
var history = llm:get
echo $[len(history.messages)]

# Branching
llm:branch "experiment"     # Create branch
llm:checkout "main"         # Switch back
llm:branches                # List branches
llm:cherry-pick "experiment" 3  # Copy message #3

# Checkpoints
llm:checkpoint "before-refactor"
llm:restore "before-refactor"

# Management
llm:summarize               # Ask LLM to summarize
llm:truncate --keep-last 10
llm:clear

# Export
llm:export > conversation.json
llm:export --format markdown > chat.md
llm:export --format sharegpt > training.jsonl

# Interactive chat mode
llm:chat                    # Enter chat mode
llm:chat --agent assistant  # With specific agent
```

## 7.6 Tool Channel (stdtool - fd 5)

### 7.6.1 Purpose

The tool channel logs all tool calls and their results:
- Tool invocations
- Arguments passed
- Results received
- Errors encountered
- Timing information

### 7.6.2 Implementation

```rust
// crates/colony-channels/src/tool.rs

pub struct ToolChannel {
    calls: Vec<ToolCallRecord>,
    max_records: usize,
    buffer: Buffer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub agent: Option<String>,
    pub tool_name: String,
    pub arguments: Value,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub status: ToolCallStatus,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolCallStatus {
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
}

impl ToolChannel {
    pub fn new() -> Self {
        Self {
            calls: Vec::new(),
            max_records: 200,
            buffer: Buffer::new(STDTOOL, "tool"),
        }
    }
    
    /// Record a new tool call
    pub fn record_call(&mut self, tool_name: &str, arguments: Value, agent: Option<&str>) -> String {
        let id = generate_id("tc");
        
        self.buffer.append_line(
            &format!("→ {}({})", tool_name, truncate_json(&arguments, 50)),
            LineStyle::default(),
        );
        
        let record = ToolCallRecord {
            id: id.clone(),
            timestamp: Utc::now(),
            agent: agent.map(|s| s.to_string()),
            tool_name: tool_name.to_string(),
            arguments,
            result: None,
            error: None,
            duration_ms: 0,
            status: ToolCallStatus::Pending,
        };
        
        if self.calls.len() >= self.max_records {
            self.calls.remove(0);
        }
        
        self.calls.push(record);
        id
    }
    
    /// Update call status to running
    pub fn mark_running(&mut self, id: &str) {
        if let Some(record) = self.calls.iter_mut().find(|r| r.id == id) {
            record.status = ToolCallStatus::Running;
        }
    }
    
    /// Record successful result
    pub fn record_result(&mut self, id: &str, result: Value, duration_ms: u64) {
        if let Some(record) = self.calls.iter_mut().find(|r| r.id == id) {
            record.result = Some(result.clone());
            record.duration_ms = duration_ms;
            record.status = ToolCallStatus::Success;
            
            self.buffer.append_line(
                &format!("  ✓ {} ({}ms)", truncate_json(&result, 50), duration_ms),
                LineStyle { foreground: Some(Color::Green), ..Default::default() },
            );
        }
    }
    
    /// Record error
    pub fn record_error(&mut self, id: &str, error: &str, duration_ms: u64) {
        if let Some(record) = self.calls.iter_mut().find(|r| r.id == id) {
            record.error = Some(error.to_string());
            record.duration_ms = duration_ms;
            record.status = ToolCallStatus::Failed;
            
            self.buffer.append_line(
                &format!("  ✗ {} ({}ms)", error, duration_ms),
                LineStyle { foreground: Some(Color::Red), ..Default::default() },
            );
        }
    }
    
    /// Get all records
    pub fn records(&self) -> &[ToolCallRecord] {
        &self.calls
    }
    
    /// Get records for specific tool
    pub fn by_tool(&self, tool_name: &str) -> Vec<&ToolCallRecord> {
        self.calls.iter()
            .filter(|r| r.tool_name == tool_name)
            .collect()
    }
    
    /// Get records for specific agent
    pub fn by_agent(&self, agent: &str) -> Vec<&ToolCallRecord> {
        self.calls.iter()
            .filter(|r| r.agent.as_deref() == Some(agent))
            .collect()
    }
    
    /// Clear all records
    pub fn clear(&mut self) {
        self.calls.clear();
        self.buffer.clear();
    }
}

impl Channel for ToolChannel {
    fn id(&self) -> ChannelId { STDTOOL }
    fn name(&self) -> &str { "tool" }
    
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        // Parse as JSON tool call
        if let Ok(value) = serde_json::from_slice::<Value>(data) {
            self.write_structured(&value)?;
        }
        Ok(data.len())
    }
    
    fn write_structured(&mut self, value: &Value) -> io::Result<()> {
        if let Value::Dict(d) = value {
            let tool_name = d.get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let arguments = d.get("arguments")
                .cloned()
                .unwrap_or(Value::Null);
            let agent = d.get("agent")
                .and_then(|v| v.as_str());
            
            self.record_call(tool_name, arguments, agent);
        }
        Ok(())
    }
    
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let json = serde_json::to_string(&self.calls).unwrap();
        let bytes = json.as_bytes();
        let len = buf.len().min(bytes.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }
    
    fn read_structured(&mut self) -> io::Result<Option<Value>> {
        let records: Vec<Value> = self.calls.iter()
            .map(|r| {
                let mut d = IndexMap::new();
                d.insert("id".to_string(), Value::Str(r.id.clone()));
                d.insert("tool".to_string(), Value::Str(r.tool_name.clone()));
                d.insert("status".to_string(), Value::Str(format!("{:?}", r.status)));
                d.insert("duration_ms".to_string(), Value::Int(r.duration_ms as i64));
                Value::Dict(d)
            })
            .collect();
        Ok(Some(Value::List(records)))
    }
    
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
    fn is_tty(&self) -> bool { false }
    
    fn buffer(&self) -> Option<&Buffer> { Some(&self.buffer) }
    fn buffer_mut(&mut self) -> Option<&mut Buffer> { Some(&mut self.buffer) }
}
```

## 7.7 Trace Channel (stdtrace - fd 6)

### 7.7.1 Purpose

The trace channel captures execution trace information:
- Command execution
- Function/proc calls
- Variable assignments
- Pipeline stages
- Performance timing

This is disabled by default for performance.

### 7.7.2 Implementation

```rust
// crates/colony-channels/src/trace.rs

pub struct TraceChannel {
    entries: Vec<TraceEntry>,
    enabled: bool,
    max_entries: usize,
    buffer: Buffer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceEntry {
    pub timestamp: DateTime<Utc>,
    pub level: TraceLevel,
    pub category: TraceCategory,
    pub message: String,
    pub source: Option<SourceLocation>,
    pub duration_ns: Option<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TraceLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TraceCategory {
    Command,
    Pipeline,
    Function,
    Variable,
    Redirect,
    Agent,
    Tool,
    Mcp,
    Custom(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl TraceChannel {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            enabled: false,
            max_entries: 5000,
            buffer: Buffer::new(STDTRACE, "trace"),
        }
    }
    
    pub fn enable(&mut self) {
        self.enabled = true;
    }
    
    pub fn disable(&mut self) {
        self.enabled = false;
    }
    
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    pub fn trace(&mut self, level: TraceLevel, category: TraceCategory, message: &str) {
        if !self.enabled {
            return;
        }
        
        let entry = TraceEntry {
            timestamp: Utc::now(),
            level,
            category,
            message: message.to_string(),
            source: None,
            duration_ns: None,
        };
        
        self.buffer.append_line(
            &format!("[{:?}] {:?}: {}", level, category, message),
            LineStyle::default(),
        );
        
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        
        self.entries.push(entry);
    }
    
    pub fn trace_with_source(&mut self, level: TraceLevel, category: TraceCategory, message: &str, source: SourceLocation) {
        if !self.enabled {
            return;
        }
        
        let entry = TraceEntry {
            timestamp: Utc::now(),
            level,
            category,
            message: message.to_string(),
            source: Some(source),
            duration_ns: None,
        };
        
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        
        self.entries.push(entry);
    }
    
    pub fn clear(&mut self) {
        self.entries.clear();
        self.buffer.clear();
    }
}

impl Channel for TraceChannel {
    fn id(&self) -> ChannelId { STDTRACE }
    fn name(&self) -> &str { "trace" }
    
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(data);
        self.trace(TraceLevel::Info, TraceCategory::Custom("write".to_string()), &text);
        Ok(data.len())
    }
    
    fn write_structured(&mut self, value: &Value) -> io::Result<()> {
        self.trace(TraceLevel::Info, TraceCategory::Custom("structured".to_string()), &value.to_string());
        Ok(())
    }
    
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let json = serde_json::to_string(&self.entries).unwrap();
        let bytes = json.as_bytes();
        let len = buf.len().min(bytes.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }
    
    fn read_structured(&mut self) -> io::Result<Option<Value>> {
        let entries: Vec<Value> = self.entries.iter()
            .map(|e| Value::Str(format!("{:?}", e)))
            .collect();
        Ok(Some(Value::List(entries)))
    }
    
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
    fn is_tty(&self) -> bool { false }
    
    fn buffer(&self) -> Option<&Buffer> { Some(&self.buffer) }
    fn buffer_mut(&mut self) -> Option<&mut Buffer> { Some(&mut self.buffer) }
}
```

## 7.8 Export Format

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    JsonLines,
    Markdown,
    Plain,
    ShareGpt,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported export format")]
    UnsupportedFormat,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

---

*Continue to Part 8: Buffer Management*