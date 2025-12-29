# Reactive TUI Design

Date: 2025-12-29

## Overview

Replace rustyline-based REPL with a full ratatui TUI featuring programmable prompt components, sidebars, vim-style modal editing, and reactive updates via tuple space. Component definitions are shared with colony-terminal (separate GPU-based terminal) through a common abstraction layer.

## Architecture

### Component System

```
┌─────────────────────────────────────────────────────────────────┐
│                    .colonyrc / Component Definitions            │
│         (TOML + shell - prompt components, sidebar panels)      │
└─────────────────────────────────┬───────────────────────────────┘
                                  │
                    ┌─────────────┴─────────────┐
                    ▼                           ▼
┌─────────────────────────────┐   ┌─────────────────────────────┐
│      colony-tui (new)       │   │    colony-terminal (wgpu)   │
│   Renders with ratatui      │   │   Renders with GPU          │
│   Works in any terminal     │   │   OSC 2337 protocol         │
│   Portable fallback         │   │   Premium experience        │
└─────────────────────────────┘   └─────────────────────────────┘
```

### Layout Structure

```
┌─────────────┬────────────────────────────────────┬─────────────┐
│ LEFT        │         MAIN CONTENT               │ RIGHT       │
│ SIDEBAR     │                                    │ SIDEBAR     │
│ (optional)  │   Shell output (raw scrollback)    │ (optional)  │
│             │   Agent tasks (structured tree)    │             │
├─────────────┴────────────────────────────────────┴─────────────┤
│                         INPUT BOX                              │
│  [prompt components: left]              [prompt components: right]  │
│  > user input with syntax highlighting                         │
│  [status bar / mode indicator / shortcuts]                     │
└────────────────────────────────────────────────────────────────┘
```

### Data Flow

```
Tuple Space ──events──▶ Component Subscriptions ──▶ Targeted Re-renders
     ▲                                                      │
     │                                                      ▼
User Input ──▶ Parser ──▶ Executor ─────────────▶ Output to Content Area
```

## Crate Structure

### New Crate: colony-ui-core (shared)

Shared component definitions consumed by both colony-tui and colony-terminal.

```
crates/colony-ui-core/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── component.rs      # PromptComponent, SidebarComponent traits
    ├── subscription.rs   # TuplePattern matching
    ├── config.rs         # TOML parsing for user components
    ├── state.rs          # ComponentState, shared data structures
    └── builtins/         # Built-in component logic (no rendering)
        ├── mod.rs
        ├── pwd.rs
        ├── git.rs
        ├── model.rs
        ├── lsp.rs
        ├── jobs.rs
        └── exit_status.rs
```

### New Crate: colony-tui (ratatui renderer)

```
crates/colony-tui/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── app.rs              # App state, main event loop
    ├── event.rs            # Terminal event handling
    ├── theme.rs            # Colors, styles, mode indicators
    │
    ├── input/
    │   ├── mod.rs
    │   ├── editor.rs       # Buffer, cursor, vim motions
    │   ├── highlight.rs    # Tree-sitter integration
    │   ├── completion.rs   # Ghost text, fuzzy matching
    │   └── history.rs      # History navigation, fzf search
    │
    ├── prompt/
    │   ├── mod.rs
    │   ├── renderer.rs     # Render components to ratatui spans
    │   └── layout.rs       # Left/right arrangement
    │
    ├── sidebar/
    │   ├── mod.rs
    │   ├── renderer.rs     # Render panels to ratatui blocks
    │   └── panel.rs        # Collapsible panel widget
    │
    ├── content/
    │   ├── mod.rs
    │   ├── raw.rs          # Shell output scrollback
    │   ├── structured.rs   # Agent task tree
    │   └── block.rs        # ContentBlock types
    │
    ├── keybindings/
    │   ├── mod.rs
    │   ├── modes.rs        # Normal, Insert, Command, Search
    │   ├── actions.rs      # Action enum
    │   └── palette.rs      # Command palette
    │
    └── subscriptions.rs    # Tuple space integration
```

## Component System

### Rust Trait (for built-ins)

