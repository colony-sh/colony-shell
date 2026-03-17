---
name: colony-channel-awareness
description: Use when you need to understand session history (previous commands, LLM interactions, tool calls), persist information for future sessions, or inspect what happened during the current Colony Shell session via extended channels fd 3-6.
---

# Colony Channel Awareness

Colony Shell has 4 extended channels (fd 3-6) that accumulate structured session data. They are viewable in the TUI sidebar and via builtins.

## Channels

| Channel | FD | Builtin | Purpose |
|---------|----|---------|---------|
| stdcontext | 3 | `context` | Session observations, decisions, learned facts |
| stdllm | 4 | `llm` | LLM conversation messages (user, assistant, system, tool) |
| stdtool | 5 | `tool` | MCP tool call requests and results |
| stddiag | 6 | `diag` | Diagnostic entries with severity levels |

## Reading Channels

```bash
# Context — what's been observed this session
context show                    # All entries
context show --tokens           # With token estimates

# LLM — conversation history
llm show                        # All messages with role badges
llm show --last 5               # Recent messages
llm export                      # Full JSON export

# Tools — MCP call log
tool show                       # All calls with results
tool show --pending             # Unanswered requests

# Diagnostics — structured logs
diag show                       # All entries
diag show --level warn          # Warnings and errors only
diag show --source debugger     # From specific source
```

## Writing to Channels

```bash
# Structured (preferred)
context add "discovered: uses PostgreSQL with Neon"

# Raw fd writes (from scripts or external commands)
echo "session note" >&3         # Write to stdcontext
```

## Channel Data Flow

Channels are populated automatically:
- **fd 4 (stdllm):** Agent writes every user message and assistant response
- **fd 5 (stdtool):** MCP executor logs every tool request and result with timing
- **fd 6 (stddiag):** Executor logs command executions; debugger writes trace events
- **fd 3 (stdcontext):** User and agent write observations via `context add`

## Sidebar Panels

The TUI right sidebar shows live channel data:
- Context panel, LLM panel, Tools panel, Diagnostics panel
- Toggle with `Ctrl+N` (right sidebar)
- Panels update in real-time as channel data arrives

## Persistence

- Channels are **session-scoped** — data clears when the shell exits
- To persist across sessions, use `context add` (writes to `~/.colony/context/` via MessagePack)
- To export channel data: `llm export > conversation.json`, `diag export > diagnostics.json`
