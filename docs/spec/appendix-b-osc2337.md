# Appendix B: OSC 2337 Protocol

## B.1 Overview

OSC 2337 is Colony's custom Operating System Command sequence for shell-terminal communication. It enables rich interactions between the shell process and a compatible terminal emulator (colony-terminal).

### B.1.1 Why OSC?

- **Standard mechanism**: OSC sequences are part of ECMA-48/ANSI X3.64
- **Terminal-safe**: Non-compatible terminals ignore unknown OSC sequences
- **Bidirectional**: Terminal can respond via stdin
- **No external IPC**: Works through existing stdin/stdout channels

### B.1.2 Sequence Format

```
ESC ] 2337 ; <command> ; <payload> BEL
│   │      │          │           │
│   │      │          │           └── Terminator (0x07 or ESC \)
│   │      │          └── Command-specific JSON or text
│   │      └── Command identifier
│   └── OSC introducer (0x1B 0x5D)
└── Escape character (0x1B)
```

## B.2 Command Categories

### B.2.1 Agent Commands

Commands for agent status and interaction:

| Command | Direction | Description |
|---------|-----------|-------------|
| `Agent=status` | Shell→Terminal | Agent execution status |
| `Agent=thinking` | Shell→Terminal | Agent is processing |
| `Agent=streaming` | Shell→Terminal | Streaming LLM response |
| `Agent=tool` | Shell→Terminal | Tool call notification |
| `Agent=complete` | Shell→Terminal | Agent finished |
| `Agent=error` | Shell→Terminal | Agent error |

### B.2.2 Widget Commands

Commands for interactive widgets:

| Command | Direction | Description |
|---------|-----------|-------------|
| `Widget=approval` | Shell→Terminal | Request tool approval |
| `Widget=input` | Shell→Terminal | Request user input |
| `Widget=select` | Shell→Terminal | Multi-choice selection |
| `Widget=progress` | Shell→Terminal | Progress indicator |
| `Widget=response` | Terminal→Shell | Widget response |

### B.2.3 Buffer Commands

Commands for buffer management:

| Command | Direction | Description |
|---------|-----------|-------------|
| `Buffer=create` | Shell→Terminal | Create named buffer |
| `Buffer=write` | Shell→Terminal | Write to buffer |
| `Buffer=clear` | Shell→Terminal | Clear buffer |
| `Buffer=show` | Shell→Terminal | Display buffer in pane |
| `Buffer=hide` | Shell→Terminal | Hide buffer pane |

### B.2.4 Context Commands

Commands for context channel visualization:

| Command | Direction | Description |
|---------|-----------|-------------|
| `Context=add` | Shell→Terminal | Add context entry |
| `Context=update` | Shell→Terminal | Update entry |
| `Context=remove` | Shell→Terminal | Remove entry |
| `Context=clear` | Shell→Terminal | Clear all context |

### B.2.5 Metadata Commands

Output metadata for enhanced display:

| Command | Direction | Description |
|---------|-----------|-------------|
| `Meta=type` | Shell→Terminal | Output type annotation |
| `Meta=schema` | Shell→Terminal | JSON schema for output |
| `Meta=fold` | Shell→Terminal | Foldable section |
| `Meta=highlight` | Shell→Terminal | Syntax highlighting hint |

## B.3 Payload Specifications

### B.3.1 Agent=status

```json
{
  "id": "agent-123",
  "name": "code-reviewer",
  "state": "running|waiting|complete|error",
  "message": "Analyzing code...",
  "progress": 0.45,
  "elapsed_ms": 1234
}
```

### B.3.2 Agent=streaming

```json
{
  "id": "agent-123",
  "delta": "partial text content",
  "token_count": 42
}
```

### B.3.3 Agent=tool

```json
{
  "id": "agent-123",
  "call_id": "call-456",
  "tool": "read_file",
  "arguments": {"path": "/src/main.rs"},
  "classification": "programmatic|user_facing",
  "status": "pending|executing|complete|error"
}
```

### B.3.4 Widget=approval

Request format:
```json
{
  "id": "approval-789",
  "tool": "write_file",
  "arguments": {"path": "/etc/config", "content": "..."},
  "risk_level": "low|medium|high",
  "preview": "Will overwrite /etc/config with new content",
  "options": ["approve", "deny", "modify"]
}
```

Response format (Terminal→Shell):
```json
{
  "id": "approval-789",
  "action": "approve|deny|modify",
  "modifications": {}
}
```

### B.3.5 Widget=input

Request format:
```json
{
  "id": "input-001",
  "prompt": "Enter your API key:",
  "type": "text|password|multiline",
  "default": "",
  "validation": "^sk-[a-zA-Z0-9]+$"
}
```

Response format:
```json
{
  "id": "input-001",
  "value": "sk-abc123...",
  "cancelled": false
}
```

### B.3.6 Widget=select

Request format:
```json
{
  "id": "select-002",
  "prompt": "Choose a model:",
  "options": [
    {"value": "sonnet", "label": "Claude Sonnet", "description": "Fast and capable"},
    {"value": "opus", "label": "Claude Opus", "description": "Most capable"}
  ],
  "multi": false,
  "default": "sonnet"
}
```

Response format:
```json
{
  "id": "select-002",
  "selected": ["sonnet"],
  "cancelled": false
}
```

### B.3.7 Buffer=write

```json
{
  "name": "llm-output",
  "content": "Response text...",
  "append": true,
  "format": "plain|markdown|json|code"
}
```

