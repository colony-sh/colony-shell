# Colony Shell — Advanced Workflows

Colony is a modern shell with a TUI panel system, secure environment variables, and reactive inter-component communication. This guide shows off what it can do.

---

## 1. Panel Sidebar System

Colony's killer feature: dock live-updating panels alongside your shell.

### Basic panel setup

```bash
# Add a file tree on the left
sidebar left filetree

# Add git status on the right
sidebar right gitstatus

# Add a log viewer at the bottom (full-width)
sidebar bottom --outer logs-panel
```

### Hotkey-toggled panel groups

Group panels under a single hotkey. All panels in a group show/hide together:

```bash
# Dev view: file tree + git status, toggled with Ctrl-B
sidebar left --hotkey ctrl-b filetree
sidebar right --hotkey ctrl-b gitstatus

# Git view: staging + status, toggled with Ctrl-G
sidebar left --hotkey ctrl-g gitstage
sidebar right --hotkey ctrl-g gitstatus
```

Press `Ctrl-B` to toggle the dev view. Press `Ctrl-G` to switch to the git view.

### Tabbed panels

Multiple panels at the same position default to tabs:

```bash
# Two left panels become tabs — filetree is shown first, press ] to switch
sidebar left --hotkey ctrl-b filetree
sidebar left --hotkey ctrl-b filedetails

# Or use the multi-command shorthand:
sidebar left --hotkey ctrl-b filetree filedetails
```

Press `Tab` to focus the panel area, then `[`/`]` to cycle between tabs.

### Stacked panels

Show all panels at once, split vertically:

```bash
sidebar left --hotkey ctrl-b --stack filetree filedetails
```

### Available panels

| Panel | Description |
|-------|-------------|
| `filetree` | Interactive file browser with icons |
| `filedetails` | File metadata and preview |
| `preview` | File content preview |
| `gitstatus` | Git branch, changes, and log |
| `gitstage` | Interactive git staging (j/k to navigate, space to stage) |
| `jobs-panel` | Background job status |
| `history-panel` | Command history viewer |
| `env-panel` | Environment variable browser |
| `logs-panel` | System/application log viewer |
| `session-panel` | Session info and stats |

### Panel navigation

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus: main → panel groups → main |
| `Esc` | Return focus to main prompt |
| `[` / `]` | Cycle tabs within focused panel group |
| `?` | Show context-sensitive help |
| `j` / `k` | Navigate within panel (most panels) |

---

## 2. Secure Environment Variables

Encrypt sensitive values at rest. They're only decrypted when a command actually needs them.

```bash
# Store an API key securely
secure set API_KEY sk-live-abc123def456

# It's encrypted in ~/.colony/secure/
# But available like any env var:
echo $API_KEY
# → sk-live-abc123def456

# List secure variables
secure list

# Remove a secure variable
secure unset API_KEY
```

Secure vars are encrypted with a per-machine key derived from your system keychain. They survive shell restarts but never appear in plain text on disk.

---

## 3. Init Scripts

Customize your shell startup with layered init scripts:

```
~/.colony/init          # Global (runs first)
.colony/init            # Per-project (runs second)
```

Init scripts run shell commands on startup — perfect for setting up your panel layout:

```bash
# ~/.colony/init
sidebar left --hotkey ctrl-b filetree filedetails
sidebar right --hotkey ctrl-b preview
sidebar bottom --outer --hotkey ctrl-l logs-panel
secure load  # load secure env vars
```

---

## 4. Flux Process Manager

Interactive process management with TUI mode, port tracking, and resource monitoring:

```bash
# Interactive TUI — browse and kill processes
flux --tui

# Filter by name
flux --filter node

# Show what's using port 3000
flux --port 3000

# Kill processes matching a pattern
flux --filter "zombie" --nuke

# Sort by memory usage
flux --sort mem --resources
```

---

## 5. Path Management

First-class `path` builtin for managing `$PATH`:

```bash
# Add to end of PATH
path add ~/.local/bin

# Add to beginning of PATH
path prepend /opt/homebrew/bin

# Check if a directory is in PATH
path has ~/.cargo/bin && echo "cargo available"

# List PATH entries
path list

# Remove from PATH
path remove /usr/local/games
```

---

## 6. Tuple Space (Inter-Component Communication)

Components communicate through a Linda-style tuple space. Panels subscribe to events and react in real time:

