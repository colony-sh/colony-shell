# Part 1: Architecture

## 1.1 System Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Colony Shell                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │   Parser    │  │  Evaluator  │  │   Runtime   │  │     UI      │        │
│  │             │  │             │  │             │  │   (GPUI)    │        │
│  │ - Lexer     │  │ - Commands  │  │ - Processes │  │ - Buffers   │        │
│  │ - AST       │  │ - Exprs     │  │ - Jobs      │  │ - Panes     │        │
│  │ - Modes     │  │ - Pipelines │  │ - Signals   │  │ - Preview   │        │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘        │
│         │                │                │                │               │
│         └────────────────┴────────────────┴────────────────┘               │
│                                   │                                         │
│  ┌────────────────────────────────┴────────────────────────────────────┐   │
│  │                         Core Services                                │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │   │
│  │  │ Channels │ │  Agents  │ │  Tuple   │ │   MCP    │ │ Sessions │  │   │
│  │  │ Manager  │ │  Engine  │ │  Space   │ │  Client  │ │ Manager  │  │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 1.2 Crate Structure

```
colony/
├── Cargo.toml                     # Workspace manifest
├── crates/
│   ├── colony-parser/             # Pest PEG parser, AST, expansion
│   ├── colony-exec/               # Execution engine, jobs, builtins
│   ├── colony-expand/             # POSIX word expansion
│   ├── colony-agent/              # Agent framework, Claude SDK
│   ├── colony-mcp/                # MCP client (rmcp)
│   ├── colony-tuple/              # Tuple space coordination
│   ├── colony-memory/             # Memory substrate (SQLite + vectors)
│   ├── colony-protocol/           # OSC 2337 terminal protocol
│   ├── colony-focus/              # Focus tracking
│   ├── colony-tree-sitter/        # Tree-sitter parser (editor features)
│   └── colony-shell/              # Binary entry points, REPL
├── docs/
│   ├── spec/                      # This specification
│   ├── design/                    # Design documents
│   └── archive/                   # Historical documents
└── tests/
    ├── posix/                     # POSIX compliance tests
    ├── integration/               # Integration tests
    └── fixtures/                  # Test fixtures
```

### 1.2.1 Crate Mapping (Spec to Implementation)

| Spec Name | Implementation | Status | Notes |
|-----------|----------------|--------|-------|
| colony-core | (merged into others) | N/A | Core types distributed |
| colony-parser | colony-parser | Complete | Pest PEG + Tree-sitter |
| colony-runtime | colony-exec | Complete | Renamed for clarity |
| colony-channels | (planned) | Future | Extended channel system |
| colony-agents | colony-agent | Complete | Singular, with claude-sdk |
| colony-tuple-space | colony-tuple | Partial | In-process only |
| colony-mcp | colony-mcp | Stubbed | Awaiting rmcp integration |
| colony-ui | (planned) | Future | GPUI terminal |
| colony-cli | colony-shell | Complete | Includes REPL |
| — | colony-expand | Added | Split from parser |
| — | colony-memory | Added | Memory substrate |
| — | colony-protocol | Added | OSC 2337 |
| — | colony-focus | Added | Focus tracking |
| — | colony-tree-sitter | Added | Editor parser |

## 1.3 Crate: colony-core

Core types, traits, and configuration shared across all crates.

```rust
// crates/colony-core/src/lib.rs
pub mod types;      // Value, Type, PipelineType
pub mod error;      // ColonyError, ErrorContext
pub mod config;     // Configuration management
pub mod options;    // ShellOptions, shopt groups
pub mod span;       // Source location tracking
```

### 1.3.1 Shell Options

