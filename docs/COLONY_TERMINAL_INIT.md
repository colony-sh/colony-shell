# Colony Terminal — Project Initialization Prompt

This document provides everything needed to initialize the colony-terminal project repository.

## Project Overview

Colony Terminal is a GPU-accelerated terminal emulator designed as the companion to Colony Shell. It consumes OSC 2337 escape sequences to provide rich visualizations, reactive prompts, agent status displays, interactive widgets, and pane management.

### Relationship to Colony Shell

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Colony Architecture                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────┐          ┌─────────────────────────────────┐  │
│  │     colony-shell        │          │       colony-terminal           │  │
│  │     (This repo)         │          │       (New repo)                │  │
│  │                         │  stdout  │                                 │  │
│  │  - POSIX shell          │─────────▶│  - Terminal emulation           │  │
│  │  - Parser (pest)        │  stdin   │  - GPU rendering (wgpu)         │  │
│  │  - Executor             │◀─────────│  - Prompt theming               │  │
│  │  - Tuple space          │          │  - Widget system                │  │
│  │  - Agent integration    │  OSC     │  - Agent visualization          │  │
│  │  - OSC 2337 emitter     │  2337    │  - OSC 2337 parser              │  │
│  └─────────────────────────┘          └─────────────────────────────────┘  │
│                                                                             │
│  Protocol: OSC 2337 (see colony-shell/docs/spec/appendix-b-osc2337.md)     │
└─────────────────────────────────────────────────────────────────────────────┘
```

Colony Shell handles:
- Command parsing and execution
- Process management and job control
- Agent/LLM integration
- State generation (prompt state, agent status)
- OSC 2337 sequence emission

Colony Terminal handles:
- Terminal emulation (VT100/xterm compatible)
- GPU-accelerated text rendering
- Prompt rendering from PromptState
- Widget display (approval dialogs, inputs)
- Agent status visualization
- Pane/buffer management
- Syntax highlighting
- Theming

---

## Repository Structure

```
colony-terminal/
├── .claude/
│   └── system/
│       ├── rules.txt       # Project rules and constraints
│       ├── features.json   # Feature tracking
│       └── history.txt     # Development history
├── CLAUDE.md               # Agent instructions
├── Cargo.toml              # Workspace root
├── crates/
│   ├── colony-term-core/   # Terminal emulation, PTY, escape sequences
│   ├── colony-term-render/ # GPU rendering with wgpu
│   ├── colony-term-ui/     # Widgets, prompts, agent displays
│   ├── colony-term-theme/  # Theming system
│   └── colony-term-app/    # Main application, window management
├── themes/
│   ├── default.toml
│   ├── dark.toml
│   └── light.toml
├── docs/
│   ├── ARCHITECTURE.md
│   └── OSC2337.md          # Local copy/reference
└── tests/
    └── integration/
```

---

## CLAUDE.md

```markdown
## Session Start Protocol

At the beginning of EVERY coding session, execute these steps IN ORDER:

### 1. Check Git Status
\`\`\`bash
git status
git log --oneline -10
\`\`\`

Review what's changed since last session. Note any uncommitted work.

### 2. Read System Files
\`\`\`bash
cat .claude/system/rules.txt
cat .claude/system/features.json
tail -50 .claude/system/history.txt
\`\`\`

Understand: What are the constraints? What features exist and their status? What was done recently?

### 3. Identify Current Focus

From features.json, find:
- Features with status "in_progress" — these are active work
- Features with status "blocked" — check if blockers are resolved
- Next "new" feature by priority (lowest number = highest priority) whose dependencies are "complete"

### 4. Announce Plan

Before writing any code, state:
- What you found in git history and system files
- What feature you're working on and why
- What specific task you'll do this session
- Any blockers or questions

---

## Updating System Files

### Updating features.json

When starting work on a feature:
\`\`\`json
{
  "id": "T001",
  "status": "in_progress",
  "notes": ["Started YYYY-MM-DD: Initial structure"]
}
\`\`\`

When completing a feature:
\`\`\`json
{
  "id": "T001",
  "status": "complete",
  "notes": ["Started YYYY-MM-DD: Initial structure", "Completed YYYY-MM-DD: All tests passing"]
}
\`\`\`

When blocked:
\`\`\`json
{
  "id": "T001",
  "status": "blocked",
  "notes": ["BLOCKED: Need decision on X"]
}
\`\`\`

### Updating history.txt

Log significant actions as you work:
\`\`\`
[2025-01-15 14:30] START: Beginning work on T001 (Core terminal)
[2025-01-15 14:45] PROGRESS: PTY creation working
[2025-01-15 15:30] DECISION: Using wgpu over glow for future WebGPU support
[2025-01-15 16:00] COMPLETE: T001 basic terminal emulation working
\`\`\`

### Updating rules.txt

Add rules when you discover important constraints:
\`\`\`
15. OSC 2337 sequences must be buffered until BEL terminator received
16. Prompt rendering must complete in < 16ms for 60fps
\`\`\`

---

## Session End Protocol

Before ending a session:

1. **Commit work** with descriptive message referencing feature ID
2. **Update features.json** with current status and notes
3. **Add history.txt entries** for all significant work done
4. **Summarize** what was accomplished and what's next
```

