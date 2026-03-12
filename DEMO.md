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

Multiple sidebar commands with the same hotkey form a group. Press the hotkey to toggle the entire group on/off. Use different hotkeys to create entirely different workspace views you can switch between instantly:

```bash
# Dev view: Ctrl-B — browse code and preview files
sidebar left --hotkey ctrl-b filetree
sidebar right --hotkey ctrl-b preview

# Git view: Ctrl-G — stage changes and see status
sidebar left --hotkey ctrl-g gitstage
sidebar right --hotkey ctrl-g gitstatus

# Debug view: Ctrl-D — jobs, env vars, and logs
sidebar left --hotkey ctrl-d jobs-panel
sidebar right --hotkey ctrl-d env-panel
sidebar bottom --outer --hotkey ctrl-d logs-panel
```

Workflow: You're coding → press `Ctrl-B` to see the file tree and preview files. Ready to commit → press `Ctrl-B` to dismiss, press `Ctrl-G` to see staging + status. Something's wrong → press `Ctrl-G` to dismiss, press `Ctrl-D` to see jobs, env vars, and logs. Each hotkey group remembers its state independently.

### Tabbed panels

Panels at the same position auto-tab. Only the active tab renders; `[`/`]` cycles:

```bash
# Two left panels share a position → they become tabs
sidebar left --hotkey ctrl-b filetree
sidebar left --hotkey ctrl-b filedetails

# Or use the multi-command shorthand:
sidebar left --hotkey ctrl-b filetree filedetails
```

Now when you press `Ctrl-B`, the left panel shows a tab bar:

```
 filetree │ filedetails
┌─────────────────────┐
│ 📁 src/              │
│ 📁 tests/            │
│ 📄 Cargo.toml        │
│ 📄 README.md         │
└─────────────────────┘
```

Press `Tab` to focus the panel, then `]` to switch to `filedetails`:

```
 filetree │ filedetails
┌─────────────────────┐
│ Name: Cargo.toml     │
│ Size: 1.2 KB         │
│ Modified: 2 min ago  │
│ Perms: -rw-r--r--    │
└─────────────────────┘
```

### Stacked panels

Show all panels at once, splitting the space:

```bash
# Both visible simultaneously, stacked vertically in the left column
sidebar left --hotkey ctrl-b --stack filetree filedetails
```

```
┌─────────────────────┐
│ 📁 src/              │ ← filetree (top half)
│ 📄 Cargo.toml        │
├─────────────────────┤
│ Name: Cargo.toml     │ ← filedetails (bottom half)
│ Size: 1.2 KB         │
└─────────────────────┘
```

### Real-world panel layout: full-stack web development

```bash
# ~/.colony/init for a Next.js + Prisma project

# Code exploration (Ctrl-B) — tabbed filetree + details on left, preview on right
sidebar left --hotkey ctrl-b filetree filedetails
sidebar right --hotkey ctrl-b preview
sidebar bottom --outer --hotkey ctrl-b logs-panel

# Git workflow (Ctrl-G) — stage on left, status + history on right
sidebar left --hotkey ctrl-g gitstage
sidebar right --hotkey ctrl-g --stack gitstatus history-panel

# Monitoring (Ctrl-M) — jobs + env stacked on left, session info on right
sidebar left --hotkey ctrl-m --stack jobs-panel env-panel
sidebar right --hotkey ctrl-m session-panel

# Project PATH
path add ./node_modules/.bin
path add ./prisma
```

Session flow:
1. Open colony, `Ctrl-B` for code browsing → navigate files with j/k, preview renders on the right
2. Edit some files, `Ctrl-B` to dismiss, `Ctrl-G` for git → left panel shows diffs, space to stage
3. After committing, `Ctrl-G` to dismiss, `Ctrl-M` to check → see running jobs and env vars

### Sidebar options reference

```bash
sidebar <position> [options] <command...>
  --hotkey ctrl-b        # Toggle hotkey (ctrl/alt/shift + key, F1-F12)
  --tabs                 # Tab mode (default for multi-panel)
  --stack                # Stack mode (all visible, split evenly)
  --weight 2             # Relative size weight
  --min-width 20         # Minimum columns (left/right) or rows (top/bottom)
  --max-width 50         # Maximum size
  --outer                # Top/bottom only: span full terminal width

sidebar reset            # Remove all panels
sidebar reset left       # Remove panels at a position
```

### Available panels

| Panel | What it does |
|-------|-------------|
| `filetree` | Interactive file browser — git status icons, auto-refreshes on cd, j/k/Enter navigation |
| `filedetails` | File metadata — size, permissions, timestamps, line count |
| `preview` | File content preview — syntax detection, scrollable |
| `gitstatus` | Branch name, commit log, dirty/staged file list |
| `gitstage` | Interactive staging — j/k navigate files, space to stage/unstage, see inline diffs |
| `jobs-panel` | Background job tracker — PID, status, command, runtime |
| `history-panel` | Searchable command history — type to filter, Enter to re-execute |
| `env-panel` | Environment variable browser — sorted, searchable |
| `logs-panel` | Scrolling command output log — captures stdout from previous commands |
| `session-panel` | Session stats — uptime, command count, cwd, git branch |

### Navigation reference

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus: main → left group → right group → top → bottom → main |
| `Esc` | Return focus to main prompt |
| `[` / `]` | Cycle tabs within focused panel group |
| `?` | Context-sensitive help modal (shows panel-specific keys) |
| `j` / `k` | Navigate up/down within panel |
| `Enter` | Select/open in filetree, re-run in history |
| `space` | Stage/unstage in gitstage |