```rust
// crates/colony-core/src/options.rs

#[derive(Clone, Debug, Default)]
pub struct ShellOptions {
    // === Parsing Options ===
    /// Enable if (expr) { } syntax
    pub parse_paren: bool,
    /// Enable { } blocks after commands
    pub parse_brace: bool,
    /// Enable @array splicing
    pub parse_at: bool,
    /// Enable -> typed pipe operator
    pub parse_arrow: bool,
    /// Enable """ and ''' multi-line strings
    pub parse_triple_quote: bool,
    /// Enable r'raw strings'
    pub parse_raw_string: bool,
    /// Enable func keyword
    pub parse_func: bool,
    /// Enable proc keyword
    pub parse_proc: bool,
    /// Enable agent keyword
    pub parse_agent: bool,

    // === Evaluation Options ===
    /// No implicit word splitting on $var
    pub simple_word_eval: bool,
    /// errexit applies inside $()
    pub command_sub_errexit: bool,
    /// pipefail for process substitutions
    pub process_sub_fail: bool,
    /// SIGPIPE (141) treated as success in pipelines
    pub sigpipe_status_ok: bool,
    /// Use lexical scoping for procs/funcs
    pub lexical_scope: bool,
    /// ENV as associative array
    pub env_dict: bool,

    // === Strict Options ===
    pub strict_argv: bool,
    pub strict_array: bool,
    pub strict_errexit: bool,
    pub strict_control_flow: bool,
    pub strict_word_eval: bool,
    pub strict_glob: bool,
    pub strict_nameref: bool,
}

impl ShellOptions {
    /// POSIX-compatible defaults (all features disabled)
    pub fn posix() -> Self {
        Self::default()
    }

    /// Minimal modern upgrades (safe interop)
    pub fn colony_upgrade() -> Self {
        Self {
            parse_paren: true,
            parse_brace: true,
            parse_at: true,
            parse_arrow: true,
            simple_word_eval: true,
            sigpipe_status_ok: true,
            ..Self::default()
        }
    }

    /// Full modern mode
    pub fn colony_all() -> Self {
        Self {
            parse_paren: true,
            parse_brace: true,
            parse_at: true,
            parse_arrow: true,
            parse_triple_quote: true,
            parse_raw_string: true,
            parse_func: true,
            parse_proc: true,
            parse_agent: true,
            simple_word_eval: true,
            command_sub_errexit: true,
            process_sub_fail: true,
            sigpipe_status_ok: true,
            lexical_scope: true,
            env_dict: true,
            strict_argv: true,
            strict_array: true,
            strict_errexit: true,
            strict_control_flow: true,
            strict_word_eval: true,
            strict_glob: true,
            strict_nameref: true,
        }
    }

    /// Apply an option group by name
    pub fn apply_group(&mut self, group: &str) -> Result<(), OptionsError> {
        match group {
            "colony:upgrade" => *self = Self::colony_upgrade(),
            "colony:all" => *self = Self::colony_all(),
            "strict:all" => {
                self.strict_argv = true;
                self.strict_array = true;
                self.strict_errexit = true;
                self.strict_control_flow = true;
                self.strict_word_eval = true;
                self.strict_glob = true;
                self.strict_nameref = true;
            }
            _ => return Err(OptionsError::UnknownGroup(group.to_string())),
        }
        Ok(())
    }

    /// Set a single option by name
    pub fn set(&mut self, name: &str, value: bool) -> Result<(), OptionsError> {
        match name {
            "parse_paren" => self.parse_paren = value,
            "parse_brace" => self.parse_brace = value,
            "parse_at" => self.parse_at = value,
            "parse_arrow" => self.parse_arrow = value,
            "parse_triple_quote" => self.parse_triple_quote = value,
            "parse_raw_string" => self.parse_raw_string = value,
            "parse_func" => self.parse_func = value,
            "parse_proc" => self.parse_proc = value,
            "parse_agent" => self.parse_agent = value,
            "simple_word_eval" => self.simple_word_eval = value,
            "command_sub_errexit" => self.command_sub_errexit = value,
            "process_sub_fail" => self.process_sub_fail = value,
            "sigpipe_status_ok" => self.sigpipe_status_ok = value,
            "lexical_scope" => self.lexical_scope = value,
            "env_dict" => self.env_dict = value,
            "strict_argv" => self.strict_argv = value,
            "strict_array" => self.strict_array = value,
            "strict_errexit" => self.strict_errexit = value,
            "strict_control_flow" => self.strict_control_flow = value,
            "strict_word_eval" => self.strict_word_eval = value,
            "strict_glob" => self.strict_glob = value,
            "strict_nameref" => self.strict_nameref = value,
            _ => return Err(OptionsError::UnknownOption(name.to_string())),
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OptionsError {
    #[error("Unknown option group: {0}")]
    UnknownGroup(String),
    #[error("Unknown option: {0}")]
    UnknownOption(String),
}
```

## 1.4 Crate: colony-parser

Lexer, parser, and AST definitions.

```rust
// crates/colony-parser/src/lib.rs
pub mod lexer;      // Tokenization
pub mod token;      // Token types
pub mod ast;        // AST node definitions
pub mod parser;     // Recursive descent parser
pub mod span;       // Source spans
```

## 1.5 Crate: colony-exec

Execution engine, job control, and process management.

