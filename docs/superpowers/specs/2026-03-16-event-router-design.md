# Event Router: Persistent Tuple Event Automation

**Date:** 2026-03-16
**Status:** Draft
**Feature:** F081
**Repo:** colony-shell

## Overview

A persistent event routing daemon that subscribes to the colony-shell tuple space, matches events against a registry of listeners, and executes handlers when patterns match. Listeners are persistent (survive shell exits), manageable from any shell session, and execute handlers as independent processes. Integrates with the serve builtin for a web-based event dashboard.

This is the missing piece that closes the reactive loop: tuples are emitted everywhere (commands, heartbeats, HTTP requests, directory changes, agent actions) but until now only Rust code (panel binaries, services) could react to them. The `on` builtin makes tuple reactivity available to shell users and scripts.

## Design Decisions

1. **Daemon process, not session-bound** — The event router is a background daemon that persists across shell sessions. Any shell session can add/remove listeners. Handlers execute regardless of which shell (or whether any shell) is running.

2. **Persistent listener registry in SQLite** — Listeners stored in the shared reflection DB (`~/.colony/reflection/observations.db`) in an `event_listeners` table. Provides atomic operations and avoids concurrent-write race conditions that a TOML file would have. Human-readable export/import via `on --export` / `on --import` for editing.

3. **Handler isolation with limits** — Each handler execution spawns a new process (`colony-shell -c '<handler>'` or direct script execution). Handlers run in parallel, don't block the router, and can't affect each other. Exported functions (F070 `export -f`) are available in handler processes. Max 64 concurrent handlers, 30s handler timeout. If a handler is still running when the same listener fires again, the new invocation is skipped (prevents runaway spawning on high-frequency events).

