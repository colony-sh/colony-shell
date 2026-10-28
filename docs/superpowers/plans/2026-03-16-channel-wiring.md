# Channel Wiring — Connect Producers to fd 3-6

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the actual data producers (agent, MCP, debugger, context system) into the channel infrastructure so fd 3-6 carry real data during shell operation.

**Architecture:** Each producer writes to its respective channel via the ChannelManager already stored on the Executor. No new crates needed — this is integration wiring across existing crates.

**Tech Stack:** Rust, existing colony-channels, colony-agent, colony-mcp, colony-debug, colony-exec

---

## File Structure (existing files only — no new files)

**Bug fix:**
- `crates/colony-channels/src/pipe.rs` — add fd 3 to `new_standard()`

**Agent → fd 4 (stdllm):**
- `crates/colony-agent/src/agent.rs` — write conversations to LlmChannel after each turn

**MCP → fd 5 (stdtool):**
- `crates/colony-mcp/src/executor.rs` (or wherever tool calls are dispatched) — log requests/results to ToolChannel

**Debug → fd 6 (stddiag):**
- `crates/colony-debug/src/controller.rs` — write TraceEvents to DiagChannel

**Channel builtins:**
- `crates/colony-exec/src/builtins.rs` — add `llm`, `tool`, `diag` show/export builtins

---

## Chunk 1: Fix fd 3 Pipe + Wire Agent to fd 4

### Task 1: Fix fd 3 pipe bug

**Files:**
- Modify: `crates/colony-channels/src/pipe.rs`

- [ ] **Step 1: Find `new_standard()` in pipe.rs**

Read the function. It currently creates pipes for fd 4, 5, 6 but NOT fd 3. The constant `STDCONTEXT` (value 3) should be included.

- [ ] **Step 2: Add fd 3 to the pipe creation**

Add `STDCONTEXT` to the array of FDs that get pipes created in `new_standard()`. Follow the exact pattern used for fd 4-6.

- [ ] **Step 3: Test**

Run: `cargo test -p colony-channels`

- [ ] **Step 4: Commit**

```bash
git commit -m "fix(channels): include fd 3 (stdcontext) in ChannelPipes::new_standard()"
```

### Task 2: Wire agent conversations to LlmChannel (fd 4)

**Files:**
- Modify: `crates/colony-agent/src/agent.rs`
- Modify: `crates/colony-exec/src/lib.rs` (pass ChannelManager reference to agent)

The agent already manages a `Vec<Message>` for conversations. After each user message and assistant response, write to the LlmChannel.

- [ ] **Step 1: Read the agent struct to understand the conversation flow**

Read `crates/colony-agent/src/agent.rs`. Find where:
- User messages are added to the conversation
- Assistant responses are received
- Tool calls/results are processed

- [ ] **Step 2: Add a channel write method to the agent**

The agent needs access to a ChannelManager (or specifically the LlmChannel write handle). The cleanest approach:

Add an optional `channel_writer: Option<tokio::sync::mpsc::UnboundedSender<(u8, String)>>` to the Agent struct. The executor feeds this on init. The agent writes `(4, json)` tuples when messages arrive.

Alternatively, if the agent already has access to the executor's channel manager, use it directly.

- [ ] **Step 3: Write user messages to fd 4**

After adding a user message to the conversation:
```rust
if let Some(ref tx) = self.channel_writer {
    let msg = serde_json::json!({
        "role": "user",
        "content": content,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    let _ = tx.send((4, msg.to_string()));
}
```

- [ ] **Step 4: Write assistant responses to fd 4**

After receiving a complete assistant response:
```rust
if let Some(ref tx) = self.channel_writer {
    let msg = serde_json::json!({
        "role": "assistant",
        "content": response_text,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    let _ = tx.send((4, msg.to_string()));
}
```

- [ ] **Step 5: Wire the channel writer from the executor**

In `colony-exec/src/lib.rs`, when the agent is created or invoked, pass a channel sender that the executor reads from and forwards to `self.channels.write_to(4, data)`.

Or simpler: pass a `&mut ChannelManager` to the agent invocation and write directly.

The exact approach depends on the current agent invocation pattern. Read the code to determine the best integration point.

- [ ] **Step 6: Test**

