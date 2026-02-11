# Colony Shell

Colony Shell is a POSIX-compliant shell written in Rust, designed as the command interpreter for Colony — an AI-native terminal emulator. This shell is one half of a unified system (the other being colony-terminal) that together enable capabilities impossible in traditional terminal environments.

## Project Vision

Colony Shell is not just another shell — it's the semantic layer that understands commands, manages agent orchestration, owns the memory substrate, and communicates rich structured data to its paired terminal via custom protocols.

Key differentiators:
- **POSIX-compliant core** — runs existing scripts, respects muscle memory
- **Agent-first architecture** — built around Claude ADK integration, multi-agent coordination via tuple spaces
- **Memory ownership** — the shell owns AI memory (LanceDB + SQLite), not the LLM provider
- **Structured output** — commands can emit typed data (JSON, tables) that the terminal renders as widgets
- **MCP executor** — special session that receives tasks, generates TypeScript MCP scripts, executes them sandboxed

## Architecture Overview
```
colony-shell/
├── crates/
│   ├── colony-parser/       # Lexer, parser, AST (POSIX grammar)
│   ├── colony-expand/       # Tilde, parameter, command, arithmetic expansion
│   ├── colony-exec/         # Fork/exec, job control, builtins
│   ├── colony-protocol/     # OSC 2337 protocol for terminal communication
│   ├── colony-agent/        # Claude ADK integration, NL detection
│   ├── colony-memory/       # LanceDB vectors + SQLite metadata
│   ├── colony-tuple/        # Linda-style tuple space (scc + parking_lot)
│   ├── colony-mcp/          # MCP connection pool, TypeScript codegen, executor
│   ├── colony-focus/        # Goal trees, checkpoints, session continuity
│   └── colony-shell/        # Main binary, REPL, config loading
├── docs/
│   ├── ARCHITECTURE.md
│   ├── POSIX_COMPLIANCE.md
│   └── PROTOCOL.md
└── tests/
    ├── posix/               # POSIX compliance test suite
    └── integration/
```

## Core Technical Requirements

### 1. POSIX Shell Compliance

Use the `vte` pattern but for shell parsing — hand-written lexer with mode stack, recursive descent parser. Key requirements:

- **Tokenizer**: Handle quoting (single, double, escape, heredoc), operators, reserved words in command position only
- **Parser**: Build AST for simple commands, pipelines, lists (&&, ||, ;), compound commands (if/while/for/case/subshell/brace), functions
- **Expansion order**: tilde → parameter → command substitution → arithmetic → field splitting → pathname → quote removal
- **Execution**: fork/exec via `nix` crate, `pre_exec` hooks for process groups, signal handling with `signal-hook`
- **Job control**: Process groups, SIGCHLD reaping, fg/bg/jobs builtins, terminal control

### 2. Terminal Protocol (OSC 2337)

Communicate with colony-terminal via custom escape sequences:
```
ESC ] 2337 ; <command>=<params> ST
```

Categories:
- `Agent=` — Agent lifecycle, status, streaming thoughts
- `Widget=` — Request widget rendering (approval, progress, table, code)
- `Memory=` — Tuple space operations
- `Output=` — Structured output metadata (content type, schema)
- `Session=` — Session management, checkpoints

### 3. Tuple Space (Linda-style)

Inter-agent coordination via generative communication:
```rust
// Primitives
space.out(tuple).await;           // Insert
let t = space.take(pattern).await; // Remove matching (blocking)
let t = space.read(pattern).await; // Read without remove (blocking)
let t = space.try_take(pattern);   // Non-blocking take
```

Implementation: `scc::HashMap` for storage, `parking_lot` for synchronization, `ipc-channel` for cross-process.

### 4. Memory Substrate

Local-first AI memory:

- **LanceDB**: Vector embeddings for semantic search (384d via fastembed/all-MiniLM-L6-v2)
- **SQLite**: Structured metadata (scope, project, timestamps, importance)
- **Scopes**: Global → Project → Session → Task
- **Operations**: Store, retrieve, consolidate, forget

