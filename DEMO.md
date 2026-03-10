# Colony Shell — Advanced Workflows

Colony is a reactive, AI-native shell built in Rust. It combines a full POSIX-compatible shell with a TUI panel system, encrypted secrets, semantic memory, goal tracking, multi-provider LLM agents, and a Linda-style tuple space that lets every component react to changes in real time.

**20 crates. 56 builtins. 10 panel binaries. One shell.**

---

## 1. Reactive Panel System

Colony's core concept: live-updating panels that dock alongside your prompt and react to everything you do.

### Basic setup

```bash
sidebar left filetree              # File browser on the left
sidebar right gitstatus            # Git info on the right
sidebar bottom --outer logs-panel  # Full-width log viewer at bottom
```

### Hotkey groups — switchable workspaces

Multiple sidebar commands with the same hotkey form a group. Press the hotkey to toggle the entire group on/off:

```bash
# Dev view: Ctrl-B
sidebar left --hotkey ctrl-b filetree
sidebar right --hotkey ctrl-b preview

# Git view: Ctrl-G
sidebar left --hotkey ctrl-g gitstage
sidebar right --hotkey ctrl-g gitstatus
```

Press `Ctrl-B` for dev view. Press `Ctrl-G` for git view. They toggle independently.

### Tabbed panels

Panels at the same position auto-tab. Only the active tab renders; `[`/`]` cycles:

```bash
# Individual commands — same hotkey + same position = tabs
sidebar left --hotkey ctrl-b filetree
sidebar left --hotkey ctrl-b filedetails

# Or shorthand:
sidebar left --hotkey ctrl-b filetree filedetails
```

### Stacked panels

Show all panels at once, split vertically:

```bash
sidebar left --hotkey ctrl-b --stack filetree filedetails
```

### Sidebar options

```bash
sidebar <position> [options] <command...>
  --hotkey ctrl-b        # Toggle hotkey
  --tabs                 # Tab mode (default for multi-panel)
  --stack                # Stack mode (all visible)
  --weight 2             # Relative size
  --min-width 20         # Minimum size (columns/rows)
  --max-width 50         # Maximum size
  --outer                # Top/bottom: span full terminal width
```

### Available panels

| Panel | What it does |
|-------|-------------|
| `filetree` | Interactive file browser with git status icons, auto-refreshes on cd |
| `filedetails` | File metadata — size, permissions, timestamps |
| `preview` | File content preview with syntax detection |
| `gitstatus` | Branch, commit log, dirty files |
| `gitstage` | Interactive staging — j/k navigate, space to stage/unstage |
| `jobs-panel` | Background job tracker |
| `history-panel` | Searchable command history |
| `env-panel` | Environment variable browser |
| `logs-panel` | Scrolling output log |
| `session-panel` | Session stats and info |

### Navigation

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus between main and panel positions |
| `Esc` | Return focus to main prompt |
| `[` / `]` | Cycle tabs within focused panel group |
| `?` | Context-sensitive help modal |
| `j` / `k` | Navigate within panel |
| `space` | Select/toggle in gitstage |

---

## 2. Tuple Space — Reactive Coordination

Everything in colony coordinates through a Linda-style tuple space. When you `cd`, `export`, or run `git add`, tuples fire and every subscribed component reacts instantly.

### How it works

```
cd ~/projects/myapp
  → emits ("dir_change", session_id, "/old/path", "/Users/me/projects/myapp")
  → filetree panel receives cwd subscription, reloads tree
  → special files loader checks for .colony/init, AGENT.md, CONTEXT.md
  → git status cache invalidates

export API_URL=https://api.example.com
  → emits ("env_change", session_id, "API_URL", null, "https://api.example.com")
  → env-panel refreshes
  → any panel subscribed to env changes updates

sidebar left filetree
  → emits ("sidebar_command", '{"Add":{"position":"Left",...}}')
  → TUI picks it up and spawns the panel process
```

### Tuple operations (5 Linda primitives)

