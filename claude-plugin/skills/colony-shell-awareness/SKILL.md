---
name: colony-shell-awareness
description: Use when COLONY_SHELL=1 env var is present, when the user mentions Colony Shell, when you need to persist observations across sessions, or when you need to read shell session state (context, channels, agent status). Activates in Colony Shell terminals.
---

# Colony Shell Awareness

You are running inside **Colony Shell**, an AI-native POSIX shell with extended channels, a context management system, and integrated LLM agent support.

## Environment Detection

Check for Colony Shell: `[ -n "$COLONY_SHELL" ]`

If `COLONY_SHELL=1` is set, you have access to all Colony builtins below.

## Available Builtins

### Context Management
```bash
context show                    # Show all context entries
context show --tokens           # Show with token counts
context add "observation text"  # Add a knowledge entry
context add --file path/to/file # Add file content to context
context remove <id-prefix>     # Remove an entry
context pin <id-prefix>        # Pin entry (won't be auto-pruned)
context clear                   # Clear non-pinned entries
context prune 50000            # Prune to fit token budget
```

Context persists across sessions in `~/.colony/context/` per project.

### Agent Status
```bash
agent status                    # Provider, model, tokens, context utilization
agent model                     # Current provider:model
agent switch anthropic:claude-sonnet-4-5-20250514  # Switch model
agent reset                     # Clear conversation, keep system prompt
agent tools                     # List registered tools with classifications
agent history 10                # List recent saved conversations
agent config                    # Full configuration dump
```

### Channel Inspection
```bash
llm show                        # Recent LLM conversation messages
llm show --last 5               # Last 5 messages
llm export                      # JSON export of conversation
tool show                       # Recent MCP tool calls and results
tool show --pending             # Unanswered tool requests
diag show                       # Diagnostic entries
diag show --level warn          # Filter by severity
diag show --source debugger     # Filter by source
```

### Channel Writing
```bash
context add "discovered: auth module uses JWT with Redis refresh tokens"
echo "note about this project" >&3    # Write to stdcontext (fd 3)
```

## Best Practices

1. **Start sessions** by reading context: `context show` to understand what's known
2. **Persist discoveries** with `context add` — these survive across sessions
3. **Check agent state** with `agent status` before making LLM-related decisions
4. **Review channel history** with `llm show`, `tool show` when you need session context
5. **Pin important context** with `context pin` so it survives pruning

## Special Files

Colony Shell loads these files automatically when entering a directory:
- `AGENT.md` — agent personality, capabilities, tools
- `CONTEXT.md` — project context (loaded as seed context)
- `.colonyrc` — project configuration with trust model