4. **Serve integration via user init** — The event dashboard is available via `on --render` (outputs HTML to stdout). Users register it with the serve builtin in their `.colony/init`: `serve endpoint /_events 'on --render'`. The daemon does not directly register serve routes (it's a separate process).

5. **Tuple space prerequisite** — The current `TupleSpace::out()` does not notify `SubscriptionManager`. This spec requires wiring `SubscriptionManager::notify()` into `TupleSpace::out()` so subscriptions fire on writes. The daemon subscribes via the `PersistentTupleSpace` SQLite polling as a fallback until IPC subscriptions are fully wired. This is a prerequisite task.

## Builtin Interface

### Register Listeners

```bash
# Eval a shell command on event
on http_request 'echo "$2 $3 → $4"'

# Call a script
on heartbeat_complete ./scripts/notify.sh

# Call an exported function
track_command() { echo "$1: exit $2" >> ~/.colony/command.log; }
export -f track_command
on cmd_complete track_command

# One-shot (fires once, then auto-removes)
on --once deploy_complete 'notify-send "Deploy finished"'

# Conditional filter
on --filter '$4 >= 500' http_request alert_errors
on --filter '$1 == "alert"' heartbeat_complete escalate
```

### Manage Listeners

```bash
on --list                 # show all registered listeners (from any session)
on --remove <id>          # remove by UUID (or unique prefix, e.g., "a3f")
on --remove-all           # remove all listeners
on --enable <id>          # re-enable a disabled listener
on --disable <id>         # disable without removing
```

### Daemon Management

```bash
on --daemon start         # start event router (auto-starts on first `on` registration)
on --daemon stop          # stop event router
on --daemon status        # PID, uptime, listener count, events processed, handlers run
on --daemon restart       # stop + start
```

## Listener Registry

Stored in the shared reflection DB (`~/.colony/reflection/observations.db`):

```sql
CREATE TABLE IF NOT EXISTS event_listeners (
    id TEXT PRIMARY KEY,            -- UUID
    pattern TEXT NOT NULL,
    handler TEXT NOT NULL,
    handler_type TEXT NOT NULL,     -- 'eval', 'script', 'function'
    filter TEXT NOT NULL DEFAULT '',
    once INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    description TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);
```

UUIDs for all IDs (project convention). The `on` builtin writes directly to this table. The daemon polls for changes every 5 seconds (or on `SIGHUP`). Human-readable export/import:

```bash
on --export > listeners.toml    # dump for editing
on --import listeners.toml      # load from file
```

### Handler Types

| Type | How it executes | When to use |
|------|----------------|-------------|
| `eval` | `colony-shell -c '<handler>'` with tuple values as env | One-liners, quick reactions |
| `script` | Execute script directly with tuple values as args | Complex logic, any language |
| `function` | `colony-shell -c '<funcname> "$@"'` with exported function env | Reusable shell functions |

### Filters

Optional `--filter` applies a condition before executing the handler. The filter expression has access to positional tuple values (`$1`, `$2`, etc.) and supports:

- String equality: `$2 == "GET"`
- Numeric comparison: `$4 >= 500`
- String contains: `$3 ~ "/api/"`
- Boolean combinators: `AND`, `OR`, `NOT`
- Pattern match: `$2 == "POST" AND $4 >= 400`

Filters are evaluated in the daemon (Rust) before spawning a handler process, so non-matching events have zero process overhead.

## Event Router Daemon

### Architecture

```
┌─────────────────────────────────────────┐
│ colony-events daemon                    │
│                                         │
│  ┌──────────────┐   ┌───────────────┐  │
│  │ Tuple Space   │   │ Listener      │  │
│  │ Subscriber    │──▶│ Registry      │  │
│  │ (wildcard)    │   │ (from TOML)   │  │
│  └──────────────┘   └───────┬───────┘  │
│                              │          │
│         on match:            │          │
│  ┌───────────────────────────┘          │
│  │                                      │
│  ▼                                      │
│  ┌──────────────┐   ┌───────────────┐  │
│  │ Filter eval  │──▶│ Handler spawn │  │
│  │ (skip fast)  │   │ (parallel)    │  │
│  └──────────────┘   └───────┬───────┘  │
│                              │          │
│                     ┌────────┴────────┐ │
│                     │ Execution log   │ │
│                     │ (reflection DB) │ │
│                     └─────────────────┘ │
└─────────────────────────────────────────┘
```

### Implementation

The daemon is a new binary: `colony-events` (in `crates/colony-events/`).

**Startup:**
1. Open reflection DB, read `event_listeners` table
2. Connect to the persistent tuple space SQLite (`~/.colony/tuples/persistent.db`) — poll for new tuples since last seen timestamp
3. Write PID to `~/.colony/events/router.pid`
4. Start listener reload timer (every 5s, re-query `event_listeners` for changes)
5. Enter event loop

**Event Loop (polling-based, upgrade to subscription later):**
1. Poll persistent tuple space for tuples inserted since last check (100ms interval)
2. For each new tuple, extract tuple key (first element)
3. For each enabled listener whose pattern matches the key:
   a. Check handler concurrency (skip if at max 64, or if this listener's previous handler is still running)
   b. Evaluate filter (if any) — skip if false
   c. Spawn handler process with tuple values injected (30s timeout)
   d. Log execution start to reflection DB
4. For each completed handler:
   a. Log result (exit code, duration, stdout snippet) to reflection DB
   b. If `once = true`, remove listener from registry

**Future upgrade:** When `TupleSpace::out()` is wired to `SubscriptionManager::notify()`, the daemon can switch from polling to push-based subscription. The polling approach works today without any tuple space changes.

**Auto-start:** The first `on <pattern> <handler>` invocation checks if the daemon is running (via PID file). If not, starts it automatically. The user never needs to run `on --daemon start` manually.

**Graceful shutdown:** `on --daemon stop` sends SIGTERM. Daemon waits up to 5 seconds for in-flight handlers, then exits. Removes PID file.

### Tuple Value Injection

When a handler executes, ALL tuple elements (including the key) are available consistently as both environment variables and positional parameters:

```bash
# Tuple: ("http_request", "sess-1", "GET", "/api/status", "200", "12")

# Environment variables (always available):
$TUPLE_KEY=http_request
$TUPLE_0=http_request
$TUPLE_1=sess-1
$TUPLE_2=GET
$TUPLE_3=/api/status
$TUPLE_4=200
$TUPLE_5=12
$TUPLE_COUNT=6

# Positional params (same numbering as $TUPLE_N):
# $0=http_request $1=sess-1 $2=GET $3=/api/status $4=200 $5=12
```

**Filters use the same numbering as handlers.** `$2` in a filter means the same element as `$2` or `$TUPLE_2` in the handler. No off-by-one.

```bash
# Filter and handler use consistent numbering:
on --filter '$4 >= 500' http_request 'echo "ERROR: $2 $3 → $4"'
# $4 = "200" or "500" in both the filter and the handler
```

For `handler_type = "script"`, values are passed as command-line arguments in the same order:
```bash
on http_request ./log_request.sh
# Invoked as: ./log_request.sh http_request sess-1 GET /api/status 200 12
```

## Execution Logging

Handler executions logged in the shared reflection DB (`~/.colony/reflection/observations.db`):

```sql
CREATE TABLE IF NOT EXISTS event_handler_runs (
    id TEXT PRIMARY KEY,
    listener_id TEXT NOT NULL,
    pattern TEXT NOT NULL,
    tuple_values TEXT NOT NULL,    -- JSON array of tuple values
    handler TEXT NOT NULL,
    exit_code INTEGER,
    duration_ms INTEGER NOT NULL,
    stdout_snippet TEXT,           -- first 500 chars of stdout
    stderr_snippet TEXT,           -- first 500 chars of stderr
    timestamp INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_event_runs_timestamp ON event_handler_runs(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_event_runs_listener ON event_handler_runs(listener_id);
```

Keep 7 days of execution history. Prune on daemon startup and every hour.

## Serve Integration: Event Dashboard

When the event router daemon detects the serve builtin is running (via `serve_started` tuple or PID file check), it auto-registers a web dashboard:

```bash
# Auto-registered by the daemon
event_dashboard() { on --render; }
serve endpoint /_events event_dashboard --method GET
```

### Dashboard Page

Self-contained HTML, auto-refresh every 5 seconds. Three sections:

**Live Event Stream**
```
┌─ Events ──────────────────────────────────────┐
│ 14:32:05  http_request   GET /api/status 200  │
│ 14:32:03  cmd_complete   cargo test 0         │
│ 14:31:58  http_request   GET /app 200         │
│ 14:31:55  dir_change     /project → /project/src │
│ 14:30:00  heartbeat      PASS 4 findings      │
└───────────────────────────────────────────────┘
```
Shows the last 50 events across all tuple types. Color-coded by type.

**Listener Registry**
```
┌─ Listeners ───────────────────────────────────┐
│ #1  http_request     echo "$2 $3 → $4"   ✓   │
│ #2  heartbeat_complete  ./notify.sh       ✓   │
│ #3  cmd_complete     track_command        ✓   │
│ #4  deploy_complete  notify-send "..."    ○   │ (one-shot, pending)
└───────────────────────────────────────────────┘
```
Shows all registered listeners with ID, pattern, handler (truncated), enabled status.

**Handler Execution History**
```
┌─ Executions ──────────────────────────────────┐
│ 14:32:05  #1  http_request   exit:0  12ms    │
│ 14:32:03  #3  cmd_complete   exit:0   3ms    │
│ 14:31:58  #1  http_request   exit:0   8ms    │
│ 14:30:02  #2  heartbeat      exit:0  89ms    │
│ 14:30:01  #2  heartbeat      exit:1  45ms  ⚠ │
└───────────────────────────────────────────────┘
```
Shows recent handler executions with listener ID, event, exit code, duration. Red highlight for non-zero exits.

### `on --render`

Builtin subcommand that queries the reflection DB and outputs the dashboard HTML to stdout. Used as the serve endpoint handler. Same pattern as `heartbeat render`.

## Panel Binary

`colony-events-bin` with binary name `events-monitor`. Subscribes to ALL tuples (same as the daemon). Shows live event stream in the terminal panel.

**Display:**
```
Events  42/min  3 listeners
  http_request    GET /api/status 200    now
  cmd_complete    cargo test 0           2s ago
  heartbeat       PASS 4 findings        30m ago
  dir_change      /project/src           31m ago
```

Key bindings: `j/k` scroll, `p` pause/resume stream, `f` filter by pattern.

## Crate Structure

```
crates/colony-events/
├── src/
│   ├── main.rs       — daemon entry point, event loop, signal handling
│   ├── registry.rs   — SQLite listener registry read/write (shared reflection DB)
│   ├── filter.rs     — custom filter expression parser and evaluator (no shell eval — prevents injection)
│   ├── handler.rs    — handler process spawning, output capture
│   ├── storage.rs    — execution logging to reflection DB
│   └── dashboard.rs  — HTML dashboard rendering
└── Cargo.toml

crates/colony-events-bin/
├── src/main.rs       — panel binary, tuple subscription, rendering
└── Cargo.toml
```

The `on` builtin lives in `colony-exec` (like all builtins). It communicates with the daemon by:
1. Writing to `event_listeners` table in the shared reflection DB (add/remove listeners)
2. Checking/managing the daemon process via PID file
3. Starting the daemon if not running

No custom IPC protocol between the builtin and daemon — the SQLite DB IS the protocol. The daemon polls the table periodically (every 5s) for registry changes. SQLite handles concurrent access safely via WAL mode.

## Integration Points

| System | Integration |
|--------|------------|
| Tuple space | Daemon subscribes via Unix socket (wildcard) |
| Serve builtin | Auto-registers /_events dashboard |
| Heartbeat | heartbeat_complete/heartbeat_finding events routable |
| HTTP requests | http_request events from serve builtin |
| Shell commands | cmd_complete events from REPL |
| Directory changes | dir_change events from cd builtin |
| Special files | special_files_loaded events from F030 |
| Export functions | F070 enables function handlers in child processes |
| Reflection DB | Execution history stored for dashboard/audit |

## Out of Scope (Future)

- Event replay (re-process historical events against new listeners)
- Listener priority/ordering (currently all fire in parallel)
- Handler chaining (output of one handler feeds into another)
- Remote event routing (subscribe to events from other machines)
- Rate limiting per-listener (debounce/throttle)
- Event transformation (modify tuple before passing to handler)
- Web UI for adding/removing listeners (read-only dashboard for now)
- Listener groups/namespaces