---

## .claude/system/rules.txt

```
# Colony Terminal — Project Rules and Constraints

## Scope

1. This is the TERMINAL component only — it renders output, doesn't execute commands
2. Communication with colony-shell happens via OSC 2337 escape sequences
3. Must work as standalone terminal (without colony-shell) for standard programs
4. GPU rendering is primary target; software fallback is optional
5. Cross-platform: macOS, Linux, (Windows via WSL initially)

## Technical Constraints

6. Use wgpu for GPU rendering — enables future WebGPU/browser support
7. Use winit for window management
8. Use vte crate for VT100/xterm escape sequence parsing
9. Use portable-pty for PTY management
10. Use serde for configuration and theme files
11. Performance target: 60fps minimum, < 16ms frame time
12. Memory target: < 100MB for typical session with scrollback

## Rendering Architecture

13. Text rendered as instanced quads with glyph atlas
14. Font rasterization via fontdue (pure Rust, no system dependencies)
15. Double-buffered rendering to prevent tearing
16. Dirty region tracking — only re-render changed cells
17. Scrollback stored in ring buffer, rendered on demand

## OSC 2337 Protocol

18. Buffer OSC sequences until BEL (0x07) or ST (ESC \) terminator
19. Parse command and JSON payload, dispatch to handler
20. Unknown commands logged but ignored (forward compatibility)
21. Widget responses sent via stdin to shell process
22. Prompt=state triggers full prompt re-render
23. Agent=streaming updates rendered incrementally

## UI Components

24. Prompt rendered from PromptState JSON (not escape codes)
25. Widgets (approval, input, select) rendered as overlays
26. Agent status shown in dedicated region (configurable position)
27. Context sidebar optional (toggle with keybind)
28. Buffers can be shown in split panes

## Theming

29. Themes are TOML files with color palettes and prompt layout
30. Support 16-color, 256-color, and true color modes
31. Prompt segments configurable per-theme
32. Icons require Nerd Font or fallback to ASCII

## Code Quality

33. All public APIs must have doc comments
34. Integration tests with colony-shell for OSC 2337
35. Visual regression tests for rendering
36. No unwrap() in library code — propagate errors
37. Use tracing for structured logging
```

---

## .claude/system/features.json

