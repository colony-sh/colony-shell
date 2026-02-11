# Colony Channels, Buffers, and Special Files

**Version:** 0.1.0-draft  
**Addendum to:** Colony Shell Language Specification  
**Date:** December 2025

---

## Overview

Colony extends the traditional Unix file descriptor model with semantic channels that capture different types of information flowing through a session. These channels can be viewed in separate buffers/panes, creating a multi-dimensional terminal experience.

Additionally, Colony recognizes special files in the filesystem that configure agent behavior, seed context, and define project-specific tooling.

---

## 1. Extended Channel Architecture

### 1.1 Channel Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            Colony Process                                │
│                                                                         │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌───────────┐ ┌─────────┐         │
│  │ stdin   │ │ stdout  │ │ stderr  │ │ stdcontext│ │ stdllm  │         │
│  │ fd 0    │ │ fd 1    │ │ fd 2    │ │ fd 3      │ │ fd 4    │         │
│  └────┬────┘ └────┬────┘ └────┬────┘ └─────┬─────┘ └────┬────┘         │
│       │           │           │            │            │               │
└───────┼───────────┼───────────┼────────────┼────────────┼───────────────┘
        │           │           │            │            │
        ▼           ▼           ▼            ▼            ▼
    ┌───────┐   ┌───────┐   ┌───────┐   ┌─────────┐  ┌─────────┐
    │ Input │   │ Main  │   │ Error │   │ Context │  │  LLM    │
    │       │   │Buffer │   │Buffer │   │ Buffer  │  │ Buffer  │
    └───────┘   └───────┘   └───────┘   └─────────┘  └─────────┘
                    │           │            │            │
                    └───────────┴────────────┴────────────┘
                                     │
                                     ▼
                            ┌─────────────────┐
                            │  Pane Manager   │
                            │  (UI Renderer)  │
                            └─────────────────┘
```

### 1.2 Standard Channels

| Channel | FD | Alias | Purpose |
|---------|----|----|---------|
| stdin | 0 | `&0` | Standard input |
| stdout | 1 | `&1` | Standard output |
| stderr | 2 | `&2` | Error output |
| stdcontext | 3 | `&ctx` | Session context accumulation |
| stdllm | 4 | `&llm` | LLM conversation stream |
| stdtool | 5 | `&tool` | Tool call/response logging |
| stddiag | 6 | `&diag` | Diagnostics |

### 1.3 Channel Redirection Syntax

```colony
# Traditional redirects still work
echo "hello" > file.txt
echo "error" >&2

# New channel redirects
echo "context note" >&ctx          # Write to stdcontext
echo "user message" >&llm          # Write to stdllm (as user)

# Named redirects (clearer)
echo "note" >&context
echo "message" >&llm

# Redirect command output to specific channel
expensive-command >&1 2>&2 3>&ctx  # Explicit all channels

# Capture channel to variable
var ctx_content = $(&ctx)          # Read from context buffer
var llm_history = $(&llm)          # Read from llm buffer
```

---

## 2. The LLM Channel (stdllm)

### 2.1 Concept

stdllm is a structured conversation buffer that maintains the chat history between you and any LLM interactions in your session. Unlike stdout which is ephemeral, stdllm accumulates and can be viewed, edited, branched, and exported.

### 2.2 Writing to stdllm

```colony
# Implicit: Any agent invocation writes to stdllm
invoke code-reviewer(files=$files)
# Conversation automatically captured in stdllm

# Explicit user message
llm:user "Can you explain this error?"

# Or redirect
echo "Can you explain this error?" >&llm:user

# System messages
llm:system "You are a helpful coding assistant"

# Assistant messages (for injecting/mocking)
llm:assistant "I'd be happy to help with that error."

# Structured message with metadata
llm:message {
    role: "user"
    content: "Explain this code"
    attachments: [{type: "file", path: "main.rs"}]
    metadata: {
        timestamp: $(date -Iseconds)
        context_snapshot: $(context:export --format json)
    }
}
```

### 2.3 Reading from stdllm

```colony
# View conversation
llm:show                           # Pretty print conversation
llm:show --last 5                  # Last 5 messages
llm:show --role user               # Only user messages

# Get as data
var history = llm:get
echo $[len(history.messages)] messages

# Export
llm:export > conversation.json
llm:export --format markdown > chat.md
llm:export --format sharegpt > training-data.jsonl

