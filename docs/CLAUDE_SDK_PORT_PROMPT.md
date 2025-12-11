# Claude Agent SDK Rust Port - Initialization Prompt

You are the coding agent for the Rust port of Claude's Agent SDK. This is a native Rust implementation of the Claude API client with streaming support, tool execution, and programmatic tool calling.

## Project Context

This port is being developed as part of Colony Shell (F015 - Claude ADK integration), but can be used standalone. The goal is NOT to wrap the TypeScript SDK via FFI, but to implement a clean, idiomatic Rust client that directly integrates with the Claude Messages API.

**Key URLs:**
- Claude API Reference: https://docs.anthropic.com/en/api/messages
- Programmatic Tool Calling: https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling
- Streaming: https://docs.anthropic.com/en/api/messages-streaming

## First-Time Setup

If `.claude/system/` does not exist, create it:

```
.claude/
└── system/
    ├── features.json      # Feature tracking
    ├── history.txt        # Work log
    └── rules.txt          # Project constraints
```

### Create features.json

```json
{
  "version": 1,
  "features": [
    {
      "id": "SDK001",
      "name": "HTTP client foundation",
      "description": "reqwest-based client with API key auth, proper headers (anthropic-version, x-api-key), error types using thiserror",
      "status": "new",
      "priority": 1,
      "dependencies": [],
      "notes": []
    },
    {
      "id": "SDK002",
      "name": "Request/response types",
      "description": "Complete serde models for MessagesRequest, MessagesResponse, Message, Content, Tool, ToolUse, ToolResult. Match official API exactly.",
      "status": "new",
      "priority": 1,
      "dependencies": [],
      "notes": []
    },
    {
      "id": "SDK003",
      "name": "Non-streaming messages",
      "description": "Implement send_message() for synchronous completion. Parse JSON response, handle errors, return typed MessagesResponse.",
      "status": "new",
      "priority": 1,
      "dependencies": ["SDK001", "SDK002"],
      "notes": []
    },
    {
      "id": "SDK004",
      "name": "SSE streaming parser",
      "description": "Parse Server-Sent Events stream using eventsource-stream or custom parser. Handle all event types: message_start, content_block_start, content_block_delta, content_block_stop, message_delta, message_stop.",
      "status": "new",
      "priority": 2,
      "dependencies": ["SDK001", "SDK002"],
      "notes": []
    },
    {
      "id": "SDK005",
      "name": "Streaming messages API",
      "description": "Implement send_streaming() returning impl Stream<Item = Result<StreamEvent>>. Use tokio streams for async iteration.",
      "status": "new",
      "priority": 2,
      "dependencies": ["SDK004"],
      "notes": []
    },
    {
      "id": "SDK006",
      "name": "Tool use primitives",
      "description": "Types for Tool definition, ToolUse in responses, ToolResult in requests. Support disable_user_input field for programmatic calling.",
      "status": "new",
      "priority": 2,
      "dependencies": ["SDK002"],
      "notes": []
    },
    {
      "id": "SDK007",
      "name": "Multi-turn conversation builder",
      "description": "Builder pattern for conversation history management. Handle role alternation (user/assistant), tool use/results, proper message formatting.",
      "status": "new",
      "priority": 3,
      "dependencies": ["SDK003", "SDK006"],
      "notes": []
    },
    {
      "id": "SDK008",
      "name": "Token counting",
      "description": "Estimate token counts using tiktoken-rs or similar. Needed for context window management (200K tokens for Claude 3.5 Sonnet).",
      "status": "new",
      "priority": 3,
      "dependencies": ["SDK002"],
      "notes": []
    },
    {
      "id": "SDK009",
      "name": "Error handling taxonomy",
      "description": "Comprehensive error types: APIError (4xx/5xx), NetworkError, ParseError, RateLimitError (with retry-after). Use thiserror for library errors.",
      "status": "new",
      "priority": 2,
      "dependencies": ["SDK001"],
      "notes": []
    },
    {
      "id": "SDK010",
      "name": "Retry logic with backoff",
      "description": "Implement exponential backoff for rate limits (429) and server errors (5xx). Respect retry-after headers. Use tokio::time::sleep.",
      "status": "new",
      "priority": 3,
      "dependencies": ["SDK009"],
      "notes": []
    },
    {
      "id": "SDK011",
      "name": "Examples and integration tests",
      "description": "Example binaries: simple chat, streaming chat, tool use, multi-turn conversation. Integration tests against real API (gated by env var).",
      "status": "new",
      "priority": 4,
      "dependencies": ["SDK005", "SDK007"],
      "notes": []
    },
    {
      "id": "SDK012",
      "name": "Documentation and publishing",
      "description": "README with quickstart, API docs with examples, publish to crates.io as 'claude-sdk' or 'anthropic-sdk-rust'.",
      "status": "new",
      "priority": 4,
      "dependencies": ["SDK011"],
      "notes": []
    }
  ]
}
```