---

## 2. Tuple Space — Reactive Coordination

The tuple space is colony's nervous system. Every builtin that changes state emits a tuple. Every panel and subsystem can subscribe to patterns and react. This is how the whole shell stays in sync without polling.

### The cascade: what happens when you `cd`

```bash
cd ~/projects/my-api
```

This single command triggers a cascade through the tuple space:

```
1. cd builtin executes
   → emits ("dir_change", "session-abc", "/Users/me", "/Users/me/projects/my-api")

2. filetree panel (subscribed to "cwd")
   → receives tuple as InputEvent
   → scans new directory
   → renders fresh file listing

3. gitstatus panel (subscribed to "cwd")
   → detects new .git directory
   → runs git status, git log
   → renders branch + dirty files

4. special files loader
   → checks for AGENT.md, CONTEXT.md, .colony/init
   → if found: agent personality changes, new context loaded
   → runs .colony/init if present (project-specific panels)

5. git status cache
   → invalidates cached status for old directory
   → primes cache for new directory

6. session-panel (subscribed to "cwd")
   → updates displayed working directory

7. env-panel (subscribed to "cwd")
   → refreshes in case directory had .env loading
```

All of this happens within milliseconds of pressing Enter. No polling. No manual refresh.

### The cascade: what happens when you `export`

```bash
export DATABASE_URL=postgres://localhost/mydb
```

```
1. export builtin executes
   → sets the environment variable
   → emits ("env_change", "session-abc", "DATABASE_URL", null, "postgres://localhost/mydb")

2. env-panel (subscribed to "env")
   → re-sorts and re-renders with new variable highlighted

3. agent (if active, subscribed to "env")
   → notes the new variable in its context
   → can reference it in future responses

4. any panel subscribed to "*" (wildcard)
   → receives the tuple for logging/observation
```

### Tuple operations (5 Linda primitives)

| Operation | Blocking? | Removes? | Use case |
|-----------|-----------|----------|----------|
| `out(tuple)` | No | N/A | Write a tuple — builtins, panels, agent |
| `inp(pattern)` | No | Yes | Check for and consume a message |
| `rdp(pattern)` | No | No | Peek at a value without consuming |
| `in(pattern)` | **Yes** | Yes | Wait for a specific message |
| `rd(pattern)` | **Yes** | No | Wait and peek |

### Pattern matching with wildcards

```rust
// Match any directory change for this session
("dir_change", "session-abc", Wildcard, Wildcard)

// Match any environment change regardless of session
("env_change", Wildcard, Wildcard, Wildcard, Wildcard)

// Match a specific sidebar command
("sidebar_command", Wildcard)
```

### Panel protocol: how panels subscribe

Each panel binary is a child process that communicates via a structured protocol on stdin/stdout. Panels declare their subscriptions at startup:

```
---CONFIG---
{"subscriptions": ["cwd", "git", "env"], "help": ["j/k: navigate", "space: stage"]}
---CONFIG---
```

When a matching tuple fires, the panel host forwards it as a JSON input event on the panel's stdin:

```json
{"type": "Tuple", "data": {"key": "cwd", "value": "/Users/me/projects/my-api"}}
```

The panel re-renders and sends a new frame:

```
---FRAME---
{"text": "📁 src/", "indent": 0, "icon": "📁"}
{"text": "📄 Cargo.toml", "indent": 0, "icon": "📄"}
---FRAME---
```

This panel-as-process architecture means panels can be written in any language, crash without taking down the shell, and be hot-swapped.

---

## 3. Secure Environment Variables

AES-GCM encrypted secrets backed by your OS keychain. Never in plain text on disk, decrypted only when a command actually reads them.

### Scenario: setting up API keys for a multi-service project

```bash
# Store credentials — encrypted immediately, never written in plain text
secure export STRIPE_KEY=sk_live_abc123
secure export DATABASE_URL="postgres://user:pass@prod.db.example.com/myapp"
secure export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE
secure export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
secure export OPENAI_API_KEY=sk-proj-abc123def456

# List what's secured (values are masked)
secure list
# STRIPE_KEY          ••••••••••c123
# DATABASE_URL        ••••••••••yapp
# AWS_ACCESS_KEY_ID   ••••••••••MPLE
# AWS_SECRET_ACCESS_KEY ••••••••EKEY
# OPENAI_API_KEY      ••••••••••f456

# Use them transparently — decrypted on access
curl -H "Authorization: Bearer $STRIPE_KEY" https://api.stripe.com/v1/charges
psql $DATABASE_URL -c "SELECT count(*) FROM users"
aws s3 ls  # AWS CLI picks up the env vars automatically

# Revoke a specific approval this session
secure revoke STRIPE_KEY

# View auto-encryption patterns — colony detects secrets automatically
secure patterns
# *_KEY, *_SECRET, *_TOKEN, *_PASSWORD, DATABASE_URL, ...
```

### How it works under the hood

```
secure export STRIPE_KEY=sk_live_abc123
  1. Derives encryption key from OS keychain (macOS Keychain / Linux secret-service)
  2. Encrypts value with AES-256-GCM + random nonce
  3. Stores encrypted blob in ~/.colony/secure/STRIPE_KEY.enc
  4. Sets environment variable for current session

# On next shell startup:
secure load
  1. Reads all .enc files from ~/.colony/secure/
  2. Prompts for approval (once per session, per variable)
  3. Decrypts using keychain-derived key
  4. Sets environment variables
```

No plain text anywhere. If someone copies `~/.colony/secure/`, they can't decrypt without your OS keychain.

---

## 4. Recipes — Declarative LLM Modes