# Search
llm:search "error handling"
```

### 2.4 Conversation Management

```colony
# Branch conversation (for exploration)
llm:branch "experiment-1"
# Now in a branch, can explore different paths
invoke assistant("Try a different approach")

# Switch branches
llm:checkout "main"
llm:checkout "experiment-1"

# Merge insights back (manually curated)
llm:cherry-pick "experiment-1" 3   # Pick message #3 from branch

# List branches
llm:branches

# Prune/compact conversation
llm:summarize                      # Ask LLM to summarize, replace history
llm:truncate --keep-last 10        # Keep only last 10 messages
llm:clear                          # Clear entirely

# Checkpoints
llm:checkpoint "before-refactor"
llm:restore "before-refactor"
```

### 2.5 Multi-Agent Conversations

When multiple agents are involved, stdllm tracks the full conversation with attribution:

```colony
# Sequential agents
invoke researcher(topic="quantum computing")    # Researcher's conversation
invoke writer(input=$research_output)           # Writer's conversation

# llm:show output
# [researcher] user: Research quantum computing
# [researcher] assistant: Quantum computing uses...
# [writer] user: Write an article based on: ...
# [writer] assistant: # Introduction to Quantum Computing...

# Filter by agent
llm:show --agent researcher
llm:show --agent writer

# Parallel agents (interleaved)
parallel {
    invoke analyst(data=$data, focus="trends")
    invoke analyst(data=$data, focus="anomalies")
}
# Both conversations in stdllm, tagged with agent instance
```

### 2.6 Interactive Chat Mode

```colony
# Enter interactive chat mode
llm:chat
# Opens a chat interface in the llm buffer
# Type messages, see responses
# Ctrl+D to exit back to shell

# Chat with specific agent
llm:chat --agent code-reviewer

# Chat with context injection
llm:chat --context "We're debugging a memory leak"

# Chat with tool access
llm:chat --tools [read_file, run_tests]
```

---

## 3. Buffer/Pane Management

### 3.1 Buffer Concepts

Each channel has an associated buffer that can be displayed in panes:

```colony
# List buffers
buffer:list
# -> main      stdout/stderr   visible   1,247 lines
# -> context   stdcontext      hidden    47 entries  
# -> llm       stdllm          hidden    12 messages
# -> tool      stdtool         hidden    8 calls
# -> diagnostic stddiag        hidden    disabled
```

### 3.2 Pane Layouts

```colony
# Show/hide panes
pane:show context              # Show context pane
pane:show llm                  # Show llm pane
pane:hide context              # Hide context pane

# Pane positions
pane:position context right    # right, left, top, bottom, float
pane:position llm bottom

# Pane sizing
pane:size context 30%          # Percentage of screen
pane:size llm 20 lines         # Fixed lines

# Quick toggles
pane:toggle context            # Toggle visibility
bind Ctrl+Shift+C pane:toggle context
bind Ctrl+Shift+L pane:toggle llm
bind Ctrl+Shift+T pane:toggle tool

# Preset layouts
pane:layout default            # Just main buffer
pane:layout dev                # main + context + llm
pane:layout debug              # main + context + tool + diagnostic
pane:layout focus              # main maximized, others in tabs
```

### 3.3 Visual Layout Examples

#### Default Layout
```
┌─────────────────────────────────────────────────────────────────────────┐
│ $ cat data.json -> jq '.users[]' -> select .name                        │
│ Alice                                                                   │
│ Bob                                                                     │
│ Charlie                                                                 │
│ $                                                                       │
└─────────────────────────────────────────────────────────────────────────┘
```

#### Dev Layout (main + context + llm)
```
┌───────────────────────────────────────────┬─────────────────────────────┐
│ $ invoke code-reviewer(files=@files)      │ ┌─ Context ───────────────┐ │
│                                           │ │ • Started code review   │ │
│ Found 3 issues:                           │ │ • Focus: security       │ │
│ 1. SQL injection risk at line 42          │ │ • Files: 3 analyzed     │ │
│ 2. Missing null check at line 87          │ │ • Issues found: 3       │ │
│ 3. Deprecated API usage at line 103       │ │                         │ │
│                                           │ └─────────────────────────┘ │
│ $                                         │ ┌─ LLM ────────────────────┐ │
│                                           │ │ user: Review these files│ │
│                                           │ │ for security issues     │ │
│                                           │ │                         │ │
│                                           │ │ assistant: I'll analyze │ │
│                                           │ │ the code for security   │ │
│                                           │ │ vulnerabilities...      │ │
│                                           │ │                         │ │
│                                           │ │ [12 messages total]     │ │
│                                           │ └─────────────────────────┘ │
└───────────────────────────────────────────┴─────────────────────────────┘
```

#### Debug Layout (main + context + tool + diagnostic)
```
┌─────────────────────────────────────────────────────────────────────────┐
│ $ invoke debugger(error=$error_log)                                     │
│ Analyzing error...                                                      │
├─────────────────────────────────────────────────────────────────────────┤
│ ┌─ Context ─────────────────┐ ┌─ Tool Calls ──────────────────────────┐ │
│ │ • Debugging NullPtrExcept │ │ → read_file("src/main.rs")            │ │
│ │ • Stack trace analyzed    │ │   ✓ 2,451 bytes                       │ │
│ │ • Root cause: line 87     │ │ → search_code("unwrap()")             │ │
│ │                           │ │   ✓ 12 matches                        │ │
│ └───────────────────────────┘ │ → run_tests("test_user_auth")         │ │
│ ┌─ Trace ───────────────────┐ │   ✗ FAILED: assertion error           │ │
│ │ 10:32:01 proc:debugger    │ └───────────────────────────────────────┘ │
│ │ 10:32:01 tool:read_file   │                                           │
│ │ 10:32:02 tool:search_code │                                           │
│ │ 10:32:03 tool:run_tests   │                                           │
│ └───────────────────────────┘                                           │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.4 Buffer Navigation