### Create history.txt

```
# Claude SDK Rust Port — Development History
# Format: [YYYY-MM-DD HH:MM] ACTION: Description

[YYYY-MM-DD HH:MM] START: Project initialized
```

### Create rules.txt

```
# Claude SDK Rust Port — Project Rules

## Design Goals

1. Native Rust implementation — no FFI, no subprocess, pure Rust
2. Idiomatic async Rust — tokio runtime, async/await, Stream trait
3. Type-safe throughout — leverage Rust type system, no stringly-typed APIs
4. Zero-copy where possible — use references, avoid unnecessary allocations
5. Standalone crate — can be used outside Colony Shell

## API Compliance

6. Match official Claude API exactly — field names, types, behavior
7. Support all Claude 3+ models — Opus, Sonnet, Haiku
8. Implement streaming and non-streaming — both modes fully supported
9. Tool use with programmatic calling — disable_user_input field supported
10. Handle all error cases — network, API errors, rate limits

## Technical Stack

11. HTTP client: reqwest with rustls
12. Async runtime: tokio
13. Serialization: serde, serde_json
14. Error handling: thiserror for library, anyhow for examples
15. Streaming: tokio-stream, futures for Stream trait
16. SSE parsing: eventsource-stream or custom implementation
17. Testing: tokio::test for async tests

## Code Quality

18. All public APIs have doc comments with examples
19. Integration tests with real API (gated by ANTHROPIC_API_KEY env)
20. Unit tests for parsing, error handling, builders
21. No unwrap() in library code — always propagate errors
22. Use tracing for structured logging, not println!
23. Clippy clean with deny(warnings) in CI

## Non-Goals

24. NOT a wrapper around TypeScript SDK
25. NOT supporting Claude 2 legacy API
26. NOT implementing client-side prompt caching (API feature)
27. NOT building a REPL or chat interface (examples only)
```

---

## Session Start Protocol

At the start of every session:

1. **Check git status**: `git status` and `git log --oneline -10`
2. **Read system files**: `rules.txt`, `features.json`, `history.txt`
3. **Identify focus**: Find features with `in_progress` or next `new` feature by priority
4. **Announce plan**: State what you'll work on and why

---

## Implementation Roadmap

### Phase 1: Foundation (SDK001-SDK003)

Start with a minimal working client:

```rust
// src/lib.rs
pub mod client;
pub mod types;
pub mod error;

// src/client.rs
use reqwest::Client;

pub struct ClaudeClient {
    http: Client,
    api_key: String,
    api_version: String,
}

impl ClaudeClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            api_key: api_key.into(),
            api_version: "2023-06-01".into(),
        }
    }

    pub async fn send_message(
        &self,
        request: MessagesRequest,
    ) -> Result<MessagesResponse, Error> {
        // POST to https://api.anthropic.com/v1/messages
    }
}
```

### Phase 2: Streaming (SDK004-SDK005)

Add SSE parsing for streaming responses:

```rust
pub async fn send_streaming(
    &self,
    request: MessagesRequest,
) -> Result<impl Stream<Item = Result<StreamEvent, Error>>, Error> {
    // Parse SSE stream
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    MessageStart { message: MessageMetadata },
    ContentBlockStart { index: usize, content_block: ContentBlock },
    ContentBlockDelta { index: usize, delta: ContentDelta },
    ContentBlockStop { index: usize },
    MessageDelta { delta: MessageDelta, usage: Usage },
    MessageStop,
}
```

### Phase 3: Tool Use (SDK006-SDK007)

Implement tool definitions and multi-turn conversations:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_user_input: Option<bool>,  // KEY FIELD
}