A recipe is a TOML file that defines a complete agent personality — system prompt, tools, skills, model preferences, approval policies, and custom commands. Think of it as a .vimrc for your AI assistant.

### Scenario: project-specific recipes for different contexts

```toml
# .colony/recipes/api-developer.toml — for your REST API project
[recipe]
name = "api-developer"
version = "0.1.0"
description = "Rust API development with Axum and SQLx"
imports = ["rust-base"]  # Inherit common Rust settings

[prompt]
system = """
You are an expert Rust backend developer specializing in:
- Axum web framework for HTTP services
- SQLx for async database operations
- Tower middleware for cross-cutting concerns
- OpenAPI spec generation

The project uses PostgreSQL. Check migrations/ for schema.
Always write tests. Prefer integration tests over unit tests.
"""
context_files = ["Cargo.toml", "README.md", "migrations/"]

[model]
provider = "anthropic"
name = "claude-3-5-sonnet"
temperature = 0.3
max_tokens = 8192

[tools.approval]
auto_approve = ["file_read", "grep", "cargo_check"]
require_approval = ["execute_command", "file_write"]
deny = ["rm -rf", "DROP TABLE", "git push --force"]

[[commands]]
name = "/test"
description = "Run the test suite"
action = "shell"
command = "cargo test -- --nocapture"

[[commands]]
name = "/migrate"
description = "Run database migrations"
action = "shell"
command = "sqlx migrate run"

[[commands]]
name = "/check"
description = "Full project check"
action = "shell"
command = "cargo clippy && cargo test && cargo doc --no-deps"

[[skills]]
name = "endpoint"
trigger = "when user asks to create an API endpoint"
prompt = """
When creating a new endpoint:
1. Add the handler function in src/handlers/
2. Add the route in src/router.rs
3. Add the request/response types in src/models/
4. Write an integration test in tests/
5. Update the OpenAPI spec
"""
```

```toml
# ~/.colony/recipes/rust-base.toml — shared foundation
[recipe]
name = "rust-base"
version = "0.1.0"

[prompt]
system = """
Follow Rust best practices:
- Use thiserror for library errors, anyhow for application errors
- Prefer &str over String in function parameters
- Use let-else for early returns, never nested if-let
- Run cargo clippy before suggesting code is done
"""

[model]
provider = "anthropic"
name = "claude-3-5-sonnet"

[tools.approval]
auto_approve = ["file_read", "cargo_check", "cargo_clippy"]
```

The recipe system composes: `api-developer` imports `rust-base`, inheriting its prompt rules and tool approvals, then adds its own project-specific overrides.

### Recipe search paths (highest priority first)

```
.colony/recipes/           # Project-specific — team-shared via git
~/.colony/recipes/          # Personal recipes
~/.config/colony/recipes/   # XDG config location
~/.cache/colony/registry/   # Downloaded from Colony Cloud registry
```

---

## 5. Semantic Memory

Colony maintains a vector-embedded memory system that remembers across sessions. Memories have importance scores that decay over time if unused and warm up when accessed frequently.

### How memory scoping works

```
Global memories
  └── Available everywhere, across all projects
      Example: "User prefers vim keybindings"
      Example: "Always use parking_lot instead of std::sync::Mutex"

Project memories
  └── Specific to a project directory
      Example: "This project uses Axum 0.7 with Tower middleware"
      Example: "The auth system uses JWT with RS256"

Session memories
  └── Current shell session only
      Example: "We're debugging the payment webhook handler"
      Example: "User asked to focus on test coverage"

Task memories
  └── Current goal/task context
      Example: "Fixing bug #123 — the issue is in src/handlers/payment.rs"
```

### Memory lifecycle

```
1. Agent conversation about database indexing
   → stores: "B-tree indexes on users.email and orders.created_at"
   → importance: 0.8, scope: project

2. Three days later, no access
   → importance decays: 0.8 → 0.6

3. User asks about query performance
   → agent retrieves memory via semantic search
   → importance warms up: 0.6 → 0.9
   → access_count incremented

4. Similar memory stored: "Added composite index on orders(user_id, created_at)"
   → consolidation detects overlap
   → merges into: "Database indexes: B-tree on users.email, composite on orders(user_id, created_at)"

5. Six months of no access
   → importance decays below threshold
   → LRU eviction removes it
```

Backend: LanceDB with 384-dimensional vectors via all-MiniLM-L6-v2 embeddings. Semantic search finds relevant memories even with different phrasing.

---

## 6. Extended Channels (fd 3–6)

Beyond stdin/stdout/stderr, colony provides four additional file descriptors for structured machine communication. These channels carry typed JSON data that agents, tools, and the reflection system can consume.

| FD | Channel | Data type | What flows through it |
|----|---------|-----------|----------------------|
| 0 | stdin | raw | Standard input |
| 1 | stdout | raw | Standard output |
| 2 | stderr | raw | Standard errors |
| 3 | stdcontext | JSON | Session observations, decisions, context signals |
| 4 | stdllm | JSON | LLM messages: `{role, content, tokens, timestamp}` |
| 5 | stdtool | JSON | Tool calls: `{request, result, status, timestamp}` |
| 6 | stddiag | JSON | Diagnostics: `{severity, category, message}` |

### Scenario: a deploy script that communicates through channels