```colony
# Focus a buffer (for scrolling, selection)
buffer:focus llm
buffer:focus main

# Scroll within buffer
buffer:scroll llm top
buffer:scroll llm bottom
buffer:scroll llm +10             # Down 10 lines
buffer:scroll llm -10             # Up 10 lines

# Search within buffer
buffer:search llm "error"
buffer:search context "decision"

# Copy from buffer
buffer:copy llm                   # Copy all
buffer:copy llm --selection       # Copy selected region
buffer:copy context --last 5      # Copy last 5 entries

# Keybindings for buffer navigation
bind Alt+1 buffer:focus main
bind Alt+2 buffer:focus context
bind Alt+3 buffer:focus llm
bind Alt+4 buffer:focus tool
```

### 3.5 Buffer Persistence

```colony
# Auto-save buffers
config set buffer.auto-save true
config set buffer.save-path "~/.local/share/colony/buffers"

# Manual save
buffer:save llm "conversation-$(date +%Y%m%d).json"
buffer:save context "session-context.json"

# Restore on session start
config set buffer.restore-on-start true

# Load buffer from file
buffer:load llm "previous-conversation.json"
```

---

## 4. Special Files

Colony recognizes special files in the filesystem that affect shell behavior when you're in that directory (or below it).

### 4.1 File Overview

| File | Purpose |
|------|---------|
| `AGENT.md` | Define agents available in this project |
| `CONTEXT.md` | Seed context for sessions |
| `TOOLS.md` | Define/configure tools for this project |
| `PROMPT.md` | Default prompt templates |
| `.colony/config.toml` | Project-specific Colony settings |
| `.colony/session.toml` | Session persistence |

### 4.2 AGENT.md

Defines agents that are automatically available when working in this directory:

```markdown
# AGENT.md

## Project Agents

Agents defined here are available when working in this project.

---

## code-reviewer

Reviews code for quality and security issues specific to this project.

### Capabilities

- tools: [read_file, search_code, run_tests, lint]
- mcp_servers: [filesystem, github]
- max_tokens: 8192
- model: claude-sonnet-4-20250514

### Input

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| files | List[Str] | required | Files to review |
| focus | Str | "all" | Focus area: security, style, performance, all |
| strict | Bool | false | Fail on any issues |

### Output

Returns `ReviewResult` with fields:
- issues: List of found issues
- summary: Overall assessment
- score: Quality score 0-100

### System Prompt

You are a code reviewer for the Colony shell project. This is a Rust project
using GPUI for the UI layer. 

Key things to check:
- Memory safety (avoid unnecessary clones, check lifetimes)
- Error handling (use Result, avoid unwrap in production code)
- GPUI patterns (proper view/model separation)
- Documentation (public APIs must have doc comments)

Project conventions:
- We use `thiserror` for error types
- Async code uses `smol` runtime
- Tests go in `tests/` not inline

---

## doc-writer

Generates documentation for this project.

### Capabilities

- tools: [read_file, write_file, search_code]
- max_tokens: 4096

### Input

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| target | Str | required | What to document (file, module, function) |
| style | Str | "rustdoc" | Documentation style |

### System Prompt

You write documentation for the Colony project. Follow rustdoc conventions.
Always include:
- Brief description
- Examples
- Panics section if applicable
- Safety section for unsafe code

---

## test-generator

Generates tests for code in this project.

### Capabilities

- tools: [read_file, write_file, run_tests]
- max_tokens: 4096

### Input

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| target | Str | required | Code to generate tests for |
| coverage | Str | "normal" | Coverage level: minimal, normal, exhaustive |

### System Prompt

Generate tests for Colony shell code. Use the following patterns:
- Unit tests with `#[test]`
- Integration tests in `tests/`
- Use `pretty_assertions` for better diffs
- Mock external services with `mockall`
```

**Behavior:**

```colony
# When you cd into a directory with AGENT.md
cd ~/projects/colony