```rust
// crates/colony-exec/src/lib.rs
pub mod executor;   // AST evaluation
pub mod pipeline;   // Pipeline execution
pub mod process;    // Process spawning
pub mod jobs;       // Job control (process groups, signals)
pub mod builtins;   // Builtin commands (cd, export, etc.)
pub mod redirect;   // I/O redirection
pub mod expand;     // Word expansion (re-exports colony-expand)
```

## 1.6 Crate: colony-channels (Planned)

Extended I/O channels beyond stdin/stdout/stderr. **Not yet implemented.**

```rust
// crates/colony-channels/src/lib.rs (planned)
pub mod channel;    // Channel trait and registry
pub mod context;    // stdcontext (fd 3)
pub mod llm;        // stdllm (fd 4)
pub mod tool;       // stdtool (fd 5)
pub mod diagnostic; // stddiag (fd 6)
pub mod buffer;     // Buffer storage
```

> **Implementation Note:** Channel functionality is currently handled via
> OSC 2337 protocol (colony-protocol crate) which communicates with the
> terminal emulator rather than using additional file descriptors.

## 1.7 Crate: colony-agent

Agent definition, parsing, and execution. Includes Rust port of Claude SDK.

```rust
// crates/colony-agent/src/lib.rs
pub mod agent;      // Agent struct and lifecycle
pub mod sdk;        // Claude SDK (Rust port)
pub mod nl;         // Natural language detection
pub mod tools;      // Tool registry and classification
pub mod session;    // Multi-turn conversation state
```

> **Implementation Note:** This crate includes a native Rust implementation
> of the Claude Agent SDK, supporting both Anthropic API and AWS Bedrock
> backends with streaming responses and tool use.

## 1.8 Crate: colony-tuple

Linda-style tuple space for coordination.

```rust
// crates/colony-tuple/src/lib.rs
pub mod tuple;      // Tuple type (Value enum)
pub mod pattern;    // Pattern matching with wildcards
pub mod space;      // TupleSpace (scc::HashMap based)
pub mod ops;        // out, in, rd, inp, rdp primitives
```