```bash
#!/bin/bash
# deploy.sh — uses colony channels for structured logging

# Log the deployment decision to context channel
echo '{"event":"deploy_started","target":"production","commit":"abc123"}' >&3

# Run the build
if cargo build --release 2>&6; then  # Build errors go to diagnostics
    echo '{"severity":"info","category":"build","message":"Build succeeded"}' >&6
else
    echo '{"severity":"error","category":"build","message":"Build failed"}' >&6
    exit 1
fi

# Log tool invocations
echo '{"request":{"tool":"scp","args":["target/release/app","prod:/opt/"]},"status":"running"}' >&5
scp target/release/app prod:/opt/app
echo '{"request":{"tool":"scp"},"status":"completed"}' >&5

# Final context observation
echo '{"event":"deploy_completed","duration_ms":45000}' >&3
```

The agent can later query these channels to understand what happened:
- fd 3 observations feed the reflection database
- fd 5 tool logs help the agent learn which tools work
- fd 6 diagnostics surface errors before you notice them

---

## 7. Agent Framework

Colony ships with multi-provider LLM integration that auto-detects available providers and falls back gracefully.

### Scenario: setting up an agent-assisted development session

```bash
# Provider auto-detection — set what you have, colony figures out the rest
secure export ANTHROPIC_API_KEY=sk-ant-api03-...
secure export OPENAI_API_KEY=sk-proj-...
# Or run Ollama locally: ollama serve

# Agent tries providers in order:
#   1. Anthropic Claude (if ANTHROPIC_API_KEY set)
#   2. OpenAI GPT (if OPENAI_API_KEY set)
#   3. Ollama (if ollama is running locally)
#   4. Colony Cloud (if COLONY_API_KEY set)
# If primary fails, auto-fallback with health tracking

# Load project context for the agent
context add "We're building a REST API with Axum and SQLx"
context add "The database is PostgreSQL 15 on localhost"
context add "Auth uses JWT with RS256, keys in config/jwt/"

# Show loaded context
context show
# 1. We're building a REST API with Axum and SQLx
# 2. The database is PostgreSQL 15 on localhost
# 3. Auth uses JWT with RS256, keys in config/jwt/

# The agent also picks up:
#   - AGENT.md from current directory (personality)
#   - CONTEXT.md from current directory (project knowledge)
#   - Recipe configuration (tools, approval policies)
#   - Semantic memories from previous sessions
#   - Current file tree state, git branch, recent commands
```

### Tool approval workflow

When the agent wants to run a command, colony shows an approval widget:

```
┌─ Tool Approval ─────────────────────┐
│                                      │
│ Agent wants to execute:              │
│   cargo test -- --nocapture          │
│                                      │
│ [y] Approve  [n] Deny  [a] Always   │
└──────────────────────────────────────┘
```

Approval policies from recipes control this:
- `auto_approve`: runs immediately, no prompt
- `require_approval`: shows the widget
- `deny`: blocked, agent is told to try something else

### Tuple bridge — agent reacts to shell events

The agent subscribes to the tuple space. When you make changes, it notices:

```bash
git checkout -b feature/payment-webhooks
# → tuple: ("dir_change", ...) — agent notes you're on a feature branch

export STRIPE_WEBHOOK_SECRET=whsec_test_abc123
# → tuple: ("env_change", ...) — agent notes Stripe is being configured

# Agent's context now includes:
#   - You're on branch feature/payment-webhooks
#   - Stripe webhook secret is configured
#   - It can proactively suggest: "I see you're setting up payment webhooks.
#     Want me to scaffold the webhook handler?"
```

---

## 8. MCP (Model Context Protocol) Integration

Connect external tool servers to extend the agent's capabilities. Colony manages connection pools, tool introspection, and TypeScript client generation.

### Scenario: connecting a database MCP server for agent-assisted queries

```toml
# .colonyrc — project configuration
[[mcp.servers]]
name = "postgres"
command = "mcp-postgres"
args = ["--connection-string", "postgres://localhost/myapp"]
auto_start = true
env = { RUST_LOG = "warn" }

[[mcp.servers]]
name = "filesystem"
command = "mcp-filesystem"
args = ["--root", "."]
auto_start = true

[[mcp.servers]]
name = "github"
command = "mcp-github"
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
```

Colony's MCP lifecycle:
1. On startup, launches all `auto_start = true` servers as stdio subprocesses
2. Introspects each server's available tools and schemas
3. Generates typed TypeScript client code in `~/.colony/mcp-clients/<server>/client.ts`
4. Makes tools available to the agent through the approval system
5. Reconnects automatically if a server crashes

Now the agent can query your database, read/write files, and interact with GitHub — all through the tool approval workflow.

---

## 9. Special Files — Dynamic Project Context

Colony watches for context files and loads them reactively when you change directories. This is how projects configure the agent's personality and knowledge without any manual setup.

### Scenario: a monorepo with different contexts per directory

```
my-company/
├── AGENT.md              ← "You are a full-stack developer at Acme Corp"
├── CONTEXT.md            ← Company-wide conventions and API standards
├── .colonyrc             ← Shared MCP servers, model preferences
│
├── services/
│   ├── api/
│   │   ├── AGENT.md      ← "You specialize in the Rust API service"
│   │   ├── CONTEXT.md    ← API routes, database schema, auth flow
│   │   └── .colony/init  ← sidebar left gitstage; sidebar right gitstatus
│   │
│   ├── web/
│   │   ├── AGENT.md      ← "You specialize in the Next.js frontend"
│   │   ├── CONTEXT.md    ← Component library, state management, routing
│   │   └── .colony/init  ← sidebar left filetree; sidebar right preview
│   │
│   └── ml/
│       ├── AGENT.md      ← "You specialize in Python ML pipelines"
│       ├── CONTEXT.md    ← Model architecture, training config, data schema
│       └── .colony/init  ← sidebar bottom --outer logs-panel
```