# Agents are now available
invoke code-reviewer(files=["src/main.rs"])
invoke doc-writer(target="src/lib.rs")

# List project agents
agent:list
# -> code-reviewer  (project: ~/projects/colony)
# -> doc-writer     (project: ~/projects/colony)
# -> test-generator (project: ~/projects/colony)

# They're loaded with project context automatically
```

### 4.3 CONTEXT.md

Seeds the context buffer when entering this directory:

```markdown
# CONTEXT.md

## Project Context

This context is automatically loaded when working in this directory.

---

## Project Overview

Colony is an agent-first shell written in Rust using the GPUI framework.
It provides POSIX compatibility with modern extensions for AI workflows.

## Architecture

- `crates/colony-core/` - Core shell implementation
- `crates/colony-parser/` - Shell language parser
- `crates/colony-runtime/` - Execution runtime
- `crates/colony-ui/` - GPUI-based terminal UI
- `crates/colony-agents/` - Agent framework

## Current Focus

We're currently working on:
1. Live evaluation system for command preview
2. stdcontext and stdllm channel implementation
3. Buffer/pane management UI

## Key Decisions

- Using GPUI instead of crossterm for richer UI capabilities
- Tuple space for inter-session communication (Linda model)
- Dual-binary architecture inspired by Oils (colony-sh / colony)

## Conventions

- Commit messages follow conventional commits
- PRs require at least one approval
- All public APIs need documentation
- Integration tests for new features

## Resources

- Design docs: docs/design/
- Meeting notes: docs/meetings/
- Architecture diagrams: docs/architecture/
```

**Behavior:**

```colony
# When entering directory
cd ~/projects/colony

# Context is loaded
context:show
# -> [seed] Project Overview: Colony is an agent-first shell...
# -> [seed] Architecture: crates/colony-core/...
# -> [seed] Current Focus: Live evaluation system...

# Agents see this context automatically
invoke code-reviewer(files=$files)
# Agent knows: "This is a Rust project using GPUI"

# You can add to seeded context
context "Fixed the buffer overflow in parser"
# This adds to the session, doesn't modify CONTEXT.md

# Update seed context
context:seed:edit                  # Opens CONTEXT.md in $EDITOR
```

### 4.4 TOOLS.md

Defines or configures tools available in this project:

```markdown
# TOOLS.md

## Project Tools

Tools defined here are available to agents working in this project.

---

## build

Build the project.

### Command

```shell
cargo build --release
```

### Output Type

text

---

## test

Run project tests.

### Command

```shell
cargo test ${args}
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| args | Str | "" | Additional arguments to cargo test |
| filter | Str | "" | Test name filter |

### Output Type

text

---

## lint

Run linters on the project.

### Command

```shell
cargo clippy -- -D warnings && cargo fmt --check
```

### Output Type

json

### Output Schema

```json
{
  "clippy_warnings": "number",
  "format_issues": "number", 
  "passed": "boolean"
}
```

---

## search_code

Search code with ripgrep.

### Command

```shell
rg --json ${pattern} ${path:-./}
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| pattern | Str | required | Search pattern |
| path | Str | "./" | Path to search |
| file_type | Str | "" | Filter by file type |

### Output Type

json

---

## run_tests

Run specific tests.

### Command

```shell
cargo test ${test_name} -- --nocapture
```

### Safety

read-only

### Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| test_name | Str | Name or pattern of tests to run |
```

**Behavior:**

```colony
# Tools are available to agents
invoke code-reviewer(files=$files)
# Agent can call: build, test, lint, search_code, run_tests