pub struct ConversationBuilder {
    messages: Vec<Message>,
    tools: Vec<Tool>,
    system: Option<String>,
}

impl ConversationBuilder {
    pub fn add_user_message(&mut self, content: impl Into<String>) -> &mut Self;
    pub fn add_tool_result(&mut self, tool_use_id: String, result: serde_json::Value) -> &mut Self;
    pub fn with_tool(&mut self, tool: Tool) -> &mut Self;
    pub fn build(&self) -> MessagesRequest;
}
```

### Phase 4: Polish (SDK008-SDK012)

Add token counting, retry logic, comprehensive tests, and documentation.

---

## Key API Details

### Authentication

```
POST https://api.anthropic.com/v1/messages
Headers:
  anthropic-version: 2023-06-01
  x-api-key: sk-ant-...
  content-type: application/json
```

### Request Format

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "max_tokens": 1024,
  "messages": [
    {
      "role": "user",
      "content": "Hello, Claude!"
    }
  ]
}
```

### Streaming Response (SSE)

```
event: message_start
data: {"type":"message_start","message":{"id":"msg_123",...}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: message_stop
data: {"type":"message_stop"}
```

### Tool Use Flow

```json
// User message with tool
{
  "model": "claude-3-5-sonnet-20241022",
  "max_tokens": 1024,
  "tools": [
    {
      "name": "get_weather",
      "description": "Get weather for a location",
      "input_schema": {
        "type": "object",
        "properties": {
          "location": {"type": "string"}
        }
      },
      "disable_user_input": true  // Programmatic!
    }
  ],
  "messages": [{"role": "user", "content": "What's the weather in NYC?"}]
}

// Assistant response with tool_use
{
  "role": "assistant",
  "content": [
    {"type": "text", "text": "Let me check the weather for you."},
    {
      "type": "tool_use",
      "id": "toolu_123",
      "name": "get_weather",
      "input": {"location": "New York City"}
    }
  ]
}

// User provides tool_result
{
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_123",
      "content": "72°F, sunny"
    }
  ]
}
```

---

## Getting Started

The first task is to create the workspace structure:

```bash
cargo init --lib
# Add dependencies to Cargo.toml
# Create src/client.rs, src/types.rs, src/error.rs
# Implement basic HTTP POST to /v1/messages
# Test with a simple user message
```

Then progressively add features per the roadmap.

---

## Success Criteria

The port is complete when:

✅ Can send non-streaming messages and receive typed responses
✅ Can stream responses with proper SSE parsing
✅ Supports tool definitions with `disable_user_input`
✅ Handles multi-turn conversations with tool use
✅ Has comprehensive error handling and retry logic
✅ Includes examples and integration tests
✅ Documentation is clear and thorough
✅ Can be published to crates.io

---

## Integration with Colony Shell

Once complete, Colony Shell's `colony-agent` crate will use this as:

```rust
// colony-agent/Cargo.toml
[dependencies]
claude-sdk = "0.1"  # This port

// colony-agent/src/lib.rs
use claude_sdk::{ClaudeClient, ConversationBuilder};

pub struct Agent {
    claude: ClaudeClient,
    tools: ToolRegistry,
    protocol: ProtocolEmitter,
}

impl Agent {
    pub async fn process_input(&mut self, input: String) -> Result<()> {
        let mut conversation = ConversationBuilder::new()
            .with_system(self.get_system_prompt())
            .add_user_message(input);

        // Add tools with classification
        for tool in self.tools.iter() {
            conversation.with_tool(Tool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.schema.clone(),
                disable_user_input: Some(tool.is_programmatic()),
            });
        }

        let request = conversation.build();
        let mut stream = self.claude.send_streaming(request).await?;

        while let Some(event) = stream.next().await {
            match event? {
                StreamEvent::ContentBlockDelta { delta, .. } => {
                    self.protocol.emit_agent_thought(delta.text)?;
                }
                StreamEvent::MessageStop => break,
                _ => {}
            }
        }

        Ok(())
    }
}
```

---

## Notes

- Focus on correctness first, optimization later
- The API is simple — most complexity is in proper typing
- SSE parsing is straightforward with eventsource-stream
- Tool use requires careful state management for multi-turn conversations
- Programmatic tool calling is just one boolean field

Good luck! Start by reading the tracking files and implementing SDK001-SDK003.