```json
{
  "version": 1,
  "features": [
    {
      "id": "T001",
      "name": "Core terminal emulation",
      "description": "Basic VT100/xterm terminal emulation using vte crate. PTY creation, input handling, escape sequence processing.",
      "status": "new",
      "priority": 1,
      "dependencies": [],
      "notes": []
    },
    {
      "id": "T002",
      "name": "GPU text rendering",
      "description": "Render terminal grid using wgpu. Glyph atlas generation with fontdue, instanced quad rendering, basic colors.",
      "status": "new",
      "priority": 1,
      "dependencies": ["T001"],
      "notes": []
    },
    {
      "id": "T003",
      "name": "Window and input",
      "description": "Window creation with winit, keyboard input handling, mouse support, clipboard integration.",
      "status": "new",
      "priority": 1,
      "dependencies": ["T002"],
      "notes": []
    },
    {
      "id": "T004",
      "name": "Scrollback buffer",
      "description": "Ring buffer for scrollback history, smooth scrolling, scroll position indicator.",
      "status": "new",
      "priority": 2,
      "dependencies": ["T002"],
      "notes": []
    },
    {
      "id": "T005",
      "name": "OSC 2337 parser",
      "description": "Parse OSC 2337 escape sequences from shell. Buffer until terminator, extract command and JSON payload, dispatch to handlers.",
      "status": "new",
      "priority": 2,
      "dependencies": ["T001"],
      "notes": []
    },
    {
      "id": "T006",
      "name": "Prompt renderer",
      "description": "Render prompts from PromptState JSON. Support segments (path, git, agent), icons, colors. Theme-driven layout.",
      "status": "new",
      "priority": 2,
      "dependencies": ["T005", "T002"],
      "notes": []
    },
    {
      "id": "T007",
      "name": "Agent status display",
      "description": "Display agent status from Agent=status/streaming/tool OSC commands. Streaming text, spinner, tool call visualization.",
      "status": "new",
      "priority": 3,
      "dependencies": ["T005", "T002"],
      "notes": []
    },
    {
      "id": "T008",
      "name": "Widget system",
      "description": "Render interactive widgets (approval, input, select) as overlays. Handle user interaction, send responses via stdin.",
      "status": "new",
      "priority": 3,
      "dependencies": ["T005", "T003"],
      "notes": []
    },
    {
      "id": "T009",
      "name": "Theming system",
      "description": "Load themes from TOML files. Color palettes, prompt segment configuration, font settings.",
      "status": "new",
      "priority": 3,
      "dependencies": ["T006"],
      "notes": []
    },
    {
      "id": "T010",
      "name": "Pane management",
      "description": "Split terminal into panes (horizontal/vertical). Buffer display in panes. Pane focus and resize.",
      "status": "new",
      "priority": 4,
      "dependencies": ["T003", "T002"],
      "notes": []
    },
    {
      "id": "T011",
      "name": "Context sidebar",
      "description": "Optional sidebar showing context entries from Context=add OSC commands. Toggle visibility, entry management.",
      "status": "new",
      "priority": 4,
      "dependencies": ["T005", "T010"],
      "notes": []
    },
    {
      "id": "T012",
      "name": "Syntax highlighting",
      "description": "Syntax highlighting for code output using Meta=highlight hints. Tree-sitter integration optional.",
      "status": "new",
      "priority": 4,
      "dependencies": ["T002"],
      "notes": []
    },
    {
      "id": "T013",
      "name": "Selection and copy",
      "description": "Text selection with mouse, keyboard selection mode, copy to clipboard, URL detection and clicking.",
      "status": "new",
      "priority": 3,
      "dependencies": ["T003"],
      "notes": []
    },
    {
      "id": "T014",
      "name": "Search",
      "description": "Search within scrollback buffer, highlight matches, jump between matches.",
      "status": "new",
      "priority": 4,
      "dependencies": ["T004"],
      "notes": []
    },
    {
      "id": "T015",
      "name": "Configuration",
      "description": "Configuration file loading, hot reload, keybinding customization, shell path configuration.",
      "status": "new",
      "priority": 3,
      "dependencies": [],
      "notes": []
    },
    {
      "id": "T016",
      "name": "Tabs",
      "description": "Multiple terminal tabs, tab bar rendering, tab switching, tab reordering.",
      "status": "new",
      "priority": 4,
      "dependencies": ["T003"],
      "notes": []
    }
  ]
}
```

---

## .claude/system/history.txt

```
# Colony Terminal — Development History
# Format: [YYYY-MM-DD HH:MM] ACTION: Description
# Actions: START, PROGRESS, COMPLETE, BLOCKED, DECISION, REFACTOR, BUG, FIX

[2025-12-17 00:00] START: Project initialized with tracking system
[2025-12-17 00:00] DECISION: Using wgpu for GPU rendering (WebGPU future-proofing)
[2025-12-17 00:00] DECISION: Using vte crate for escape sequence parsing
[2025-12-17 00:00] DECISION: Using fontdue for font rasterization (pure Rust)
[2025-12-17 00:00] DECISION: Themes in TOML format for human readability
```

---

## Cargo.toml (Workspace Root)