# Tools also available directly
tool:run build
tool:run test --filter "parser"

# List project tools
tool:list
# -> build        (project)  cargo build --release
# -> test         (project)  cargo test
# -> lint         (project)  cargo clippy && cargo fmt --check
# -> search_code  (project)  rg --json
# -> run_tests    (project)  cargo test
```

### 4.5 PROMPT.md

Default prompt templates for the project:

```markdown
# PROMPT.md

## Prompt Templates

Reusable prompts for common tasks in this project.

---

## review

Standard code review prompt.

### Template

```
Review the following {{language}} code for:
- Correctness and potential bugs
- Performance issues
- Security vulnerabilities  
- Code style and readability

Project context:
{{context}}

Code to review:
{{code}}

Provide specific, actionable feedback with line numbers.
```

### Variables

| Variable | Source |
|----------|--------|
| language | auto-detect from file extension |
| context | CONTEXT.md + session context |
| code | input file contents |

---

## explain

Explain code or concepts.

### Template

```
Explain the following code/concept in the context of the Colony shell project:

{{input}}

Assume the reader is familiar with Rust but may not know GPUI or shell internals.
Use examples where helpful.
```

---

## fix

Fix an issue in code.

### Template

```
Fix the following issue in this {{language}} code:

Issue: {{issue}}

Code:
{{code}}

Requirements:
- Maintain existing behavior except for the fix
- Follow project conventions (see CONTEXT.md)
- Add comments explaining the fix
```

---

## test

Generate tests.

### Template

```
Generate tests for the following {{language}} code:

{{code}}

Requirements:
- Test happy path and error cases
- Use project testing conventions
- Include edge cases for: {{edge_cases}}
- Target {{coverage}} code coverage
```
```

**Behavior:**

```colony
# Use a prompt template
prompt:use review --file src/parser.rs

# Interpolate variables
prompt:use fix \
    --issue "Panic on empty input" \
    --file src/parser.rs

# List available prompts
prompt:list
# -> review   Standard code review prompt
# -> explain  Explain code or concepts
# -> fix      Fix an issue in code
# -> test     Generate tests

# Preview interpolated prompt
prompt:preview review --file src/parser.rs
```

### 4.6 .colony/config.toml

Project-specific Colony configuration:

```toml
# .colony/config.toml

[project]
name = "colony"
version = "0.1.0"

[shell]
# Override global settings for this project
live-preview = true
preview-safety = "cautious"

[context]
# Auto-load CONTEXT.md
auto-seed = true
# Additional context files to load
seed-files = ["docs/ARCHITECTURE.md", "docs/DECISIONS.md"]

[agents]
# Agent configuration
default-model = "claude-sonnet-4-20250514"
max-parallel = 3

# Per-agent overrides
[agents.code-reviewer]
model = "claude-sonnet-4-20250514"
temperature = 0.3

[agents.doc-writer]
model = "claude-sonnet-4-20250514"
temperature = 0.7

[tools]
# Tool configuration
[tools.build]
timeout = 300

[tools.test]
timeout = 600

[buffers]
# Buffer settings for this project
auto-save = true
llm-retention = "session"        # session, 7d, 30d, forever

[mcp]
# MCP servers to auto-connect
servers = ["filesystem", "github"]

[mcp.github]
repo = "owner/colony"
```

### 4.7 .colony/session.toml

Session state persistence:

```toml
# .colony/session.toml
# Automatically managed by Colony

[session]
id = "sess_abc123"
started = "2025-12-11T10:00:00Z"
last_active = "2025-12-11T14:30:00Z"

[context]
# Reference to context buffer file
buffer = "~/.local/share/colony/buffers/sess_abc123_context.jsonl"
entries = 47

[llm]
# Reference to LLM buffer file  
buffer = "~/.local/share/colony/buffers/sess_abc123_llm.json"
messages = 23
branches = ["main", "experiment-1"]
current_branch = "main"

[checkpoints]
"before-refactor" = "2025-12-11T12:00:00Z"
"working-state" = "2025-12-11T14:00:00Z"
```

---

## 5. File Discovery and Loading

### 5.1 Discovery Order

When Colony needs to find special files, it walks up the directory tree:

```
/home/jesse/projects/colony/crates/parser/src/
└── Look for: AGENT.md, CONTEXT.md, TOOLS.md, .colony/
    │
/home/jesse/projects/colony/crates/parser/
└── Look for: ...
    │
/home/jesse/projects/colony/crates/
└── Look for: ...
    │