### B.3.8 Context=add

```json
{
  "id": "ctx-001",
  "type": "observation|decision|action|fact|user",
  "content": "User prefers TypeScript for new projects",
  "source": {
    "command": "context --type user",
    "agent": null,
    "file": null
  },
  "importance": 0.8,
  "ttl_secs": null
}
```

### B.3.9 Meta=type

```json
{
  "type": "List[{name: Str, size: Int}]",
  "count": 42
}
```

### B.3.10 Meta=fold

```json
{
  "id": "fold-001",
  "label": "Stack trace (23 lines)",
  "collapsed": true,
  "start": true
}
```

## B.4 Implementation

### B.4.1 Rust Emitter

```rust
// crates/colony-protocol/src/osc.rs

use std::io::Write;

const OSC_START: &str = "\x1b]2337;";
const OSC_END: &str = "\x07";

/// Emit an OSC 2337 sequence
pub fn emit<W: Write>(writer: &mut W, command: &str, payload: &str) -> std::io::Result<()> {
    write!(writer, "{}{};{}{}", OSC_START, command, payload, OSC_END)?;
    writer.flush()
}

/// Emit agent status
pub fn emit_agent_status<W: Write>(
    writer: &mut W,
    id: &str,
    name: &str,
    state: AgentState,
    message: &str,
) -> std::io::Result<()> {
    let payload = serde_json::json!({
        "id": id,
        "name": name,
        "state": state.as_str(),
        "message": message,
    });
    emit(writer, "Agent=status", &payload.to_string())
}

/// Emit streaming delta
pub fn emit_streaming_delta<W: Write>(
    writer: &mut W,
    id: &str,
    delta: &str,
    token_count: usize,
) -> std::io::Result<()> {
    let payload = serde_json::json!({
        "id": id,
        "delta": delta,
        "token_count": token_count,
    });
    emit(writer, "Agent=streaming", &payload.to_string())
}

/// Request tool approval widget
pub fn request_approval<W: Write>(
    writer: &mut W,
    id: &str,
    tool: &str,
    arguments: &serde_json::Value,
    preview: &str,
) -> std::io::Result<()> {
    let payload = serde_json::json!({
        "id": id,
        "tool": tool,
        "arguments": arguments,
        "preview": preview,
        "options": ["approve", "deny", "modify"],
    });
    emit(writer, "Widget=approval", &payload.to_string())
}
```

### B.4.2 Response Reader

```rust
// crates/colony-protocol/src/reader.rs

use std::io::BufRead;

/// Read OSC 2337 response from stdin
pub fn read_widget_response<R: BufRead>(reader: &mut R) -> std::io::Result<WidgetResponse> {
    let mut line = String::new();
    reader.read_line(&mut line)?;

    // Parse OSC sequence
    if line.starts_with("\x1b]2337;Widget=response;") {
        let payload_start = line.find('{').ok_or(io::ErrorKind::InvalidData)?;
        let payload_end = line.rfind('}').ok_or(io::ErrorKind::InvalidData)?;
        let json = &line[payload_start..=payload_end];

        serde_json::from_str(json)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "Not an OSC response"))
    }
}

#[derive(Debug, Deserialize)]
pub struct WidgetResponse {
    pub id: String,
    #[serde(flatten)]
    pub data: WidgetResponseData,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum WidgetResponseData {
    Approval { action: String, modifications: Option<serde_json::Value> },
    Input { value: String, cancelled: bool },
    Select { selected: Vec<String>, cancelled: bool },
}
```

## B.5 Terminal Implementation Notes

### B.5.1 Parsing

Terminals should:
1. Buffer input until complete OSC sequence received
2. Parse command and payload
3. Dispatch to appropriate handler
4. Ignore unknown commands gracefully

### B.5.2 Widget Rendering

Widgets should be rendered as overlays or inline elements:
- Approval: Modal dialog with approve/deny buttons
- Input: Inline text field
- Select: Dropdown or radio group
- Progress: Status bar or spinner

### B.5.3 Response Timing

- Widget responses should be sent immediately on user action
- Timeout after 5 minutes for approval widgets
- Timeout after 1 minute for input widgets

## B.6 Fallback Behavior

When running in a non-Colony terminal:

1. OSC sequences are ignored (standard behavior)
2. Shell falls back to stdin/stdout prompts
3. Approval requests print to stderr and read stdin
4. Progress indicators use simple text updates

```rust
impl ApprovalHandler {
    pub fn request_approval(&self, tool: &str, args: &Value) -> bool {
        if self.is_colony_terminal() {
            self.request_approval_osc(tool, args)
        } else {
            // Fallback to text prompt
            eprintln!("Tool '{}' requires approval.", tool);
            eprintln!("Arguments: {}", serde_json::to_string_pretty(args).unwrap());
            eprint!("Approve? [y/N]: ");

            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            input.trim().to_lowercase() == "y"
        }
    }
}
```

## B.7 Security Considerations

### B.7.1 Payload Validation

- All JSON payloads must be validated against schemas
- Large payloads should be rejected (max 1MB)
- Path arguments must be sanitized

### B.7.2 Widget Trust

- Only the shell process should emit OSC 2337
- Terminal should verify sender (via pty)
- Approval widgets must display full tool arguments

### B.7.3 Response Authenticity

- Responses should include request ID for correlation
- Timeout responses should be clearly marked
- No automatic approval for timed-out requests

## B.8 Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | Dec 2025 | Initial specification |

---

*Return to [Overview](0-overview.md)*