```toml
[workspace]
resolver = "2"
members = [
    "crates/colony-term-core",
    "crates/colony-term-render",
    "crates/colony-term-ui",
    "crates/colony-term-theme",
    "crates/colony-term-app",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/mcfearsome/colony-terminal"

[workspace.dependencies]
# Terminal emulation
vte = "0.13"
portable-pty = "0.8"

# Rendering
wgpu = "0.19"
winit = "0.29"
fontdue = "0.8"
image = "0.24"

# Async
tokio = { version = "1.35", features = ["full"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"

# Utilities
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
parking_lot = "0.12"
unicode-width = "0.1"
dirs = "5.0"

# Internal crates
colony-term-core = { path = "crates/colony-term-core" }
colony-term-render = { path = "crates/colony-term-render" }
colony-term-ui = { path = "crates/colony-term-ui" }
colony-term-theme = { path = "crates/colony-term-theme" }
```

---

## Key Implementation Notes

### OSC 2337 Handler Structure

```rust
// crates/colony-term-core/src/osc.rs

pub enum OscCommand {
    // Agent commands
    AgentStatus(AgentStatusPayload),
    AgentStreaming(StreamingPayload),
    AgentTool(ToolPayload),
    AgentComplete,
    AgentError(String),

    // Widget commands
    WidgetApproval(ApprovalRequest),
    WidgetInput(InputRequest),
    WidgetSelect(SelectRequest),
    WidgetProgress(ProgressPayload),

    // Prompt commands
    PromptState(PromptState),
    PromptUpdate(serde_json::Value),
    PromptReady { timestamp: u64, last_duration_ms: u64 },
    PromptBusy { command: String, started_at: u64 },

    // Buffer commands
    BufferCreate { name: String },
    BufferWrite(BufferWritePayload),
    BufferClear { name: String },
    BufferShow { name: String },
    BufferHide { name: String },

    // Context commands
    ContextAdd(ContextEntry),
    ContextUpdate(ContextEntry),
    ContextRemove { id: String },
    ContextClear,

    // Metadata commands
    MetaType(TypeAnnotation),
    MetaSchema(serde_json::Value),
    MetaFold(FoldPayload),
    MetaHighlight { language: String },

    // Unknown (forward compatibility)
    Unknown { command: String, payload: String },
}

pub fn parse_osc_2337(data: &[u8]) -> Option<OscCommand> {
    // Find command;payload separator
    let sep = data.iter().position(|&b| b == b';')?;
    let command = std::str::from_utf8(&data[..sep]).ok()?;
    let payload = std::str::from_utf8(&data[sep+1..]).ok()?;

    match command {
        "Agent=status" => serde_json::from_str(payload).ok().map(OscCommand::AgentStatus),
        "Agent=streaming" => serde_json::from_str(payload).ok().map(OscCommand::AgentStreaming),
        "Prompt=state" => serde_json::from_str(payload).ok().map(OscCommand::PromptState),
        // ... etc
        _ => Some(OscCommand::Unknown {
            command: command.to_string(),
            payload: payload.to_string()
        }),
    }
}
```

### Prompt Rendering

```rust
// crates/colony-term-ui/src/prompt.rs

pub struct PromptRenderer {
    config: PromptConfig,
    icons: IconSet,
}

impl PromptRenderer {
    pub fn render(&self, state: &PromptState, theme: &Theme) -> Vec<PromptSegment> {
        let mut segments = Vec::new();

        // Context markers
        for ctx in &state.context {
            segments.push(PromptSegment {
                text: ctx.label.clone(),
                style: theme.context_style(&ctx.kind),
                icon: self.icons.for_context(&ctx.kind),
            });
        }

        // Path segment
        segments.push(PromptSegment {
            text: state.display_path.clone(),
            style: theme.path_style(),
            icon: Some(self.icons.folder.clone()),
        });

        // Git segment
        if let Some(ref git) = state.git {
            let style = if git.status.modified > 0 || git.status.staged > 0 {
                theme.git_dirty_style()
            } else {
                theme.git_clean_style()
            };

            let mut text = git.branch.clone();
            if git.status.staged > 0 {
                text.push_str(&format!(" +{}", git.status.staged));
            }
            if git.status.modified > 0 {
                text.push_str(&format!(" ~{}", git.status.modified));
            }

            segments.push(PromptSegment {
                text,
                style,
                icon: Some(self.icons.git_branch.clone()),
            });
        }

        // Agent segment
        if let Some(ref agent) = state.agent {
            if !matches!(agent.state, AgentState::Idle) {
                segments.push(PromptSegment {
                    text: format!("{}", agent.name),
                    style: theme.agent_style(&agent.state),
                    icon: Some(self.icons.for_agent_state(&agent.state)),
                });
            }
        }

        // Exit status (if non-zero)
        if state.exit_status != 0 {
            segments.push(PromptSegment {
                text: format!("{}", state.exit_status),
                style: theme.error_style(),
                icon: Some(self.icons.error.clone()),
            });
        }

        segments
    }
}
```