> **Implementation Note:** Uses `scc::HashMap` for lock-free concurrent access
> (Rule #8). Cross-process IPC via `ipc-channel` is planned but not yet implemented.

## 1.9 Crate: colony-mcp

Model Context Protocol client.

```rust
// crates/colony-mcp/src/lib.rs
pub mod client;     // MCP client
pub mod transport;  // stdio, HTTP, SSE
pub mod tools;      // Tool discovery
pub mod resources;  // Resource management
pub mod server;     // Server definitions
```

## 1.10 Crate: colony-ui (Planned)

Terminal UI using GPUI. **Not yet implemented.**

```rust
// crates/colony-ui/src/lib.rs (planned)
pub mod terminal;   // Terminal emulation
pub mod buffer;     // Buffer rendering
pub mod pane;       // Pane management
pub mod preview;    // Live preview
pub mod help;       // Help system
pub mod input;      // Input handling
pub mod theme;      // Theming
```

> **Implementation Note:** Colony currently runs in existing terminal emulators.
> The GPUI-based terminal (colony-terminal) is a separate project that will
> receive OSC 2337 protocol messages from the shell.

## 1.11 Crate: colony-shell

Binary entry points and REPL.

```rust
// crates/colony-shell/src/main.rs

#[tokio::main]
async fn main() -> Result<()> {
    let mode = determine_mode();

    // Initialize Tree-sitter for syntax validation
    let ts_parser = colony_tree_sitter::ColonyParser::new();

    // Run REPL or execute script
    match mode {
        ShellMode::Posix => run_posix_shell().await,
        ShellMode::Modern => run_modern_shell().await,
    }
}

fn determine_mode() -> ShellMode {
    // 1. Check binary name
    let argv0 = std::env::args().next().unwrap_or_default();
    if argv0.ends_with("colony-sh") || argv0.ends_with("sh") {
        return ShellMode::Posix;
    }

    // 2. Check --posix flag
    if std::env::args().any(|a| a == "--posix") {
        return ShellMode::Posix;
    }

    // 3. Default to modern
    ShellMode::Modern
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShellMode {
    Posix,
    Modern,
}
```

> **Implementation Note:** The REPL integrates both Tree-sitter (for real-time
> syntax validation) and Pest (for execution). Natural language queries (# or ?)
> are routed to the agent system.

## 1.12 Additional Crates

### 1.12.1 colony-expand

POSIX word expansion (split from parser for modularity).

```rust
// crates/colony-expand/src/lib.rs
pub mod tilde;      // Tilde expansion (~, ~user)
pub mod parameter;  // Parameter expansion (${var}, ${var:-default})
pub mod command;    // Command substitution $(...)
pub mod arithmetic; // Arithmetic expansion $((...))
pub mod field;      // IFS field splitting
pub mod pathname;   // Glob expansion
```

### 1.12.2 colony-protocol

OSC 2337 terminal protocol for shell-terminal communication.

```rust
// crates/colony-protocol/src/lib.rs
pub mod osc;        // OSC escape sequence encoding
pub mod message;    // Protocol message types
pub mod emit;       // Terminal emission functions
```

### 1.12.3 colony-memory

Memory substrate for persistent context and agent memory.

```rust
// crates/colony-memory/src/lib.rs
pub mod store;      // SQLite schema and operations
pub mod vector;     // LanceDB vector embeddings
pub mod search;     // Hybrid text + semantic search
```

### 1.12.4 colony-tree-sitter

Tree-sitter parser for editor features (syntax highlighting, navigation).

```rust
// crates/colony-tree-sitter/src/lib.rs
pub fn language() -> tree_sitter::Language;
pub struct ColonyParser { /* ... */ }
```

> **Implementation Note:** Forked from tree-sitter-bash with Colony extensions
> (typed pipes, var/const declarations, type annotations).

### 1.12.5 colony-focus

Focus tracking for determining active context.

```rust
// crates/colony-focus/src/lib.rs
pub mod tracker;    // Window/project focus tracking
pub mod context;    // Context determination from focus
```

## 1.13 Dual-Binary Setup

The same codebase produces two binaries:

```toml
# crates/colony-shell/Cargo.toml
[[bin]]
name = "colony"
path = "src/main.rs"

[[bin]]
name = "colony-sh"
path = "src/main.rs"
```

Installation creates both, plus symlinks:

```bash
/usr/local/bin/colony      # Modern shell
/usr/local/bin/colony-sh   # POSIX shell
/usr/local/bin/csh         # Optional symlink to colony
```

## 1.14 Mode Detection Order

1. **Binary name** (`argv[0]`)
   - `colony-sh`, `sh` → POSIX mode
   - `colony`, `csh` → Modern mode

2. **Command-line flags**
   - `--posix` → POSIX mode
   - `--modern` → Modern mode

3. **Shebang analysis** (for scripts)
   - `#!/usr/bin/env colony-sh` → POSIX mode
   - `#!/usr/bin/env colony` → Modern mode

4. **File extension**
   - `.sh` → POSIX mode
   - `.csh`, `.colony` → Modern mode

5. **Shopt directives in script**

   ```colony
   shopt --set colony:upgrade  # Enable colony features
   shopt --set colony:all      # Enable all features
   ```

6. **Configuration file** (`~/.config/colony/config.toml`)
   ```toml
   [shell]
   default_mode = "modern"  # or "posix"
   ```

## 1.15 Dependencies

### 1.15.1 External Crates

```toml
# Workspace Cargo.toml
[workspace.dependencies]
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
toml = "0.8"
indexmap = { version = "2.0", features = ["serde"] }
thiserror = "1.0"
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.0", features = ["v4"] }
regex = "1.0"
glob = "0.3"
dirs = "5.0"
nix = { version = "0.27", features = ["process", "signal", "term"] }
libc = "0.2"
```

### 1.15.2 GPUI (for UI)

GPUI is vendored or included as a git dependency:

```toml
gpui = { git = "https://github.com/zed-industries/zed", package = "gpui" }
```

## 1.16 Configuration

### 1.16.1 Config File Location

```
~/.config/colony/config.toml       # User config
/etc/colony/config.toml            # System config
.colony/config.toml                # Project config (merged)
```

### 1.16.2 Config Schema

```toml
# ~/.config/colony/config.toml

[shell]
default_mode = "modern"
history_file = "~/.colony_history"
history_size = 10000

[options]
# Default shopt settings
colony_upgrade = true

[channels]
context_max_entries = 1000
llm_max_messages = 500
tool_max_records = 200

[buffers]
scrollback = 10000
auto_save = true
save_path = "~/.local/share/colony/buffers"

[panes]
default_layout = "default"

[agents]
default_model = "claude-sonnet-4-20250514"
max_parallel = 3

[mcp]
# Auto-connect servers
servers = []
```

---

_Continue to Part 2: Lexical Structure_
