# Claude Agent SDK Rust Port - Initialization Prompt

You are the coding agent for the Rust port of Claude's Agent SDK. This is a native Rust implementation of the Claude API client with streaming support, tool execution, and programmatic tool calling.

## Project Context

This port is being developed as part of Colony Shell (F015 - Claude ADK integration), but can be used standalone. The goal is NOT to wrap the TypeScript SDK via FFI, but to implement a clean, idiomatic Rust client that directly integrates with the Claude Messages API.

**Key URLs:**
- Claude API Reference: https://docs.anthropic.com/en/api/messages
- Programmatic Tool Calling: https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling
- Streaming: https://docs.anthropic.com/en/api/messages-streaming

## Tracking Official SDKs for Updates

To keep our Rust port in sync with official API changes, we track the canonical SDKs maintained by Anthropic.

### Official SDK Repositories

**TypeScript SDK (Primary Reference):**
- Repo: https://github.com/anthropics/anthropic-sdk-typescript
- Package: `@anthropic-ai/sdk` on npm
- Why: Most actively maintained, first to receive updates
- Watch: Releases, commits to `src/resources/messages.ts`

**Python SDK (Secondary Reference):**
- Repo: https://github.com/anthropics/anthropic-sdk-python
- Package: `anthropic` on PyPI
- Watch: `src/anthropic/resources/messages.py`

**API Changelog:**
- URL: https://docs.anthropic.com/en/api/changelog
- RSS: Subscribe for notifications of API version bumps

### What to Track

Create `.claude/system/upstream.json` to track API compatibility:

```json
{
  "last_checked": "2025-12-11",
  "api_version": "2023-06-01",
  "typescript_sdk_version": "0.28.0",
  "python_sdk_version": "0.35.0",
  "tracked_endpoints": [
    {
      "endpoint": "/v1/messages",
      "method": "POST",
      "last_schema_hash": "abc123...",
      "fields_added": [],
      "fields_deprecated": []
    }
  ],
  "pending_updates": []
}
```

### Automation Strategy

#### 1. GitHub Actions Workflow

Create `.github/workflows/upstream-check.yml`:

```yaml
name: Check Upstream SDKs

on:
  schedule:
    - cron: '0 0 * * 1'  # Weekly on Monday
  workflow_dispatch:      # Manual trigger

jobs:
  check-upstream:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Fetch TypeScript SDK
        run: |
          npm view @anthropic-ai/sdk version > ts-version.txt

      - name: Fetch Python SDK
        run: |
          pip index versions anthropic | head -1 > py-version.txt

      - name: Check API Changelog
        run: |
          curl -s https://docs.anthropic.com/en/api/changelog | \
          grep -A 5 "## 20" | head -20 > changelog.txt

      - name: Compare with tracked versions
        run: |
          ./scripts/check-upstream-changes.sh

      - name: Create issue if outdated
        if: failure()
        uses: actions/github-script@v7
        with:
          script: |
            github.rest.issues.create({
              owner: context.repo.owner,
              repo: context.repo.repo,
              title: 'Upstream SDK updates detected',
              body: 'New API changes found. See CI logs.',
              labels: ['upstream-sync', 'needs-review']
            })
```

#### 2. Schema Validation Script

Create `scripts/check-upstream-changes.sh`:

```bash
#!/bin/bash
set -e

# Fetch TypeScript SDK types
curl -s https://raw.githubusercontent.com/anthropics/anthropic-sdk-typescript/main/src/resources/messages.ts > /tmp/ts-messages.ts

# Extract type definitions using tree-sitter or regex
grep -A 50 "interface MessageCreateParams" /tmp/ts-messages.ts > /tmp/ts-types.txt

# Compare with our Rust types
cargo run --bin schema-validator -- compare /tmp/ts-types.txt src/types.rs

# Exit with error if differences found
if [ $? -ne 0 ]; then
  echo "Schema differences detected!"
  echo "Check docs/UPSTREAM_CHANGES.md for details"
  exit 1
fi
```

#### 3. Schema Validator Tool

Create `src/bin/schema-validator.rs`:

```rust
//! Validates our Rust types against TypeScript SDK

use std::collections::HashMap;

fn main() {
    // Parse TypeScript interface definitions
    // Compare with our serde struct fields
    // Report missing fields, type mismatches, new additions

    let ts_fields = parse_typescript_types("ts-messages.ts");
    let rust_fields = parse_rust_types("src/types.rs");

    let diff = compare_schemas(ts_fields, rust_fields);

    if !diff.is_empty() {
        println!("Schema differences found:");
        for change in diff {
            println!("  - {}", change);
        }
        std::process::exit(1);
    }
}
```

### Tracking Changes in System Files

When upstream changes are detected:

**1. Update `upstream.json`:**
```json
{
  "pending_updates": [
    {
      "detected_on": "2025-12-11",
      "source": "typescript-sdk-0.29.0",
      "changes": [
        {
          "type": "field_added",
          "location": "MessagesRequest",
          "field": "metadata",
          "rust_type": "Option<HashMap<String, String>>"
        }
      ],
      "issue_url": "https://github.com/you/claude-sdk-rust/issues/42"
    }
  ]
}
```

**2. Update `features.json`:**
```json
{
  "id": "SDK013",
  "name": "Metadata field support",
  "description": "Add optional metadata field to MessagesRequest (added in TS SDK 0.29.0)",
  "status": "new",
  "priority": 2,
  "dependencies": ["SDK002"],
  "notes": [
    "Upstream: TypeScript SDK 0.29.0",
    "API version: 2023-06-01 (no change)",
    "Field: metadata: Option<HashMap<String, String>>"
  ]
}
```

**3. Log in `history.txt`:**
```
[2025-12-11 10:00] UPSTREAM: TypeScript SDK 0.29.0 released
[2025-12-11 10:05] UPSTREAM: New field detected: MessagesRequest.metadata
[2025-12-11 10:10] PROGRESS: Created SDK013 for metadata support
```

### Manual Monitoring (Until Automation Complete)

**Weekly checklist:**
1. Check https://github.com/anthropics/anthropic-sdk-typescript/releases
2. Check https://docs.anthropic.com/en/api/changelog
3. Run: `npm view @anthropic-ai/sdk version`
4. Run: `pip index versions anthropic | head -1`
5. Compare with `.claude/system/upstream.json`
6. Create feature tickets for new capabilities

### Notification Strategy

**GitHub Watch Settings:**
- Watch TypeScript SDK repo: Releases only
- Watch Python SDK repo: Releases only
- Subscribe to API changelog RSS in feed reader

**Slack/Discord Webhook (Optional):**
```yaml
# In CI workflow
- name: Notify on changes
  if: failure()
  run: |
    curl -X POST $SLACK_WEBHOOK \
      -H 'Content-Type: application/json' \
      -d '{"text":"Claude SDK upstream changes detected!"}'
```

### Testing Against Official API

Create `tests/compatibility.rs`:

```rust
//! Compatibility tests against official Claude API
//! Only run when ANTHROPIC_API_KEY is set

#[tokio::test]
#[ignore]  // Manual only
async fn test_parity_with_typescript_sdk() {
    // Same request sent via TS SDK and our Rust SDK
    // Compare responses field-by-field
    // Ensures we parse responses identically
}
```

### Documentation of Differences

Maintain `docs/UPSTREAM_DIFFERENCES.md`:

```markdown
# Known Differences from Official SDKs

## Intentional Deviations

- **Error types**: We use thiserror instead of JavaScript Error classes
- **Streaming**: We use tokio Stream instead of AsyncIterator
- **Builder pattern**: More idiomatic in Rust than chaining

## Feature Parity Status

| Feature | TS SDK | Py SDK | Rust SDK | Notes |
|---------|--------|--------|----------|-------|
| Basic messages | ✅ | ✅ | ✅ | Complete |
| Streaming | ✅ | ✅ | ✅ | Complete |
| Tool use | ✅ | ✅ | ✅ | Complete |
| Prompt caching | ✅ | ✅ | 🚧 | Planned SDK014 |
| Vision (images) | ✅ | ✅ | 🚧 | Planned SDK015 |

## Version Compatibility Matrix

| API Version | TS SDK | Py SDK | Rust SDK |
|-------------|--------|--------|----------|
| 2023-01-01 | 0.1.x  | 0.2.x  | ❌ |
| 2023-06-01 | 0.20.x | 0.25.x | ✅ 0.1.x |
```

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
    },
    {
      "id": "SDK013",
      "name": "Amazon Bedrock support",
      "description": "Support Claude models via AWS Bedrock. Implement AWS SigV4 signing, bedrock-runtime endpoint, handle different request/response envelope format.",
      "status": "new",
      "priority": 3,
      "dependencies": ["SDK003", "SDK005"],
      "notes": [
        "Bedrock uses AWS SigV4 authentication instead of API keys",
        "Endpoint: bedrock-runtime.{region}.amazonaws.com",
        "Request/response wrapped in Bedrock envelope",
        "Use aws-config and aws-sigv4 crates"
      ]
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
11. Support Amazon Bedrock deployment — AWS SigV4 auth, different endpoint
12. Abstract over Anthropic vs Bedrock differences — unified client interface

## Technical Stack

13. HTTP client: reqwest with rustls
14. Async runtime: tokio
15. Serialization: serde, serde_json
16. Error handling: thiserror for library, anyhow for examples
17. Streaming: tokio-stream, futures for Stream trait
18. SSE parsing: eventsource-stream or custom implementation
19. AWS integration: aws-config, aws-sigv4, aws-types for Bedrock
20. Testing: tokio::test for async tests

## Code Quality

21. All public APIs have doc comments with examples
22. Integration tests with real API (gated by ANTHROPIC_API_KEY and AWS_PROFILE env)
23. Unit tests for parsing, error handling, builders
24. No unwrap() in library code — always propagate errors
25. Use tracing for structured logging, not println!
26. Clippy clean with deny(warnings) in CI

## Non-Goals

27. NOT a wrapper around TypeScript SDK
28. NOT supporting Claude 2 legacy API
29. NOT implementing client-side prompt caching (API feature)
30. NOT building a REPL or chat interface (examples only)
31. NOT supporting Azure OpenAI or other Claude deployments (Anthropic + Bedrock only)
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

## Amazon Bedrock Support (SDK013)

### Overview

Amazon Bedrock provides Claude models via AWS infrastructure. Key differences from Anthropic API:

| Aspect | Anthropic API | AWS Bedrock |
|--------|---------------|-------------|
| **Authentication** | API key (x-api-key header) | AWS SigV4 signing |
| **Endpoint** | api.anthropic.com | bedrock-runtime.{region}.amazonaws.com |
| **Request format** | Direct JSON | Wrapped in Bedrock envelope |
| **Response format** | Direct JSON | Wrapped in Bedrock envelope |
| **Model IDs** | `claude-3-5-sonnet-20241022` | `anthropic.claude-3-5-sonnet-20241022-v2:0` |
| **Streaming** | SSE | AWS event stream |

### Client Interface Design

Unified client that abstracts over both backends:

```rust
use aws_config::BehaviorVersion;

pub enum ClaudeBackend {
    Anthropic { api_key: String },
    Bedrock { region: String },
}

pub struct ClaudeClient {
    backend: ClaudeBackend,
    http: reqwest::Client,
}

impl ClaudeClient {
    /// Create client for Anthropic API
    pub fn anthropic(api_key: impl Into<String>) -> Self {
        Self {
            backend: ClaudeBackend::Anthropic {
                api_key: api_key.into(),
            },
            http: reqwest::Client::new(),
        }
    }

    /// Create client for AWS Bedrock
    pub async fn bedrock(region: impl Into<String>) -> Result<Self> {
        // Load AWS credentials from environment/config
        let _config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(region.into()))
            .load()
            .await;

        Ok(Self {
            backend: ClaudeBackend::Bedrock {
                region: region.into(),
            },
            http: reqwest::Client::new(),
        })
    }

    /// Send message (works with both backends)
    pub async fn send_message(
        &self,
        request: MessagesRequest,
    ) -> Result<MessagesResponse> {
        match &self.backend {
            ClaudeBackend::Anthropic { .. } => {
                self.send_anthropic(request).await
            }
            ClaudeBackend::Bedrock { .. } => {
                self.send_bedrock(request).await
            }
        }
    }
}
```

### Bedrock Request Envelope

Bedrock wraps requests:

```rust
// Internal type, not exposed to users
#[derive(Serialize)]
struct BedrockInvokeRequest {
    #[serde(flatten)]
    messages_request: MessagesRequest,

    // Bedrock-specific fields
    anthropic_version: String,
}

// Bedrock wraps responses
#[derive(Deserialize)]
struct BedrockInvokeResponse {
    #[serde(flatten)]
    messages_response: MessagesResponse,

    // Bedrock metadata
    #[serde(rename = "amazon-bedrock-invocationMetrics")]
    metrics: Option<BedrockMetrics>,
}
```

### AWS SigV4 Signing

Use `aws-sigv4` crate for request signing:

```rust
use aws_sigv4::http_request::{sign, SigningSettings, SigningParams};
use aws_credential_types::Credentials;

async fn send_bedrock(&self, request: MessagesRequest) -> Result<MessagesResponse> {
    let region = match &self.backend {
        ClaudeBackend::Bedrock { region } => region,
        _ => unreachable!(),
    };

    // Load AWS credentials
    let config = aws_config::load_from_env().await;
    let credentials = config.credentials_provider()
        .expect("credentials required")
        .provide_credentials()
        .await?;

    // Build HTTP request
    let url = format!(
        "https://bedrock-runtime.{}.amazonaws.com/model/{}/invoke",
        region,
        normalize_model_id(&request.model)
    );

    let body = serde_json::to_vec(&BedrockInvokeRequest {
        messages_request: request,
        anthropic_version: "bedrock-2023-05-31".into(),
    })?;

    // Sign request with SigV4
    let signing_params = SigningParams::builder()
        .access_key(credentials.access_key_id())
        .secret_key(credentials.secret_access_key())
        .region(region)
        .service_name("bedrock")
        .build()?;

    let signable_request = /* ... build signable request ... */;
    let signed = sign(signable_request, &signing_params)?;

    // Send signed request
    let response = self.http
        .post(&url)
        .headers(signed.headers())
        .body(body)
        .send()
        .await?;

    // Unwrap Bedrock envelope
    let bedrock_response: BedrockInvokeResponse = response.json().await?;
    Ok(bedrock_response.messages_response)
}
```

### Model ID Normalization

```rust
fn normalize_model_id(model: &str) -> String {
    // Anthropic: claude-3-5-sonnet-20241022
    // Bedrock:   anthropic.claude-3-5-sonnet-20241022-v2:0

    if model.starts_with("anthropic.") {
        model.to_string()
    } else {
        // Convert Anthropic format to Bedrock format
        format!("anthropic.{}", model)
    }
}
```

### Bedrock Streaming

Bedrock uses AWS event stream format (not SSE):

```rust
// For streaming, Bedrock uses invoke-with-response-stream
async fn send_bedrock_streaming(
    &self,
    request: MessagesRequest,
) -> Result<impl Stream<Item = Result<StreamEvent>>> {
    let url = format!(
        "https://bedrock-runtime.{}.amazonaws.com/model/{}/invoke-with-response-stream",
        region,
        normalize_model_id(&request.model)
    );

    // Sign and send request
    // Parse AWS event stream (different from SSE)
    // Convert to our StreamEvent enum
}
```

### Feature Flag (Optional)

Gate Bedrock support behind feature flag for minimal builds:

```toml
[features]
default = ["anthropic"]
anthropic = []
bedrock = ["aws-config", "aws-sigv4", "aws-types", "aws-credential-types"]
full = ["anthropic", "bedrock"]
```

### Testing Bedrock

```rust
#[tokio::test]
#[ignore]  // Requires AWS credentials
async fn test_bedrock_basic_message() {
    let client = ClaudeClient::bedrock("us-east-1")
        .await
        .expect("AWS credentials required");

    let request = MessagesRequest {
        model: "claude-3-5-sonnet-20241022".into(),
        max_tokens: 100,
        messages: vec![Message::user("Hello")],
        ..Default::default()
    };

    let response = client.send_message(request).await.unwrap();
    assert!(!response.content.is_empty());
}
```

### Documentation

Users can choose backend at runtime:

```rust
// Using Anthropic API
let client = ClaudeClient::anthropic(std::env::var("ANTHROPIC_API_KEY")?);

// Using AWS Bedrock
let client = ClaudeClient::bedrock("us-east-1").await?;

// Same interface for both!
let response = client.send_message(request).await?;
```

---

## Success Criteria

The port is complete when:

✅ Can send non-streaming messages and receive typed responses
✅ Can stream responses with proper SSE parsing
✅ Supports tool definitions with `disable_user_input`
✅ Handles multi-turn conversations with tool use
✅ Has comprehensive error handling and retry logic
✅ Supports both Anthropic API and AWS Bedrock backends
✅ Unified client interface abstracts over backend differences
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
