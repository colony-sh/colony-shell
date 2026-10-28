# Event Router Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Add a persistent event routing daemon with an `on` builtin for registering tuple event listeners that survive shell sessions.

**Architecture:** colony-events daemon binary polls persistent tuple space SQLite for new tuples, matches against listener registry (stored in shared reflection DB), spawns handler processes with tuple values injected. `on` builtin in colony-exec manages listeners via direct DB access.

**Tech Stack:** Rust, rusqlite, tokio, colony-tuple, colony-panel protocol

**Spec:** `docs/superpowers/specs/2026-03-16-event-router-design.md`

---

## Implementation Steps

### Phase A: Listener Registry and Filter Evaluator (colony-exec)

- [x] **A1.** Create `crates/colony-exec/src/events/` module directory with `mod.rs`
- [x] **A2.** Create `crates/colony-exec/src/events/registry.rs` — SQLite CRUD for `event_listeners` table
  - Opens shared reflection DB (`~/.colony/reflection/observations.db`)
  - `init_schema()` creates `event_listeners` and `event_handler_runs` tables with UUID TEXT PKs
  - `add_listener(pattern, handler, handler_type, filter, once, description) -> UUID`
  - `remove_listener(id_or_prefix) -> bool`
  - `remove_all() -> usize`
  - `enable_listener(id_or_prefix) -> bool`
  - `disable_listener(id_or_prefix) -> bool`
  - `list_listeners() -> Vec<EventListener>`
  - `export_listeners() -> String` (TOML format)
  - `import_listeners(toml_content) -> usize`
  - `log_handler_run(run: HandlerRun)`
  - `recent_runs(limit) -> Vec<HandlerRun>`
  - `prune_old_runs(days)`
  - In-memory test support via `open_in_memory()`
  - WAL mode on open (matches reflection DB pattern)
- [x] **A3.** Create `crates/colony-exec/src/events/filter.rs` — custom expression parser
  - Token types: `Var($N)`, `StringLiteral`, `NumberLiteral`, `Op(==, !=, >, >=, <, <=, ~)`, `And`, `Or`, `Not`, `LParen`, `RParen`
  - Lexer: tokenize filter string into tokens
  - Parser: recursive descent producing `FilterExpr` AST
  - Evaluator: `evaluate(expr, tuple_values: &[String]) -> bool`
  - `$0` = key, `$1` = first value, etc. (consistent with handler numbering)
  - String comparison by default; coerce to f64 for numeric ops
  - `~` operator: substring/contains check
  - AND/OR/NOT boolean combinators
  - NO shell eval -- pure Rust evaluation (prevents injection)
- [x] **A4.** Wire `events` module into `crates/colony-exec/src/lib.rs` — add `pub mod events;`
- [x] **A5.** `cargo check -p colony-exec` — verify compilation

### Phase B: `on` Builtin (colony-exec)

- [x] **B1.** Add `"on"` to `is_builtin_base()` match in `builtins.rs`
- [x] **B2.** Add `"on"` match arm in `execute_builtin()` dispatch — delegates to `events::builtin_on()`
- [x] **B3.** Implement `events::builtin_on()` function in `events/mod.rs`
  - Subcommand parsing:
    - `on <pattern> <handler>` — register listener (auto-detect handler_type: eval/script/function)
    - `on --list` — list all listeners with ID, pattern, handler, enabled status
    - `on --remove <id>` — remove by UUID or unique prefix
    - `on --remove-all` — remove all listeners
    - `on --enable <id>` / `on --disable <id>` — toggle listener
    - `on --once <pattern> <handler>` — one-shot listener
    - `on --filter '<expr>' <pattern> <handler>` — conditional listener
    - `on --daemon start|stop|status|restart` — daemon management
    - `on --render` — output HTML dashboard to stdout
    - `on --export` — dump listeners as TOML to stdout
    - `on --import <file>` — load listeners from TOML file
  - Auto-start daemon on first `on <pattern> <handler>` if not already running
  - PID file check at `~/.colony/events/router.pid`
  - Handler type detection: path ending in known extension or starting with `./ | /` = script; exported function name = function; otherwise = eval
- [x] **B4.** `cargo check -p colony-exec` — verify builtin wiring

### Phase C: Event Daemon Binary (colony-events)