| Operation | Behavior |
|-----------|----------|
| `out(tuple)` | Write — non-blocking |
| `inp(pattern)` | Read + remove — non-blocking, returns None if no match |
| `rdp(pattern)` | Read without removing — non-blocking |
| `in(pattern)` | Read + remove — **blocking** until match |
| `rd(pattern)` | Read without removing — **blocking** |

### Panel subscriptions

Panels declare what tuples they care about in their `---CONFIG---` block:

```json
{"subscriptions": ["cwd", "git", "env"], "help": ["j/k: navigate", "space: stage"]}
```

When a matching tuple fires, the panel receives it as an input event and can re-render.

---

## 3. Secure Environment Variables

AES-GCM encrypted secrets backed by your OS keychain. Never in plain text on disk.

```bash
secure export API_KEY=sk-live-abc123def456   # Encrypt and store
secure list                                   # List secured vars
echo $API_KEY                                 # Decrypted on access
secure revoke API_KEY                         # Revoke approval
secure patterns                               # Auto-encryption patterns
```

How it works:
- Encryption key derived from OS keychain (macOS Keychain, Linux secret-service)
- Stored in `~/.colony/secure/` as encrypted blobs
- Session-scoped approvals — approve once per session
- Pattern-based auto-detection catches secrets before they hit disk

---

## 4. Recipes — Declarative LLM Modes

A recipe is a TOML file that defines a complete LLM configuration: system prompt, tools, skills, model preferences, and approval policies.

```toml
# ~/.colony/recipes/rust-expert.toml
[recipe]
name = "rust-expert"
version = "0.1.0"
imports = ["base"]

[prompt]
system = "You are a Rust systems programming expert..."
context_files = ["Cargo.toml", "README.md"]

[model]
provider = "anthropic"
name = "claude-3-5-sonnet"
temperature = 0.7

[tools.approval]
auto_approve = ["file_read", "grep"]
require_approval = ["execute_command"]
deny = ["rm", "format"]

[[commands]]
name = "/test"
action = "shell"
command = "cargo test"
```

Recipe search paths (highest priority first):
1. `.colony/recipes/` — Project-specific
2. `~/.colony/recipes/` — Personal
3. `~/.config/colony/recipes/` — Config dir
4. `~/.cache/colony/registry/` — Downloaded from Colony Cloud

Recipes compose: `imports = ["base"]` inherits and overrides.

---

## 5. Semantic Memory

Vector-embedded memory with importance decay, scoping, and LLM-assisted consolidation.

```bash
# Memory is stored automatically as you work:
# - Commands you run frequently
# - Decisions the agent makes
# - Context from conversations

# Scoped organization:
#   Global → Project → Session → Task

# Memory decays over time if unused
# Frequently accessed memories warm up (higher importance)
# Consolidation merges similar memories
```

Backend: LanceDB with 384-dimensional vectors (all-MiniLM-L6-v2 embeddings). Eviction policies: TTL, LRU, or composite.

---

## 6. Extended Channels (fd 3–6)

Beyond stdin/stdout/stderr, colony provides semantic channels for structured machine communication:

| FD | Channel | What flows through it |
|----|---------|----------------------|
| 0 | stdin | Standard input |
| 1 | stdout | Standard output |
| 2 | stderr | Standard errors |
| 3 | stdcontext | Session context, observations, decisions |
| 4 | stdllm | LLM conversation messages (role, content, tokens) |
| 5 | stdtool | Tool call requests and results |
| 6 | stddiag | Diagnostics (severity, category, message) |

```bash
# Redirect structured data to channels
echo '{"event":"deploy","status":"success"}' >&3

# Capture diagnostics
mycommand 6>/tmp/diag.log

# Agent reads context from fd 3 to understand shell state
```

---

## 7. Agent Framework

Multi-provider LLM integration with auto-detection, tool approval, and conversation persistence.

```bash
# Auto-detects from environment:
export ANTHROPIC_API_KEY=sk-ant-...   # Claude (primary)
export OPENAI_API_KEY=sk-...          # GPT (fallback)
# Or run Ollama locally — no API key needed

# Manage context
context add "Building a REST API with Axum"
context show
context clear
```