```bash
cd services/api
# → Loads api/AGENT.md → agent becomes a Rust API specialist
# → Loads api/CONTEXT.md → agent knows your routes and schema
# → Runs api/.colony/init → git panels appear
# → All via tuple: ("dir_change", ...)

cd ../web
# → Drops api context instantly
# → Loads web/AGENT.md → agent becomes a Next.js specialist
# → Loads web/CONTEXT.md → agent knows your components
# → Runs web/.colony/init → file tree + preview panels appear

cd ../ml
# → Agent becomes a Python ML specialist
# → Knows your model architecture
# → Logs panel appears for training output
```

Each `cd` completely reshapes the agent's knowledge and the panel layout. No manual switching, no config files to edit. Just move between directories and colony adapts.

### Special files reference

| File | Loaded on | Purpose |
|------|-----------|---------|
| `AGENT.md` | `cd` | Agent personality — who it is, what it knows, how it behaves |
| `CONTEXT.md` | `cd` | Project knowledge — architecture, schema, conventions, gotchas |
| `TOOLS.md` | `cd` | Tool availability — what tools the agent can use here |
| `PROMPT.md` | `cd` | Prompt templates — structured prompts for common tasks |
| `.colonyrc` | `cd` | Configuration — model preferences, MCP servers, trust-checked |
| `.colony/init` | `cd` | Init script — shell commands to run (panels, aliases, path) |

---

## 10. Goal Tracking

Hierarchical task management built into the shell. Goals persist across sessions, auto-calculate progress from children, and support checkpoints for snapshots.

### Scenario: tracking a feature implementation

```bash
# Start a feature
goal add "Implement payment webhooks"
# → 1. [pending] Implement payment webhooks

# Break it down
goal add "Webhook endpoint handler" --parent 1
goal add "Signature verification" --parent 1
goal add "Event routing (charge.succeeded, charge.failed)" --parent 1
goal add "Idempotency check" --parent 1
goal add "Integration tests" --parent 1

# See the tree
goal tree
# 1. [pending] Implement payment webhooks (0%)
#   2. [pending] Webhook endpoint handler
#   3. [pending] Signature verification
#   4. [pending] Event routing (charge.succeeded, charge.failed)
#   5. [pending] Idempotency check
#   6. [pending] Integration tests

# Start working
goal complete 2
goal complete 3

goal tree
# 1. [in_progress] Implement payment webhooks (40%)   ← auto-calculated
#   2. [completed] Webhook endpoint handler              ✓
#   3. [completed] Signature verification                ✓
#   4. [pending] Event routing
#   5. [pending] Idempotency check
#   6. [pending] Integration tests

# Hit a blocker
goal block 4 --reason "Waiting for Stripe to enable webhook v2 in test mode"

# Checkpoint before context switch
goal checkpoint 1

# Come back later — goals survived the session restart
goal tree
# 1. [in_progress] Implement payment webhooks (40%)
#   2. [completed] Webhook endpoint handler              ✓
#   3. [completed] Signature verification                ✓
#   4. [blocked] Event routing — Waiting for Stripe...   ✗
#   5. [pending] Idempotency check
#   6. [pending] Integration tests

# Blocker resolved, continue
goal complete 4
goal complete 5
goal complete 6

goal tree
# 1. [completed] Implement payment webhooks (100%)      ✓
#   2. [completed] Webhook endpoint handler              ✓
#   3. [completed] Signature verification                ✓
#   4. [completed] Event routing                         ✓
#   5. [completed] Idempotency check                     ✓
#   6. [completed] Integration tests                     ✓
```

Goals are stored in `~/.colony/goals/<project-hash>.json` and persist across sessions. The agent can read goals to understand what you're working on and prioritize suggestions.

---

## 11. Flux Process Manager

Interactive process management with a TUI browser, port detection, and resource monitoring.

### Scenario: debugging a stuck development environment

```bash
# Something is eating CPU. What's going on?
flux --sort cpu --resources
# PID    NAME              CPU%   MEM%   RSS
# 12345  node              98.2   4.1    312MB
# 12346  webpack            2.1   1.2     92MB
# 12347  postgres           0.8   2.3    184MB

# That node process — is it holding a port?
flux --filter node --ports
# PID    NAME   PORT   STATE
# 12345  node   3000   LISTEN
# 12345  node   9229   LISTEN  (debugger)

# Kill it
flux --filter node --nuke

# Or use the interactive TUI to browse, filter, and kill
flux --tui
# ┌─ Flux Process Manager ──────────────────────┐
# │ Filter: [          ]                          │
# │                                               │
# │ PID    NAME        CPU%  MEM%  PORTS          │
# │ ▶12345 node        98.2  4.1   3000,9229      │
# │  12346 webpack      2.1  1.2                   │
# │  12347 postgres     0.8  2.3   5432            │
# │  12348 colony       0.1  0.4                   │
# │                                               │
# │ [k] Kill  [s] Signal  [f] Filter  [q] Quit   │
# └───────────────────────────────────────────────┘

# Is anything using port 5432?
flux --port 5432
# PID    NAME       PORT   STATE
# 12347  postgres   5432   LISTEN
```

Flux emits structured data to fd 5 (stdtool) and tuples to the tuple space, so the jobs-panel updates reactively.

---

## 12. Reflection & Observation Database

Colony silently records observations about your shell sessions — what you run, when you run it, what errors you hit, how long you focus. Over time, this builds a behavioral profile that agents can query.

### What gets recorded

