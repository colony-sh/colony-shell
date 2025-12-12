# Colony Shell - Quick Start Guide

## Running Colony Shell

### 1. Build the Shell

```bash
cargo build --release --bin colony
```

### 2. Run Without Agent (Shell-only mode)

```bash
./target/release/colony
```

You'll see:
```
Colony Shell v0.1.0
POSIX-compliant shell with AI agent integration

⚠️  Agent mode disabled (set ANTHROPIC_API_KEY to enable)
   Type 'exit' to quit

shell>
```

### 3. Run With Agent

```bash
export ANTHROPIC_API_KEY=sk-ant-...
./target/release/colony
```

You'll see:
```
Colony Shell v0.1.0
POSIX-compliant shell with AI agent integration

🤖 Agent mode enabled (ANTHROPIC_API_KEY found)
   Use # or ? prefix for natural language queries
   Type 'exit' to quit

colony>
```

## Using the Agent

### Natural Language Queries

Prefix your query with `#` or `?`:

```bash
colony> # what is rust ownership
```

Or ask questions naturally:
```bash
colony> what files are in this directory
colony> how do I compile this project
colony> explain the colony-parser crate
```

### Shell Commands

Just type normal commands:
```bash
colony> ls -la
colony> cd /tmp
colony> echo hello
```

Currently shows: `(Shell execution not yet implemented)`

## Testing the Agent Integration

### Test Streaming Responses

```bash
colony> # tell me a story about a shell
```

Watch the response stream in real-time, character by character.

### Test Detection Logic

```bash
# These route to agent:
colony> # what is this
colony> ? how do I
colony> what is the weather
colony> can you help me

# These route to shell parser:
colony> ls -la
colony> echo hello
colony> cd /tmp
```

### Test Error Handling

```bash
# Without API key:
./target/release/colony
shell> # this won't work
Error: parser error: unexpected end of input

# With invalid API key:
export ANTHROPIC_API_KEY=invalid
./target/release/colony
colony> # test
Agent error: Claude API error: ...
```

## Current Capabilities

✅ **Agent Integration**
- Claude 3.5 Sonnet via claude-sdk
- Streaming responses in real-time
- Natural language detection
- Conversation state management
- Prompt caching enabled

✅ **Tool System**
- PROGRAMMATIC vs USER_FACING classification
- Tool registry (empty, no tools registered yet)
- Approval request framework (TODO)

❌ **Not Yet Implemented**
- Shell command execution (F001-F007 needed)
- Tool registration (MCP integration F016-F018)
- Memory substrate (F012-F013)
- OSC 2337 protocol emission (F009)

## Next Steps

To make Colony Shell fully functional, we need:

1. **F001-F002**: Implement lexer and parser (currently stubs)
2. **F004**: Command execution via fork/exec
3. **F009**: OSC 2337 protocol for terminal communication
4. **F016-F018**: MCP integration for tool registration

Try it now and watch the agent respond!