Features:
- **Provider fallback** — Anthropic → OpenAI → Ollama → Colony Cloud, with health tracking
- **Tool approval** — OSC 2337 widget in terminal, stderr fallback for simple terminals
- **Token budgets** — Configurable per-session limits with accounting
- **Conversation persistence** — SQLite-backed, resume across sessions
- **Streaming cancellation** — Ctrl+C cleanly stops mid-response
- **Tuple bridge** — Agent reads/writes to tuple space for reactive coordination
- **Recipe-driven** — Behavior configured via recipe TOML files

---

## 8. MCP (Model Context Protocol) Integration

Connect to MCP tool servers for extensible agent capabilities:

```toml
# .colonyrc
[[mcp.servers]]
name = "filesystem"
command = "mcp-filesystem"
args = ["--root", "/project"]
auto_start = true
```

Colony manages the MCP connection pool, introspects available tools, generates TypeScript client code, and routes tool calls through the approval system.

---

## 9. Special Files — Dynamic Project Context

Colony watches for context files and loads them reactively on `cd`:

| File | Purpose |
|------|---------|
| `AGENT.md` | Agent personality, capabilities, prompt |
| `CONTEXT.md` | Project context (API overview, schema, conventions) |
| `TOOLS.md` | Available tools and permissions |
| `PROMPT.md` | Prompt templates |
| `.colonyrc` | Project configuration (TOML, trust-checked like direnv) |
| `.colony/init` | Project-specific init script |

```bash
cd ~/projects/my-api
# → Loads AGENT.md → agent becomes an "API expert"
# → Loads CONTEXT.md → agent knows about your endpoints
# → Runs .colony/init → sets up project-specific panels
# → All via tuple space: ("dir_change", ...)
```

---

## 10. Goal Tracking

Hierarchical task management with status tracking and checkpoints:

```bash
goal add "Ship v1.0"                    # Create a goal
goal add "Write tests" --parent 1       # Nested under parent
goal complete 2                          # Mark done
goal block 1 --reason "Waiting on API"  # Mark blocked
goal tree                                # View hierarchy
goal checkpoint 1                        # Snapshot state

# Statuses: pending → in_progress → completed | blocked | cancelled
# Progress auto-calculates from children
```

Stored in `~/.colony/goals/<project-hash>.json`. Survives across sessions.

---

## 11. Flux Process Manager

Interactive process management with TUI, port detection, and resource monitoring:

```bash
flux --tui                    # Interactive TUI browser
flux --filter node            # Filter by name
flux --port 3000              # What's using port 3000?
flux --sort mem --resources   # Sort by memory usage
flux --filter zombie --nuke   # Kill matching processes
```

Emits structured data to fd 5 (stdtool) and tuples for reactive updates.

---

## 12. Reflection & Observation Database

Colony automatically records observations about your shell sessions:

```bash
mirror   # Tail and analyze recent observations
```

Recorded automatically:
- Commands executed (with timing)
- Errors encountered
- File activity (reads/writes)
- Agent conversations
- Token usage per model call

Session context enriches every observation: CWD, git branch, time of day, focus duration, energy level, project name.

Storage: `~/.colony/reflection/observations.db` (SQLite). Agents can query patterns across sessions.

---

## 13. OSC 2337 Protocol

Colony communicates with compatible terminals via an escape-code protocol:

```
ESC ] 2337 ; <JSON> BEL
```

Message types: Agent status, Widget rendering (tool approval), Memory operations, Syntax-highlighted output, Session checkpoints, File tree sync, Secure access requests.

Terminals that don't support it get graceful fallbacks (stderr prompts, plain text).

---

## 14. Path Management

First-class `path` builtin:

```bash
path add ~/.local/bin           # Append to PATH
path prepend /opt/homebrew/bin  # Prepend to PATH
path has ~/.cargo/bin           # Check membership (exit code)
path list                       # List all entries
path remove /usr/local/games   # Remove entry
```

Emits tuples on change so panels stay in sync.

---

## 15. Full Shell Features

Colony is a real POSIX-compatible shell with 56 builtins:

```bash
# Pipelines and redirects
cat file.txt | grep "pattern" | sort > output.txt
cmd 2>/dev/null

# Command substitution (nested)
echo "Branch: $(git branch --show-current)"
count=$(find . -name "*.rs" | wc -l)

# Job control
sleep 100 &    # Background
jobs            # List
fg %1           # Foreground
bg %1           # Background
disown %1       # Detach

# Directory stack
pushd /tmp && popd
dirs

# Conditionals and loops
if [ -f Cargo.toml ]; then cargo build; fi
for f in *.rs; do wc -l "$f"; done
while read line; do echo "$line"; done < input.txt

# Functions and aliases
alias ll="ls -la"
deploy() { cargo build --release && scp target/release/app server:/opt/; }

# POSIX expansion pipeline
echo ~                          # Tilde expansion
echo ${var:-default}            # Parameter expansion with modifiers
echo $((2 + 2))                 # Arithmetic expansion
echo *.rs                       # Pathname expansion (glob)
echo {a,b}{1,2}                 # Brace expansion

# Shell options
set -e          # Exit on error
set -o pipefail # Pipe failure propagation
set -x          # Xtrace debugging (customizable PS4)
shopt -s        # Shell options
```

---

## 16. Project Configuration (.colonyrc)

Per-project configuration with trust model (like direnv):

```toml
# .colonyrc
[agent]
personality = "You are a Rust expert focused on performance"
preferred_provider = "anthropic"
preferred_model = "claude-3-5-sonnet"

[project]
name = "my-api"
language = "rust"
conventions = ["Use parking_lot", "No unwrap in production"]

[[mcp.servers]]
name = "database"
command = "mcp-postgres"
args = ["--connection-string", "postgres://localhost/mydb"]

[tools]
classifications = { "file_read" = "programmatic", "execute_code" = "user_facing" }
```

---

## 17. Complete Setup Example

`~/.colony/init`:

```bash
# Encrypted secrets
secure load

# Dev workspace (Ctrl-B)
sidebar left --hotkey ctrl-b filetree filedetails
sidebar right --hotkey ctrl-b preview

# Git workspace (Ctrl-G)
sidebar left --hotkey ctrl-g gitstage
sidebar right --hotkey ctrl-g gitstatus

# Monitoring (Ctrl-L)
sidebar bottom --outer --hotkey ctrl-l logs-panel
sidebar top --outer --hotkey ctrl-j jobs-panel

# PATH management
path prepend /opt/homebrew/bin
path add ~/.cargo/bin
path add ./node_modules/.bin

# Aliases
alias gs="git status"
alias gc="git commit"
alias gd="git diff"
alias ll="ls -la"
```

Launch `colony` and you have a fully reactive, AI-augmented, panel-driven development environment in your terminal.

---

## Architecture

```
20 crates:
  colony-shell      — Main binary, TUI REPL, orchestrator
  colony-exec       — Command execution, 56 builtins, sidebar parser
  colony-parser     — POSIX shell parser (recursive descent)
  colony-expand     — Full expansion pipeline (tilde, param, cmd sub, glob)
  colony-panel      — Panel host, layout engine, protocol, slot management
  colony-tuple      — Linda-style tuple space (in-memory + SQLite)
  colony-secure     — AES-GCM encrypted env vars, OS keychain integration
  colony-agent      — Multi-provider LLM (Anthropic, OpenAI, Ollama)
  colony-memory     — Semantic memory with LanceDB vectors
  colony-channels   — Extended FD channels (fd 3-6)
  colony-recipe     — Declarative LLM mode configuration
  colony-config     — .colonyrc project configuration
  colony-focus      — Goal trees, session continuity
  colony-reflection — Observation database, session tracking
  colony-protocol   — OSC 2337 shell-terminal protocol
  colony-mcp        — Model Context Protocol integration
  colony-cloud-sdk  — Colony Cloud API client
  colony-filetree   — File tree state with git overlay + watcher
  colony-tui        — TUI framework utilities
  colony-ui-core    — Shared UI primitives

10 panel binaries:
  filetree, filedetails, preview, gitstatus, gitstage,
  jobs-panel, history-panel, env-panel, logs-panel, session-panel
```