```bash
# Every command is an observation
cargo test
# → {category: "Command", content: "cargo test",
#    context: {cwd: "/project", git_branch: "main",
#              time_of_day: "afternoon", command_count: 42,
#              focus_duration_ms: 1800000}}

# Errors are observed
cargo build 2>&1
# error[E0308]: mismatched types
# → {category: "Error", content: "mismatched types in src/main.rs:15",
#    context: {cwd: "/project", git_branch: "feature/auth"}}

# File activity is tracked
# → {category: "FileActivity", content: "modified src/main.rs",
#    context: {...}}

# Agent conversations are logged
# → {category: "AgentChat", content: "User asked about error handling patterns",
#    context: {tokens: 1247, model: "claude-3-5-sonnet"}}
```

### Querying observations

```bash
mirror
# Shows recent observations with timestamps:
#
# 14:23 [Command] cargo test -- payment
# 14:23 [Error] test payment::webhook_handler FAILED
# 14:24 [FileActivity] modified src/handlers/payment.rs
# 14:25 [Command] cargo test -- payment
# 14:25 [Command] ✓ All tests passed
# 14:30 [Command] git add -A && git commit -m "fix webhook handler"
```

The observation database is SQLite at `~/.colony/reflection/observations.db`. Agents can query patterns across sessions — "what errors do I usually hit on Monday mornings?" or "which files do I edit most in this project?"

---

## 13. OSC 2337 Protocol

Colony communicates with compatible terminals through an escape-code protocol for rich interactions that go beyond plain text.

### Protocol format

```
ESC ] 2337 ; <JSON_payload> BEL
\x1b]2337;{"type":"Agent","status":"thinking"}\x07
```

### Message types in action

```
# Agent is thinking...
\x1b]2337;{"type":"Agent","status":"thinking"}\x07

# Agent is streaming a response
\x1b]2337;{"type":"Agent","status":"streaming"}\x07

# Tool approval widget
\x1b]2337;{"type":"Widget","kind":"tool_approval","data":{
  "tool":"execute_command","args":["cargo test"],"id":"req-123"
}}\x07

# Memory operation
\x1b]2337;{"type":"Memory","operation":"search","query":"database indexes"}\x07

# Session checkpoint (for resume after crash)
\x1b]2337;{"type":"Session","action":"checkpoint","data":{...}}\x07

# Secure access request
\x1b]2337;{"type":"SecureAccess","variable":"API_KEY","action":"request"}\x07

# File tree sync (initial snapshot)
\x1b]2337;{"type":"Tree","mode":"snapshot","data":[...]}\x07

# File tree delta (incremental update)
\x1b]2337;{"type":"Tree","mode":"update","data":{"path":"src/main.rs","event":"modified"}}\x07

# Version negotiation handshake
\x1b]2337;{"type":"Hello","version":"0.4.1","capabilities":[...]}\x07
```

Terminals that don't understand OSC 2337 ignore it silently. Colony detects support via the Hello/HelloAck handshake and falls back to stderr prompts and plain text rendering.

---

## 14. Path Management

First-class `path` builtin that emits tuples on change so every subscribed component stays in sync.

### Scenario: setting up a polyglot development environment

```bash
# System defaults
path list
# /usr/local/bin
# /usr/bin
# /bin

# Add language-specific tooling
path prepend /opt/homebrew/bin          # Homebrew first for macOS
path add ~/.cargo/bin                    # Rust toolchain
path add ~/.local/share/mise/shims      # mise (polyglot version manager)
path add ~/go/bin                        # Go binaries
path add ~/.bun/bin                      # Bun runtime

# Project-specific (in .colony/init)
path add ./node_modules/.bin             # npm scripts
path add ./vendor/bin                    # PHP Composer
path add ./venv/bin                      # Python virtualenv

# Check before adding
path has ~/.cargo/bin && echo "Rust ready" || path add ~/.cargo/bin

# Clean up stale entries
path remove /usr/local/games

# Result: env-panel updates instantly via tuple space
path list
# /opt/homebrew/bin          ← prepended (first)
# /usr/local/bin
# /usr/bin
# /bin
# /Users/me/.cargo/bin       ← appended
# /Users/me/.local/share/mise/shims
# /Users/me/go/bin
# /Users/me/.bun/bin
# ./node_modules/.bin
# ./vendor/bin
# ./venv/bin
```

Every `path add` / `path remove` emits a tuple, so the env-panel and any subscribed panels update immediately.

---

## 15. Full Shell Features

Colony is a real POSIX-compatible shell — not a wrapper around bash. It has its own parser, expander, and executor written from scratch in Rust.

### POSIX expansion pipeline (7 stages, in order)