Run: `cargo check -p colony-agent && cargo check -p colony-exec`

- [ ] **Step 7: Commit**

```bash
git commit -m "feat(channels): wire agent conversations to LlmChannel (fd 4)"
```

---

## Chunk 2: Wire MCP to fd 5 + Debug to fd 6

### Task 3: Wire MCP tool calls to ToolChannel (fd 5)

**Files:**
- Modify: MCP executor/session code (find the right file)

- [ ] **Step 1: Find where MCP tool calls are dispatched**

Search for `tool_call`, `execute_tool`, or `call_tool` in `crates/colony-mcp/`. Find the function where:
- A tool request is sent to an MCP server
- A tool result is received back

- [ ] **Step 2: Log tool requests to fd 5**

Before dispatching a tool call:
```rust
let request_json = serde_json::json!({
    "type": "request",
    "tool": tool_name,
    "arguments": arguments,
    "request_id": request_id,
    "timestamp": chrono::Utc::now().to_rfc3339()
});
// Write to ToolChannel via channel_writer or ChannelManager
```

- [ ] **Step 3: Log tool results to fd 5**

After receiving a tool result:
```rust
let result_json = serde_json::json!({
    "type": "result",
    "tool": tool_name,
    "request_id": request_id,
    "success": success,
    "output": output_text,
    "duration_ms": duration.as_millis(),
    "timestamp": chrono::Utc::now().to_rfc3339()
});
```

- [ ] **Step 4: Test**

Run: `cargo check -p colony-mcp`

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(channels): wire MCP tool calls to ToolChannel (fd 5)"
```

### Task 4: Wire debugger trace events to DiagChannel (fd 6)

**Files:**
- Modify: `crates/colony-debug/src/controller.rs`

- [ ] **Step 1: Add DiagChannel writer to DebugController**

Add an optional channel writer to DebugController:
```rust
diag_writer: Option<Box<dyn Fn(String) + Send + Sync>>,
```

Or accept a closure/callback during construction.

- [ ] **Step 2: In `emit()`, also write to fd 6**

When emitting a TraceEvent (which currently goes to the TUI via mpsc), also serialize to JSON and write to the diag channel:

```rust
pub fn emit(&self, event: TraceEvent) {
    let _ = self.event_tx.send(event.clone());

    // Also write to stddiag (fd 6) for channel consumers
    if let Some(ref writer) = self.diag_writer {
        if let Ok(json) = serde_json::to_string(&event) {
            writer(json);
        }
    }
}
```

- [ ] **Step 3: Wire from executor**

When the executor creates the DebugController, pass a closure that writes to `self.channels.write_to(6, data)`.

- [ ] **Step 4: Test**

Run: `cargo check -p colony-debug && cargo check -p colony-exec`

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(channels): wire debug trace events to DiagChannel (fd 6)"
```

---

## Chunk 3: Channel Show Builtins

### Task 5: Add llm, tool, diag show builtins

**Files:**
- Modify: `crates/colony-exec/src/builtins.rs`

- [ ] **Step 1: Add `llm` builtin**

```bash
llm show              # Pretty-print recent conversation
llm show --last 5     # Last 5 messages
llm export            # JSON export to stdout
llm clear             # Clear conversation history
```

Implementation: access `channels.llm()`, call `messages()` or `last_messages()`, format and print.

- [ ] **Step 2: Add `tool` builtin**

```bash
tool show             # Show recent tool calls with results
tool show --pending   # Show unanswered requests
tool export           # JSON export
tool clear            # Clear tool log
```

Implementation: access `channels.tool()`, call `entries()` or `pending_requests()`.

- [ ] **Step 3: Add `diag` builtin**

```bash
diag show                    # Show recent diagnostics
diag show --level warn       # Filter by severity
diag show --source debugger  # Filter by source
diag export                  # JSON export
diag clear                   # Clear
```

Implementation: access `channels.diag()`, call `entries_at_level()` or `entries_from_source()`.

- [ ] **Step 4: Register in builtin dispatcher**

Add "llm", "tool", "diag" to `is_builtin_base()` and the match dispatch in `execute_builtin()`.

- [ ] **Step 5: Test**

Run: `cargo check -p colony-exec`

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(channels): add llm, tool, diag show/export/clear builtins"
```