```bash
# When you `cd`, the file tree auto-updates because it subscribes to cwd changes
cd ~/projects/myapp
# → filetree panel instantly shows ~/projects/myapp contents

# Git panels auto-update after git operations
git add .
# → gitstatus and gitstage panels refresh automatically
```

Panels declare subscriptions in their `---CONFIG---` block:
```json
{"subscriptions": ["cwd", "git"], "help": ["j/k: navigate", "space: stage"]}
```

---

## 7. Goal Tracking

Track tasks and objectives hierarchically, right from the shell:

```bash
# Add goals
goal add "Ship v1.0"
goal add "Write tests" --parent 1

# Track progress
goal list
#  1. [pending] Ship v1.0
#    2. [pending] Write tests

# Update status
goal complete 2
goal block 1 --reason "Waiting on API approval"

# See the tree
goal tree
```

Goals support statuses (`pending`, `in_progress`, `completed`, `blocked`, `cancelled`), hierarchical nesting, and auto-calculated progress from children.

---

## 8. Extended Channels

Beyond stdin/stdout/stderr, colony provides dedicated file descriptors for structured communication:

| FD | Name | Purpose |
|----|------|---------|
| 3 | `stdcontext` | Session context and observations |
| 4 | `stdllm` | LLM conversation history |
| 5 | `stdtool` | Tool call logging |
| 6 | `stddiag` | Diagnostics |

```bash
# Write structured context
echo '{"event":"deploy","status":"success"}' >&3

# Read diagnostics from a command
mycommand 6>/tmp/diag.log
```

---

## 9. Agent Framework

Multi-provider LLM integration with tool use, conversation persistence, and token budgets:

```bash
# Agent auto-detects providers from API keys
export ANTHROPIC_API_KEY=sk-ant-...   # Claude (primary)
export OPENAI_API_KEY=sk-...          # GPT (fallback)
# Or use Ollama locally for free

# Manage agent context
context add "We're building a REST API in Rust"
context show
context clear
```

Features: streaming responses, tool approval workflow, conversation persistence (SQLite), automatic provider fallback with health tracking.

---

## 10. Mirror & Observation Database

Tail and analyze your shell session history with LLM assistance:

```bash
# Review recent session observations
mirror

# Observations are logged automatically:
# - Commands executed
# - Errors encountered
# - File changes
# - Directory navigation
```

Observations are stored in `~/.colony/reflection/observations.db` (SQLite) for cross-session analysis.

---

## 11. Full Shell Features

Colony is a real shell — not just a TUI wrapper. It supports:

### Pipelines and redirects
```bash
cat file.txt | grep "pattern" | sort | uniq > output.txt
ls -la 2>/dev/null
```

### Command substitution
```bash
echo "Current branch: $(git branch --show-current)"
files=$(find . -name "*.rs" | wc -l)
```

### Job control
```bash
sleep 100 &       # Background job
jobs               # List jobs
fg %1              # Bring to foreground
bg %1              # Continue in background
```

### Directory stack
```bash
pushd /tmp         # Push current dir, cd to /tmp
pushd ~/projects   # Push /tmp, cd to ~/projects
dirs               # Show stack
popd               # Pop back
```

### Conditionals and loops
```bash
if [ -f Cargo.toml ]; then
    echo "Rust project detected"
    cargo build
fi

for f in *.rs; do
    echo "Processing $f"
done
```

### Aliases and functions
```bash
alias ll="ls -la"
alias gs="git status"

greet() {
    echo "Hello, $1!"
}
```

---

## 12. Example: Full Development Setup

Put this in `~/.colony/init` for a complete dev environment:

```bash
# Secure credentials
secure load

# Dev panel layout (Ctrl-B to toggle)
sidebar left --hotkey ctrl-b filetree filedetails
sidebar right --hotkey ctrl-b preview

# Git panels (Ctrl-G to toggle)
sidebar left --hotkey ctrl-g gitstage
sidebar right --hotkey ctrl-g gitstatus

# Monitoring (Ctrl-L to toggle)
sidebar bottom --outer --hotkey ctrl-l logs-panel
sidebar top --outer --hotkey ctrl-j jobs-panel

# Shortcuts
alias gs="git status"
alias gc="git commit"
alias gp="git push"
alias ll="ls -la"

# Project-specific PATH
path add ./node_modules/.bin
path add ./target/debug
```

Then just launch `colony` and you have a full IDE-like environment in your terminal.