```bash
# 1. Tilde expansion
echo ~                    # → /Users/me
echo ~root                # → /root
cd ~/projects              # → /Users/me/projects

# 2. Parameter expansion (all ${} forms)
name="world"
echo "Hello, ${name}"                    # → Hello, world
echo "${name:-default}"                  # → world (use default if unset)
echo "${undefined:-fallback}"            # → fallback
echo "${name:0:3}"                       # → wor (substring)
echo "${name^^}"                         # → WORLD (uppercase)
echo "${name,,}"                         # → world (lowercase)
echo "${#name}"                          # → 5 (length)
filename="archive.tar.gz"
echo "${filename%.gz}"                   # → archive.tar (remove suffix)
echo "${filename%%.*}"                   # → archive (remove longest suffix)
echo "${filename#*.}"                    # → tar.gz (remove prefix)
echo "${filename##*.}"                   # → gz (remove longest prefix)
echo "${filename/tar/zip}"              # → archive.zip.gz (replace first)
echo "${filename//a/A}"                 # → Archive.tAr.gz (replace all)

# 3. Command substitution (nestable)
echo "Today is $(date +%Y-%m-%d)"
echo "Lines: $(wc -l < $(find . -name '*.rs' | head -1))"
branch=$(git branch --show-current)
count=$(find . -name "*.rs" -exec grep -l "TODO" {} \; | wc -l)

# 4. Arithmetic expansion
echo $((2 + 3 * 4))       # → 14
echo $((RANDOM % 100))     # → random 0-99
i=0; echo $((i += 1))      # → 1

# 5. Field splitting (IFS-based)
IFS=:; for dir in $PATH; do echo "$dir"; done

# 6. Pathname expansion (glob)
echo *.rs                   # → main.rs lib.rs
echo src/**/*.rs            # → recursive glob
echo test_?.log             # → test_1.log test_2.log

# 7. Brace expansion
echo {a,b,c}              # → a b c
echo file{1,2,3}.txt       # → file1.txt file2.txt file3.txt
echo {a,b}{1,2}            # → a1 a2 b1 b2
mkdir -p src/{models,handlers,middleware}
```

### Job control

```bash
# Start background jobs
cargo build --release &    # Background immediately
[1] 12345

sleep 300 &
[2] 12346

# Suspend foreground (Ctrl+Z) then background
vim src/main.rs
# ^Z
# [3]+ Stopped     vim src/main.rs
bg %3                       # Continue vim in background (unusual, but possible)

# List all jobs
jobs
# [1]  Running     cargo build --release &
# [2]  Running     sleep 300 &
# [3]  Running     vim src/main.rs &

# Bring a job to foreground
fg %1                       # Wait for cargo build to finish

# Detach a job (survives shell exit)
disown %2

# Wait for specific jobs
wait %1 %2                  # Block until both complete
```

### Redirections and pipes

```bash
# Standard redirections
cmd > output.txt             # Overwrite stdout to file
cmd >> output.txt            # Append stdout to file
cmd 2> errors.txt            # Redirect stderr
cmd > out.txt 2>&1           # Merge stderr into stdout
cmd &> both.txt              # Shorthand for above
cmd < input.txt              # stdin from file
cmd >| locked.txt            # Override noclobber

# Pipes
cat data.csv | sort -t, -k2 | uniq -c | sort -rn | head -20

# Here documents
cat <<EOF
Hello $USER,
Today is $(date).
EOF

# Process substitution (if supported)
diff <(sort file1.txt) <(sort file2.txt)
```

### Conditionals and loops

```bash
# if/elif/else
if [ -f Cargo.toml ]; then
    echo "Rust project"
    cargo build
elif [ -f package.json ]; then
    echo "Node project"
    npm install
else
    echo "Unknown project type"
fi

# Test expressions
[ -d src ] && echo "src/ exists"
[ -z "$VAR" ] && echo "VAR is empty"
[ "$ENV" = "production" ] && echo "PROD!"

# For loops
for file in src/*.rs; do
    echo "$(wc -l < "$file") lines in $file"
done

for i in 1 2 3 4 5; do
    echo "Attempt $i..."
    curl -s http://localhost:3000/health && break
    sleep 2
done

# While loops
while read -r line; do
    echo "Processing: $line"
done < todo.txt

# Case statements
case "$1" in
    start)  systemctl start myapp ;;
    stop)   systemctl stop myapp ;;
    *)      echo "Usage: $0 {start|stop}" ;;
esac
```

### Functions and aliases

```bash
# Functions
deploy() {
    local env="${1:-staging}"
    echo "Deploying to $env..."
    cargo build --release
    scp target/release/app "$env-server:/opt/app"
    ssh "$env-server" "systemctl restart app"
    echo "Deployed to $env"
}

# Call it
deploy production

# Aliases
alias ll="ls -la"
alias gs="git status"
alias gd="git diff"
alias gc="git commit"
alias gp="git push"
alias k="kubectl"
alias tf="terraform"

# Shell options
set -e              # Exit on error
set -u              # Error on undefined variables
set -o pipefail     # Pipe failure propagation
set -x              # Xtrace — show commands before execution
                    # Customize with PS4: export PS4='+ ${BASH_SOURCE}:${LINENO}: '
```

---

## 16. Project Configuration (.colonyrc)

Per-project configuration with a trust model inspired by direnv — colony won't load a `.colonyrc` until you explicitly trust it.

### Scenario: team configuration for a microservices project

```toml
# .colonyrc — checked into git, shared by the team

[agent]
personality = """
You are a backend developer working on the Acme payment service.
This is a Rust microservice using Axum, SQLx, and RabbitMQ.
Production runs on Kubernetes in AWS us-east-1.
Never suggest changes to the migrations/ directory without explicit approval.
"""
preferred_provider = "anthropic"
preferred_model = "claude-3-5-sonnet"
temperature = 0.3
max_tokens = 8192

[project]
name = "payment-service"
description = "Handles payment processing, webhooks, and billing"
language = "rust"
goals = [
    "100% test coverage on handlers",
    "Sub-50ms p99 latency",
    "Zero unhandled panics in production"
]
conventions = [
    "Use thiserror for all error types",
    "All handlers must return Result<Json<T>, AppError>",
    "Database queries go through the repository pattern",
    "Use tracing for structured logging, never println",
]
docs = [
    "https://docs.stripe.com/webhooks",
    "https://docs.rs/axum/latest/axum/",
]

# MCP servers for agent tooling
[[mcp.servers]]
name = "postgres"
command = "mcp-postgres"
args = ["--connection-string", "${DATABASE_URL}"]
auto_start = true

[[mcp.servers]]
name = "github"
command = "mcp-github"
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }

# Tool classifications
[tools]
classifications = {
    "file_read" = "programmatic",      # Agent can read files without asking
    "cargo_test" = "programmatic",      # Agent can run tests without asking
    "file_write" = "user_facing",       # Agent must ask before writing
    "execute_command" = "user_facing",  # Agent must ask before running commands
}

# Completion settings
[completion]
suggest_commands = true
suggest_variables = true
suggest_paths = true
```