- [x] **C1.** Create `crates/colony-events/Cargo.toml` — binary crate with deps: tokio, rusqlite, colony-tuple, colony-exec (for registry/filter types), uuid, chrono, serde, serde_json, tracing, tracing-subscriber, dirs, signal-hook
- [x] **C2.** Create `crates/colony-events/src/main.rs` — daemon entry point
  - Parse CLI args: `colony-events daemon` (default), `colony-events --foreground`
  - Startup:
    1. Open reflection DB, init `event_listeners` + `event_handler_runs` tables
    2. Open persistent tuple space SQLite (`~/.config/colony/tuple-space.db`)
    3. Record last-seen tuple timestamp
    4. Write PID file to `~/.colony/events/router.pid`
    5. Start listener reload timer (5s)
    6. Prune old handler runs (7-day retention)
    7. Enter event loop
  - Event loop:
    1. Poll persistent tuple space for tuples inserted since last check (100ms interval)
    2. For each new tuple, extract key (first field)
    3. For each enabled listener whose pattern matches the key:
       a. Check handler concurrency (max 64 global, skip if this listener's handler still running)
       b. Evaluate filter if present
       c. Spawn handler process with tuple values injected
       d. Log execution start
    4. Reap completed handlers, log results
    5. If `once = true` on completed listener, remove it
  - Signal handling: SIGTERM -> graceful shutdown (wait 5s for in-flight), remove PID file
  - SIGHUP -> immediate listener registry reload
- [x] **C3.** Create `crates/colony-events/src/handler.rs` — handler process spawning
  - `spawn_handler(listener, tuple_values) -> JoinHandle<HandlerResult>`
  - Environment injection: `TUPLE_KEY`, `TUPLE_0..N`, `TUPLE_COUNT`
  - For eval: `colony-shell -c '<handler>'` with env vars
  - For script: direct execution with args
  - For function: `colony-shell -c '<funcname> "$@"'` with exported function env
  - 30s timeout per handler
  - Capture stdout/stderr (first 500 chars each)
- [x] **C4.** Create `crates/colony-events/src/dashboard.rs` — HTML dashboard rendering
  - `render_dashboard(listeners, recent_events, recent_runs) -> String`
  - Three sections: Live Event Stream, Listener Registry, Handler Execution History
  - Self-contained HTML with inline CSS
  - Auto-refresh every 5 seconds
  - Color-coded by event type and exit code
- [x] **C5.** Add `colony-events` to workspace `members` in root `Cargo.toml`
- [x] **C6.** `cargo check -p colony-events` — verify daemon compiles

### Phase D: Panel Binary (colony-events-bin)

- [x] **D1.** Create `crates/colony-events-bin/Cargo.toml` — binary crate with `events-monitor` binary name
- [x] **D2.** Create `crates/colony-events-bin/src/main.rs` — panel binary
  - Subscribe to ALL tuples (wildcard subscription via panel protocol)
  - Show live event stream with timestamp, key, values
  - Track events/min rate
  - j/k scroll, p pause/resume, f filter
- [x] **D3.** Add `colony-events-bin` to workspace `members` in root `Cargo.toml`
- [x] **D4.** `cargo check -p colony-events-bin` — verify panel compiles

### Phase E: Integration and Testing

- [x] **E1.** `cargo check` — full workspace compilation
- [x] **E2.** `cargo test -p colony-exec` — events module tests
- [x] **E3.** `cargo test -p colony-events` — daemon unit tests
- [x] **E4.** `cargo clippy -p colony-exec -p colony-events -p colony-events-bin` — lint check

---

## File Inventory

### New files:
- `crates/colony-exec/src/events/mod.rs` — `on` builtin logic, module re-exports
- `crates/colony-exec/src/events/registry.rs` — SQLite listener registry CRUD
- `crates/colony-exec/src/events/filter.rs` — custom filter expression parser/evaluator
- `crates/colony-events/Cargo.toml` — daemon crate manifest
- `crates/colony-events/src/main.rs` — daemon entry point + event loop
- `crates/colony-events/src/handler.rs` — handler process spawning
- `crates/colony-events/src/dashboard.rs` — HTML dashboard rendering
- `crates/colony-events-bin/Cargo.toml` — panel crate manifest
- `crates/colony-events-bin/src/main.rs` — panel binary

### Modified files:
- `Cargo.toml` — add `colony-events` and `colony-events-bin` to workspace members
- `crates/colony-exec/src/lib.rs` — add `pub mod events;`
- `crates/colony-exec/src/builtins.rs` — add `"on"` to builtin dispatch

---

## Key Design Constraints

1. **UUIDs everywhere** — listener IDs, run IDs are TEXT PRIMARY KEY UUIDs
2. **Custom filter parser** — NO shell eval; pure Rust tokenizer/parser/evaluator
3. **SQLite as IPC** — `on` builtin writes to DB, daemon polls DB; no custom protocol
4. **Shared reflection DB** — same `~/.colony/reflection/observations.db` as heartbeat
5. **WAL mode** — enables concurrent reads from daemon + shell processes
6. **$TUPLE_N consistency** — $0 = key in both filters and handlers; no off-by-one
7. **Handler limits** — max 64 concurrent, 30s timeout, skip-if-busy per listener
8. **7-day retention** — prune `event_handler_runs` on daemon startup + hourly