```rust
pub trait PromptComponent: Send + Sync {
    /// Tuple patterns this component cares about
    fn subscriptions(&self) -> Vec<TuplePattern>;
    
    /// Render current state (renderer-agnostic)
    fn render(&self, ctx: &ComponentContext) -> ComponentOutput;
    
    /// Optional: poll interval (0 = event-only)
    fn poll_interval(&self) -> Duration { Duration::ZERO }
}

pub trait SidebarComponent: PromptComponent {
    /// Panel title
    fn title(&self) -> &str;
    
    /// Can this panel collapse?
    fn collapsible(&self) -> bool { true }
    
    /// Render as block content (multiple lines)
    fn render_block(&self, ctx: &ComponentContext) -> Vec<ComponentLine>;
}
```

### User TOML Definition

```toml
[component.docker_status]
command = "docker ps -q | wc -l | xargs printf '%d containers'"
icon = "🐳"
style = { fg = "blue" }
subscribe = ["dir_change"]
interval = 30  # also poll every 30s
condition = "command -v docker"  # only show if docker exists
```

### Prompt Configuration

```toml
[prompt]
left = ["pwd", "git_branch", "git_status"]
right = ["model", "lsp", "jobs"]
gap = "  "  # separator between components
```

### Sidebar Configuration

```toml
[sidebar.left]
width = 25
panels = ["file_tree"]

[sidebar.right]
width = 30
panels = ["session", "mcp", "lsp", "git_files"]
collapsed = ["session"]  # start collapsed
```

### Built-in Components

| Name | Type | Subscribes To | Description |
|------|------|---------------|-------------|
| `pwd` | prompt | `dir_change` | Current directory (truncated) |
| `git_branch` | prompt | `dir_change` | Branch name with icon |
| `git_status` | prompt | `dir_change`, `file_change` | Dirty/staged indicators |
| `jobs` | prompt | `job_change` | Background job count |
| `model` | prompt | `model_change` | Current AI model |
| `lsp` | prompt | `lsp_status` | LSP server status |
| `exit_status` | prompt | `cmd_complete` | Last exit code (if non-zero) |
| `file_tree` | sidebar | `dir_change`, `file_change` | Directory tree |
| `session` | sidebar | `cmd_complete` | Token count, cost |
| `mcp` | sidebar | `mcp_status` | Connected MCP servers |
| `git_files` | sidebar | `file_change` | Modified files with diff stats |

## Input Box / Mini-Editor

Full code editor optimized for shell input:

| Feature | Implementation |
|---------|----------------|
| Syntax highlighting | Tree-sitter `colony-tree-sitter` |
| Multi-line editing | Expand on `\` continuation, heredocs, `{` |
| Ghost completions | Shadow text from history, files, commands |
| Fuzzy search | fzf-style popup for history and completions |
| Partial execution | Run subexpressions for live hints |
| Vim motions | Normal/insert modes, `hjkl`, `w/b/e`, `ciw`, etc. |

### Completion Sources (ranked)

1. History - Previous commands (weighted by recency/frequency)
2. File paths - After `/`, `./`, or whitespace
3. Commands - Executables in `$PATH`, builtins, aliases, functions
4. Git - Branch names, remotes (context-aware)
5. Variables - After `$` or `${`
6. Custom - User-defined completers via tuple space

### Configuration

```toml
[input]
live_hints = true
live_hint_delay_ms = 500
```

## Main Content Area

### Dual Rendering Modes

**Raw Mode (shell commands):**
Traditional scrollback. Output streams directly, ANSI colors preserved.

**Structured Mode (agent interactions):**
```
● Task "Fix the failing test"
  ├─ Read tests/foo.rs
  ├─ Edit src/lib.rs [+3 -1]
  ├─ Bash cargo test
  │   └─ ✓ 47 passed
  └─ ✓ Complete
```

Collapsible tree. Tool calls grouped under task.

### Content Block Types

```rust
pub enum ContentBlock {
    Shell {
        command: String,
        output: Vec<u8>,
        exit_code: i32,
        collapsed: bool,
    },
    Agent {
        task: String,
        steps: Vec<AgentStep>,
        status: TaskStatus,
        collapsed: bool,
    },
}
```

## Keybindings

### Modal Structure

```
┌─────────────────────────────────────────────────────────┐
│                      NORMAL MODE                        │
│  Navigation, scrolling, panel management                │
│                                                         │
│  i/a/o → INSERT    :/ Ctrl+p → COMMAND    / → SEARCH   │
└────────────┬────────────────────┬───────────────────────┘
             ▼                    ▼
┌────────────────────┐  ┌─────────────────────┐
│    INSERT MODE     │  │   COMMAND MODE      │
│  Typing in editor  │  │  Command palette    │
│  Esc → NORMAL      │  │  Esc/Enter → NORMAL │
└────────────────────┘  └─────────────────────┘
```

### Normal Mode

| Key | Action |
|-----|--------|
| `i` | Enter insert mode |
| `a` | Enter insert mode (after cursor) |
| `o` | New line below, enter insert |
| `j/k` | Scroll content |
| `h/l` | Switch sidebar focus |
| `gg/G` | Top/bottom of content |
| `Ctrl+b` | Toggle left sidebar |
| `Ctrl+n` | Toggle right sidebar |
| `Ctrl+p` or `:` | Command palette |
| `/` | Search content |

### Insert Mode

| Key | Action |
|-----|--------|
| `Esc` | Return to normal mode |
| `Ctrl+r` | fzf history search |
| `Tab` | Accept ghost completion |
| `Ctrl+n/p` | Cycle completions |
| `Enter` | Execute or newline |

### Global (both modes)

| Key | Action |
|-----|--------|
| `Ctrl+p` | Command palette |
| `Ctrl+b` | Toggle left sidebar |
| `Ctrl+n` | Toggle right sidebar |
| `Ctrl+c` | Interrupt |
| `Ctrl+d` | EOF |

## Tuple Space Events

| Pattern | Emitted When |
|---------|--------------|
| `("dir_change", path)` | `cd` or startup |
| `("env_change", key, val)` | `export`, `unset` |
| `("job_change", action, id)` | Background job state |
| `("cmd_complete", code)` | Command finishes |
| `("git_change", repo)` | Git state changes |
| `("model_change", model)` | AI model switched |
| `("lsp_status", lang, status)` | LSP connects/disconnects |
| `("mcp_status", server, status)` | MCP server state |
| `("file_change", path)` | File modified |
| `("mode_change", mode)` | Editor mode switch |

## Render Mode Detection

```rust
pub enum RenderMode {
    ColonyTerminal,  // Inside colony-terminal - emit OSC 2337
    Tui,             // Regular terminal - use ratatui
    Plain,           // Dumb terminal / pipe - plain text
}

fn detect_render_mode() -> RenderMode {
    if env::var("COLONY_TERMINAL").is_ok() {
        RenderMode::ColonyTerminal
    } else if is_tty() && term_supports_tui() {
        RenderMode::Tui
    } else {
        RenderMode::Plain
    }
}
```

## Startup Behavior

- Minimal by default (input + content, no sidebars)
- Sidebars toggle with keybind or configured in `.colonyrc`
- Per-directory config can set defaults
- State persists in `~/.colony/state/`

## Implementation Phases

### Phase 1: colony-ui-core
1. Component traits (PromptComponent, SidebarComponent)
2. Subscription/tuple pattern matching
3. TOML config parser
4. Built-in component logic (pwd, git, model - no rendering)

### Phase 2: colony-tui basics
5. App loop with ratatui
6. Input editor with syntax highlighting
7. Raw content area
8. Render mode detection

### Phase 3: colony-tui prompt
9. Prompt renderer (consumes colony-ui-core components)
10. Built-in prompt components wired up
11. User TOML components working

### Phase 4: colony-tui editor
12. Vim motions
13. Ghost completions + fzf
14. Partial execution hints

### Phase 5: colony-tui sidebars
15. Sidebar renderer
16. Built-in panels
17. Toggle keybinds

### Phase 6: colony-terminal integration
18. OSC 2337 emitter using colony-ui-core state
19. Verify colony-terminal renders same components
20. End-to-end testing

## Success Criteria

1. Shell works in any terminal with full TUI (ratatui)
2. Same component config works in colony-terminal
3. Prompt components react to tuple space events
4. Vim motions feel native
5. Completions are fast and relevant
6. Sidebars toggle smoothly
7. No regression in shell functionality