First time colony sees this file:
```
⚠ New .colonyrc detected in /Users/me/projects/payment-service
  Trust this configuration? [y/n]
```

After trusting, it loads automatically every time you enter the directory.

---

## 17. Complete Setup Example

### Global init: `~/.colony/init`

```bash
# ── Secure credentials (encrypted, keychain-backed) ──
secure load

# ── Dev workspace (Ctrl-B) ──
# Left: tabbed file tree + details
# Right: file preview
# Bottom: scrolling command output log
sidebar left --hotkey ctrl-b filetree filedetails
sidebar right --hotkey ctrl-b preview
sidebar bottom --outer --hotkey ctrl-b logs-panel

# ── Git workspace (Ctrl-G) ──
# Left: interactive staging with inline diffs
# Right: stacked status + history
sidebar left --hotkey ctrl-g gitstage
sidebar right --hotkey ctrl-g --stack gitstatus history-panel

# ── System monitoring (Ctrl-M) ──
# Left: stacked jobs + env
# Right: session info
# Bottom: full-width log viewer
sidebar left --hotkey ctrl-m --stack jobs-panel env-panel
sidebar right --hotkey ctrl-m session-panel
sidebar bottom --outer --hotkey ctrl-m logs-panel

# ── PATH setup ──
path prepend /opt/homebrew/bin
path add ~/.cargo/bin
path add ~/go/bin
path add ~/.bun/bin
path add ~/.local/bin

# ── Aliases ──
alias ll="ls -la"
alias la="ls -A"
alias gs="git status"
alias gd="git diff"
alias gc="git commit"
alias gp="git push"
alias gl="git log --oneline -20"
alias k="kubectl"
alias d="docker"
alias dc="docker compose"
```

### Per-project init: `my-project/.colony/init`

```bash
# Project-specific panels override global
sidebar left --hotkey ctrl-b filetree
sidebar right --hotkey ctrl-b preview
sidebar bottom --outer --hotkey ctrl-t logs-panel

# Project PATH
path add ./node_modules/.bin
path add ./scripts

# Project aliases
alias dev="npm run dev"
alias test="npm test"
alias lint="npm run lint"
alias build="npm run build"
alias db="npx prisma studio"
alias migrate="npx prisma migrate dev"
```

### Session flow

```bash
colony                                    # Launch the shell
# → Runs ~/.colony/init
# → Loads secure env vars
# → Panels configured but hidden (hotkey groups)
# → PATH set up

cd ~/projects/my-api                      # Enter a project
# → Runs .colony/init (project overrides)
# → Loads AGENT.md → agent becomes API specialist
# → Loads CONTEXT.md → agent knows the codebase

Ctrl-B                                    # Dev view appears
# Left: filetree + filedetails (tabs)
# Right: preview
# Navigate with j/k, Enter to open, ] to switch tabs

Ctrl-B                                    # Dismiss dev view
Ctrl-G                                    # Git view appears
# Left: gitstage with inline diffs
# Right: gitstatus + history (stacked)
# Press space to stage, see status update in real time

# Stage some files, write commit message, Esc back to prompt
git commit -m "feat: add payment webhook handler"

Ctrl-G                                    # Dismiss git view

# Ask the agent for help
# (agent already knows: Rust + Axum, your schema, your conventions)
# → Uses recipe-defined tools, respects approval policies
# → Conversation persisted to SQLite for cross-session context

Ctrl-M                                    # System monitoring
# See running jobs, env vars, session stats
```

---

## Architecture

```
20 crates:
  colony-shell      — Main binary, TUI REPL, orchestrator
  colony-exec       — Command execution, 56 builtins, sidebar parser
  colony-parser     — POSIX shell parser (recursive descent AST)
  colony-expand     — 7-stage expansion pipeline (tilde → quote removal)
  colony-panel      — Panel host, layout engine, protocol, slot management
  colony-tuple      — Linda-style tuple space (in-memory + SQLite persistence)
  colony-secure     — AES-GCM encrypted env vars, OS keychain integration
  colony-agent      — Multi-provider LLM (Anthropic, OpenAI, Ollama, Cloud)
  colony-memory     — Semantic memory, LanceDB vectors, importance decay
  colony-channels   — Extended FD channels (fd 3-6 for context/llm/tool/diag)
  colony-recipe     — Declarative LLM mode configuration (TOML)
  colony-config     — .colonyrc project configuration with trust model
  colony-focus      — Goal trees, checkpoints, session continuity
  colony-reflection — Observation database, session context tracking
  colony-protocol   — OSC 2337 shell↔terminal communication protocol
  colony-mcp        — Model Context Protocol connection pool + codegen
  colony-cloud-sdk  — Colony Cloud API client (managed LLM, recipe registry)
  colony-filetree   — File tree state, git status overlay, filesystem watcher
  colony-tui        — TUI framework utilities
  colony-ui-core    — Shared UI primitives

10 panel binaries (child processes, any can crash without affecting the shell):
  filetree, filedetails, preview, gitstatus, gitstage,
  jobs-panel, history-panel, env-panel, logs-panel, session-panel
```