### 5. MCP Executor

Special agent session that:
1. Receives task messages from other agents
2. Introspects connected MCP servers for tool schemas
3. Generates TypeScript client classes from schemas (Tera templates)
4. Writes task-specific TypeScript scripts
5. Executes in sandboxed Deno runtime (rustyscript)
6. Returns results via tuple space
7. Indexes successful scripts to institutional memory (vector store)

### 6. Focus & Continuity

Prevent agent context drift:

- **Goal trees**: Hierarchical goals with status, progress, blockers
- **Checkpoints**: Snapshot state at meaningful moments
- **Session summaries**: LLM-generated recap for resume
- **Working memory**: Current task, decisions, notes
- `.colonyrc`: Project config with agent personality, conventions, goals

## Key Crates

| Purpose | Crate |
|---------|-------|
| Async runtime | `tokio` |
| Unix syscalls | `nix` |
| Signals | `signal-hook` |
| Concurrent collections | `scc`, `dashmap` |
| Synchronization | `parking_lot` |
| IPC | `ipc-channel` |
| Vector store | `lancedb` |
| Embeddings | `fastembed` |
| SQL | `rusqlite` |
| MCP client | `rmcp` |
| TS execution | `rustyscript` |
| Templates | `tera` |
| Config | `figment` |
| Serialization | `serde`, `rmp-serde` |
| CLI | `clap` |
| Errors | `miette`, `thiserror` |

## Current Status and Roadmap

### Implemented in the codebase
- Dual-parser stack (tree-sitter for UX + PEG for execution)
- POSIX execution pipeline (expansion → resolution → exec)
- Job control primitives and core builtins
- Tuple space (in-process, IPC, persistence tiers)
- Extended channels (fd 3-6) for structured streams
- OSC 2337 protocol emission for terminal integration
- Agent routing and multi-provider LLM integration
- Core language extensions: typed variables, structured data literals, and expression helpers (see `docs/design/FEATURES_AUDIT.md`)
- Namespaced builtins (`cmd:sub`) for builtin subcommands

### Planned (next major milestones)
- Memory substrate (LanceDB + SQLite) and focus/continuity
- MCP executor (schema introspection → TS codegen → sandboxed exec)
- Reactive prompt and widget workflows via colony-terminal
- Protocol stability guarantees and conformance tests

**Canonical roadmap**: `docs/design/FEATURES_AUDIT.md`  
**Implementation status**: `docs/IMPLEMENTATION_ARCHITECTURE.md`  
**Shell/terminal boundary**: `docs/design/RESPONSIBILITY_BOUNDARY.md`  
**Integration test plan**: `docs/design/INTEGRATION_TEST_PLAN.md`

## Code Style

- Use `thiserror` for library errors, `miette` for user-facing diagnostics
- Prefer `async` for I/O, but fork/exec is inherently sync (use `spawn_blocking`)
- Document public APIs with examples
- Test POSIX compliance against real scripts (bash, zsh comparison)
- Use feature flags to gate heavy dependencies (agent, mcp)

## Getting Started

If you're contributing, start with `docs/IMPLEMENTATION_ARCHITECTURE.md` to see
the current state, then align work with the canonical roadmap in
`docs/design/FEATURES_AUDIT.md`.

## Non-Goals (for colony-shell)

- Terminal rendering (that's colony-terminal)
- GPU/graphics (that's colony-terminal)
- TUI widgets (communicate via protocol, terminal renders)
- Full readline (we'll use reedline, but terminal handles display)

## Emitting Markdown Output (Protocol)

Colony Shell should emit structured output payloads and let colony-terminal render them.
In non-Colony terminals, fall back to plain text.

```rust
use colony_protocol::emit_output_payload;
use serde_json::json;

emit_output_payload("markdown", json!({
    "markdown": "# Title\\n\\nBody"
}));
```

## Namespaced Builtins

Builtins that take subcommands can be invoked as `cmd:sub`:

```bash
context:show --last 5
context:add "remember this"
```

## License

MIT OR Apache-2.0