### Default Theme

```toml
# themes/default.toml

[meta]
name = "Colony Default"
author = "Colony Team"
version = "1.0"

[colors]
background = "#1a1b26"
foreground = "#c0caf5"
cursor = "#c0caf5"
selection = "#33467c"

# ANSI colors
black = "#15161e"
red = "#f7768e"
green = "#9ece6a"
yellow = "#e0af68"
blue = "#7aa2f7"
magenta = "#bb9af7"
cyan = "#7dcfff"
white = "#a9b1d6"

# Bright variants
bright_black = "#414868"
bright_red = "#f7768e"
bright_green = "#9ece6a"
bright_yellow = "#e0af68"
bright_blue = "#7aa2f7"
bright_magenta = "#bb9af7"
bright_cyan = "#7dcfff"
bright_white = "#c0caf5"

[prompt]
style = "powerline"  # powerline, minimal, classic

[prompt.left]
segments = ["context", "path", "git"]

[prompt.right]
segments = ["agent", "exit_status", "time"]

[prompt.separator]
left = ""
right = ""

[prompt.colors]
path_bg = "#7aa2f7"
path_fg = "#1a1b26"
git_clean_bg = "#9ece6a"
git_clean_fg = "#1a1b26"
git_dirty_bg = "#e0af68"
git_dirty_fg = "#1a1b26"
context_bg = "#bb9af7"
context_fg = "#1a1b26"
error_bg = "#f7768e"
error_fg = "#1a1b26"
agent_thinking_bg = "#7dcfff"
agent_thinking_fg = "#1a1b26"

[prompt.icons]
folder = ""
home = ""
git_branch = ""
git_modified = "✱"
git_staged = "●"
error = ""
agent_thinking = ""
agent_executing = ""

[font]
family = "JetBrainsMono Nerd Font"
size = 14.0
line_height = 1.2

[window]
padding = 8
opacity = 1.0
decorations = true

[cursor]
style = "block"  # block, underline, bar
blink = true
blink_interval_ms = 500

[scrollback]
lines = 10000
```

---

## Getting Started Commands

After creating the repository:

```bash
# Create directory structure
mkdir -p colony-terminal/{crates,.claude/system,themes,docs}
cd colony-terminal

# Initialize git
git init

# Create workspace and crate structure
cargo new --lib crates/colony-term-core
cargo new --lib crates/colony-term-render
cargo new --lib crates/colony-term-ui
cargo new --lib crates/colony-term-theme
cargo new --bin crates/colony-term-app

# Copy system files from this document
# - .claude/system/rules.txt
# - .claude/system/features.json
# - .claude/system/history.txt
# - CLAUDE.md
# - Cargo.toml (workspace root)
# - themes/default.toml

# Build to verify structure
cargo build

# First commit
git add .
git commit -m "Initial project structure with tracking system"
```

---

## Integration Testing with Colony Shell

```bash
# In colony-shell repo
cargo build --release
export COLONY_SHELL=/path/to/colony-shell/target/release/colony-shell

# In colony-terminal repo
cargo run -- --shell $COLONY_SHELL

# Test OSC 2337 by running colony-shell commands that emit sequences
```

---

## Success Criteria

Colony Terminal is ready when:

1. **Standalone terminal works** - Can run bash, vim, htop without colony-shell
2. **OSC 2337 parsing** - All command types parsed and dispatched
3. **Prompt rendering** - PromptState renders with git, context, agent info
4. **Widget interaction** - Approval dialogs work, responses sent to shell
5. **60fps rendering** - Smooth scrolling, no visible frame drops
6. **Theme support** - Multiple themes load and apply correctly
7. **Integration tests** - Colony shell + terminal work together

---

*This document should be placed at `docs/COLONY_TERMINAL_INIT.md` in colony-shell for reference, then used to bootstrap the colony-terminal repository.*