/home/jesse/projects/colony/          ← Found! AGENT.md, CONTEXT.md, .colony/
└── Use these files
    │
/home/jesse/projects/                 ← Could also have files (org-wide)
└── Merge with project files
    │
/home/jesse/                          ← User-level defaults
└── ~/.colony/AGENT.md, etc.
```

### 5.2 Merging Behavior

```colony
# Agent definitions: Most specific wins, can extend
# If ~/projects/colony/AGENT.md defines code-reviewer
# and ~/projects/AGENT.md also defines code-reviewer
# The project-specific one is used

# Context: Accumulated from all levels
# Global context + org context + project context

# Tools: Merged, most specific wins for conflicts
# Config: Deep merged, most specific wins
```

### 5.3 Explicit Loading

```colony
# Force reload special files
colony:reload

# Load from specific location
colony:load --agent ./custom/AGENT.md
colony:load --context ./custom/CONTEXT.md

# Ignore directory's special files
colony:ignore .                # Ignore current directory
colony:ignore ~/projects/old   # Ignore specific path

# Check what's loaded
colony:status
# -> Project: ~/projects/colony
# -> AGENT.md: loaded (3 agents)
# -> CONTEXT.md: loaded (5 sections)
# -> TOOLS.md: loaded (5 tools)
# -> Config: .colony/config.toml
```

---

## 6. Integration Example

Here's how everything works together:

```colony
# Enter project directory
cd ~/projects/colony

# Colony automatically:
# 1. Loads AGENT.md → agents available
# 2. Loads CONTEXT.md → context seeded
# 3. Loads TOOLS.md → tools registered
# 4. Loads .colony/config.toml → settings applied
# 5. Restores .colony/session.toml → buffers restored

# Check status bar
# -> colony v0.1.0 | project: colony | ctx: 12 | llm: 5 msgs | sess: main-dev

# Start working - everything just works
invoke code-reviewer(files=["src/parser.rs"])

# The agent:
# - Knows project conventions (from CONTEXT.md)
# - Has access to project tools (from TOOLS.md)
# - Uses project-specific settings (from config.toml)
# - Conversation captured in llm buffer
# - Actions logged to context buffer

# View buffers
pane:layout dev                # Show main + context + llm

# Later, export session artifacts
llm:export --format markdown > review-session.md
context:export > session-context.json

# Session auto-saved to .colony/session.toml
# Next time you return, everything restores
```

---

## 7. Configuration Reference

### 7.1 Buffer Configuration

```colony
config set buffer.main.scrollback 10000
config set buffer.context.max-entries 1000
config set buffer.llm.max-messages 500
config set buffer.tool.max-calls 200
config set buffer.diagnostic.enabled false

config set buffer.auto-save true
config set buffer.save-interval 60s
config set buffer.save-path "~/.local/share/colony/buffers"
```

### 7.2 Pane Configuration

```colony
config set pane.default-layout "default"
config set pane.context.position "right"
config set pane.context.size "30%"
config set pane.llm.position "bottom"
config set pane.llm.size "40%"
config set pane.borders true
config set pane.titles true
```

### 7.3 Special Files Configuration

```colony
config set files.agent-file "AGENT.md"
config set files.context-file "CONTEXT.md"
config set files.tools-file "TOOLS.md"
config set files.prompt-file "PROMPT.md"
config set files.config-dir ".colony"
config set files.discovery true
config set files.discovery-depth 10          # Max directories to walk up
```

---

## 8. Keybindings Reference

```colony
# Buffer management
bind Alt+1 buffer:focus main
bind Alt+2 buffer:focus context
bind Alt+3 buffer:focus llm
bind Alt+4 buffer:focus tool

# Pane toggles
bind Ctrl+Shift+C pane:toggle context
bind Ctrl+Shift+L pane:toggle llm
bind Ctrl+Shift+T pane:toggle tool

# Layout presets
bind Ctrl+Shift+1 pane:layout default
bind Ctrl+Shift+2 pane:layout dev
bind Ctrl+Shift+3 pane:layout debug

# LLM shortcuts
bind Ctrl+Shift+Enter llm:chat           # Enter chat mode
bind Ctrl+Shift+/ llm:show --last 1      # Show last response

# Context shortcuts
bind Ctrl+Shift+N context:add            # Add context note
bind Ctrl+Shift+S context:show --last 5  # Show recent context
```

---

*End of Colony Channels, Buffers, and Special Files Specification*
