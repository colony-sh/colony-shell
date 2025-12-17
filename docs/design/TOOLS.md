# Tool Execution Model

This document describes how Colony Shell handles tool execution with the programmatic tool calling pattern from the Claude API.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                       Colony Shell                           │
│                                                              │
│  ┌──────────────┐           ┌─────────────────┐            │
│  │ Claude Agent │◄─────────►│  Tool Registry  │            │
│  │   (F015)     │           │                 │            │
│  └──────┬───────┘           └────────┬────────┘            │
│         │                             │                      │
│         │ Tool Call Request           │                      │
│         ▼                             ▼                      │
│  ┌──────────────────────────────────────────┐              │
│  │       Tool Execution Router              │              │
│  │  ┌────────────────┬──────────────────┐  │              │
│  │  │ PROGRAMMATIC   │  USER_FACING     │  │              │
│  │  │ (auto-execute) │  (needs approval)│  │              │
│  │  └────┬───────────┴─────┬────────────┘  │              │
│  └───────┼──────────────────┼───────────────┘              │
│          │                  │                               │
│          │                  │  Approval Request             │
│          │                  └─────────────────────────────┐ │
│          │                                                 │ │
│          ▼                                                 ▼ │
│  ┌──────────────┐                              ┌──────────┴─┴───┐
│  │ Tuple Space  │                              │ OSC 2337 Widget │
│  │ Memory Ops   │                              │ Approval UI     │
│  │ MCP Executor │                              └─────────────────┘
│  └──────────────┘                                        │
│                                                           │
└───────────────────────────────────────────────────────────┼──┘
                                                            │
                                     Approval Response      │
                                     (Approve/Deny/Modify) ◄┘
                                     via colony-terminal
```

## Tool Classification

### PROGRAMMATIC Tools (disable_user_input: true)

Automatically execute without user approval:

- **Memory operations**: `memory.store()`, `memory.retrieve()`, `memory.search()`
- **Tuple space operations**: `tuple.out()`, `tuple.read()`, `tuple.take()`
- **Read-only queries**: `fs.read()`, `db.query()`, `api.get()`
- **Safe computations**: `math.calculate()`, `text.transform()`
- **Agent coordination**: `agent.send_message()`, `checkpoint.save()`

### USER_FACING Tools (disable_user_input: false)

Require user approval via terminal widget:

- **File system writes**: `fs.write()`, `fs.delete()`, `fs.chmod()`
- **External API calls**: `http.post()`, `email.send()`
- **Destructive operations**: `db.delete()`, `process.kill()`
- **Credential access**: `secrets.get()`, `keychain.read()`
- **Network operations**: `ssh.connect()`, `docker.run()`

## Implementation in colony-agent Crate

```rust
use colony_protocol::ProtocolMessage;

/// Tool execution classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolClassification {
    /// Auto-execute without user approval
    Programmatic,
    /// Require user approval via terminal
    UserFacing,
}

/// Tool descriptor from MCP server
pub struct Tool {
    pub name: String,
    pub schema: serde_json::Value,
    pub classification: ToolClassification,
}

/// Tool execution request
pub struct ToolCall {
    pub tool: String,
    pub arguments: serde_json::Value,
    pub classification: ToolClassification,
}

/// Tool execution router
pub struct ToolExecutor {
    registry: ToolRegistry,
    protocol: ProtocolEmitter,
}

impl ToolExecutor {
    pub async fn execute(&self, call: ToolCall) -> Result<serde_json::Value> {
        match call.classification {
            ToolClassification::Programmatic => {
                // Execute immediately
                self.execute_immediate(call).await
            }
            ToolClassification::UserFacing => {
                // Request approval via terminal
                self.request_approval_and_execute(call).await
            }
        }
    }

    async fn request_approval_and_execute(&self, call: ToolCall) -> Result<serde_json::Value> {
        // Emit OSC 2337 approval request widget
        let approval_id = uuid::Uuid::new_v4();
        self.protocol.emit(ProtocolMessage::Widget {
            kind: "tool_approval".into(),
            data: serde_json::json!({
                "approval_id": approval_id,
                "tool": call.tool,
                "arguments": call.arguments,
                "classification": "user_facing"
            })
        })?;

        // Wait for approval response via tuple space
        let response = self.wait_for_approval(approval_id).await?;

        match response {
            ApprovalResponse::Approved => self.execute_immediate(call).await,
            ApprovalResponse::Modified(args) => {
                let modified_call = ToolCall { arguments: args, ..call };
                self.execute_immediate(modified_call).await
            }
            ApprovalResponse::Denied(reason) => {
                Err(ToolError::UserDenied(reason))
            }
        }
    }
}
```

## MCP Server Manifest Extension

MCP servers can declare tool classification in their manifest:

```json
{
  "tools": [
    {
      "name": "memory.store",
      "classification": "programmatic",
      "schema": { ... }
    },
    {
      "name": "fs.write",
      "classification": "user_facing",
      "schema": { ... }
    }
  ]
}
```

If not specified, default to `user_facing` for safety.

## Terminal Approval Widget

The terminal (colony-terminal) receives approval requests via OSC 2337:

```
ESC ] 2337 ; Widget={"kind":"tool_approval","data":{...}} ST
```

And displays an interactive approval UI:

```
┌─────────────────────────────────────────────────────┐
│ 🤖 Agent wants to execute: fs.write()               │
├─────────────────────────────────────────────────────┤
│ File:    /home/user/project/config.json             │
│ Content: { "api_key": "...", "endpoint": "..." }    │
│                                                      │
│ [ Approve ]  [ Modify ]  [ Deny ]  [ Always Allow ] │
└─────────────────────────────────────────────────────┘
```

User response is sent back to shell via:
- IPC channel (if same process)
- Tuple space (if cross-process)
- STDIN (if different terminal session)

## Security Considerations

1. **Default deny**: Unknown tools default to USER_FACING
2. **Audit logging**: All tool executions logged to memory substrate
3. **Rate limiting**: Prevent approval fatigue attacks
4. **Privilege escalation**: Programmatic tools cannot spawn USER_FACING tools
5. **Sandboxing**: MCP executor runs in rustyscript sandbox

## Claude API Integration

When calling Claude API with tools:

```rust
// Programmatic tools
let request = MessagesRequest {
    tools: vec![
        Tool { name: "memory.store", disable_user_input: true },
        Tool { name: "tuple.out", disable_user_input: true },
    ],
    // ...
};

// User-facing tools
let request = MessagesRequest {
    tools: vec![
        Tool { name: "fs.write", disable_user_input: false },
        Tool { name: "http.post", disable_user_input: false },
    ],
    // ...
};
```

The `disable_user_input` parameter is set based on our classification.

## Future Extensions

- **Permission policies**: `.colonyrc` can override classifications
- **Trust on first use**: Remember user decisions
- **Dry-run mode**: Show what tools would do without executing
- **Tool composition**: Programmatic tools can call other programmatic tools
