# Heartbeat Scheduler: Cron for Your AI Agent

**Date:** 2026-03-16
**Status:** Draft
**Feature:** F080
**Repo:** colony-shell

## Overview

A scheduled agent execution system driven by a user-authored HEARTBEAT.md checklist. The agent processes the checklist on a configurable interval (default 30 minutes), executes checks using shell tools, reports findings, and optionally updates the checklist. Integrates with the `serve` builtin to provide a web dashboard showing the current checklist, run history, and live findings.

Inspired by [OpenClaw's heartbeat](https://docs.openclaw.ai/gateway/heartbeat) and [TinMan](https://github.com/hesreallyhim/awesome-claude-code/issues/993).

## Design Decisions

1. **HEARTBEAT.md is a checklist, not a journal** — The user writes what to check. The agent executes the checks and reports findings. This is "cron for your AI agent," not a living document the agent maintains unprompted.

2. **Replaces F030's HeartbeatParser** — The F030 special files agent built a `HeartbeatParser` that treats HEARTBEAT.md as an agent-written status file (`## agent: <name>` headings with key-value metadata). This is incorrect per the OpenClaw/TinMan convention. This spec replaces that format. The existing `HeartbeatParser` and `HeartbeatWatcher` in `crates/colony-exec/src/special_files/heartbeat.rs` should be removed and replaced with the checklist parser described here. The `SpecialFileType::Heartbeat` variant in F030's `types.rs` stays — only the parser changes.

3. **Agent invocation on timer** — The `HeartbeatScheduler` runs on a background tokio task. Each tick creates a fresh `Agent` (no message history carry-over between ticks) with a custom `ToolRegistry` containing only mode-appropriate tools. The agent interprets natural language checklist items and uses tools to fulfill them.

4. **Skip when empty** — If HEARTBEAT.md contains only headers and whitespace, the scheduler skips the run to save API calls. No unnecessary LLM invocations.

5. **Three security modes** — `notify` (read-only, report findings), `active` (can modify HEARTBEAT.md and project files), `full` (full shell access). Default is `notify`.

6. **Serve integration** — When both the heartbeat scheduler and serve builtin are running, the heartbeat auto-registers a web dashboard route showing the current checklist, run history, and live findings as server-rendered HTML.

## HEARTBEAT.md Format

```markdown
---
interval: 30m
mode: notify
notify: true
---

# Heartbeat

- [ ] Check if any tests are failing (`cargo test 2>&1 | tail -5`)
- [ ] Scan for TODO/FIXME items added today (`git diff --name-only HEAD~5 | xargs grep -l TODO`)
- [ ] Verify the dev server is still running (`curl -s localhost:8080/health`)
- [ ] Check disk usage on build artifacts (`du -sh target/`)
- [ ] Note anything unusual in recent git history
```

### Frontmatter (optional YAML)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `interval` | duration | `30m` | Time between runs. Supports `5m`, `15m`, `30m`, `1h`, `4h`, `1d`. |
| `mode` | string | `notify` | Security mode: `notify`, `active`, `full` |
| `notify` | bool | `true` | Whether to emit notification tuples for findings |
| `timeout` | duration | `60s` | Max time per heartbeat run before abort |
| `max_tool_calls` | int | `20` | Max agent tool invocations per run |
| `token_budget` | int | `10000` | Max LLM tokens per run |

If no frontmatter, defaults apply. The checklist body is freeform Markdown — the agent interprets it as natural language instructions.

**Config hot-reload:** Frontmatter is re-read on each tick. If the user changes `interval: 30m` to `interval: 1h` in HEARTBEAT.md, the scheduler picks it up on the next tick and adjusts the timer. No restart needed.

### Checklist Semantics

- Items with backtick-wrapped commands are hints the agent should execute those commands
- Items without commands are open-ended — the agent decides how to check
- `- [ ]` (unchecked) items are active checks to run each tick
- `- [x]` (checked) items are skipped (resolved/acknowledged)
- The agent can check/uncheck items in `active` or `full` mode

## Builtin Interface

```bash
heartbeat start              # Start scheduler (reads config from HEARTBEAT.md frontmatter)
heartbeat stop               # Stop scheduler
heartbeat run                # Run once immediately
heartbeat status             # Last run time, next scheduled, active checklist item count
heartbeat interval 15m       # Override interval (does not modify HEARTBEAT.md)
heartbeat log                # Show recent run history (timestamp, duration, findings)
heartbeat log <run_id>       # Show full output for a specific run
```

### Lifecycle

- `heartbeat start` discovers HEARTBEAT.md via the F030 special files hierarchy (current dir → parents → ~/.colony/)
- Validates agent availability (checks for LLM provider via `ModelRouter::auto_detect()`) — prints error and exits if no provider configured
- Parses frontmatter for interval, mode, timeout, token_budget
- Starts a background tokio task that ticks on the interval
- If no HEARTBEAT.md found, prints error and exits
- `heartbeat stop` cancels the background task
- `heartbeat run` executes a single tick immediately (useful for testing your checklist)
- Shell exit gracefully stops the scheduler

## Execution Model

### Per-Tick Flow

```
Timer fires (or `heartbeat run`)
  │
  ├─ Read HEARTBEAT.md from discovered path
  │
  ├─ Parse frontmatter + checklist body
  │
  ├─ Skip if body is empty (only headers/whitespace)
  │
  ├─ Build agent context:
  │    ├─ System prompt: "You are a heartbeat monitor. Execute each checklist
  │    │   item, report findings concisely. Do not infer tasks from prior runs."
  │    ├─ HEARTBEAT.md content (the checklist)
  │    ├─ CONTEXT.md content (if loaded via F030, for project context)
  │    └─ Current directory, git branch, timestamp
  │
  ├─ Configure agent tools via custom ToolRegistry (NOT with_builtins()):
  │    ├─ notify: file_read, shell_exec (capture stdout only), search
  │    ├─ active: + file_write (project files + HEARTBEAT.md)
  │    └─ full: + unrestricted shell_exec
  │    (Tool names match colony-agent's existing tool registry names)
  │
  ├─ Create fresh Agent per tick (no history carry-over between runs):
  │    ├─ Agent::new() with custom ToolRegistry
  │    ├─ AgentConfig with token_budget: 10_000 and max_tool_calls: 20
  │    ├─ Timeout: 60s default (configurable via frontmatter `timeout: 120s`)
  │    └─ If no LLM provider available, log error and skip tick
  │
  ├─ Capture agent output as HeartbeatRun:
  │    ├─ run_id: UUID
  │    ├─ timestamp: DateTime
  │    ├─ duration_ms: u64
  │    ├─ findings: Vec<Finding>  (parsed from agent output)
  │    ├─ raw_output: String      (full agent response)
  │    └─ status: Pass | Warn | Alert
  │
  ├─ Store HeartbeatRun in reflection DB
  │
  ├─ Emit tuple: ("heartbeat_complete", checklist_path, run_id, status, findings_count, duration_ms)
  │
  └─ If notify=true and findings exist:
       Emit tuple: ("heartbeat_finding", checklist_path, run_id, severity, message)
```

### Finding Extraction

The agent's output is parsed for structured findings. The agent is instructed to format findings as:

```
PASS: Tests are passing (147/147)
WARN: 3 new TODO items found in src/main.rs
ALERT: Dev server is not responding on localhost:8080
PASS: Disk usage normal (2.1G target/)
```

Lines starting with `PASS:`, `WARN:`, or `ALERT:` are extracted as `Finding` structs. Everything else is captured as `raw_output` for the full log.

```rust
struct Finding {
    severity: Severity,  // Pass, Warn, Alert
    message: String,
}

enum HeartbeatStatus {
    Pass,         // All findings are PASS
    Warn,         // At least one WARN, no ALERT
    Alert,        // At least one ALERT
    Unstructured, // Agent produced output but no PASS/WARN/ALERT lines detected
    Timeout,      // Agent exceeded timeout
    Error,        // Agent invocation failed (no provider, API error)
}
```

**Fallback:** If no structured findings (`PASS:`/`WARN:`/`ALERT:` lines) are extracted from the agent's output, status is `Unstructured` and the full `raw_output` is preserved. The dashboard shows the raw output for manual review. This accounts for LLMs not always following formatting instructions.
```

### Security Modes

| Mode | Can read files | Can run commands | Can write files | Can modify HEARTBEAT.md |
|------|---------------|-----------------|----------------|----------------------|
| `notify` | Yes | Yes (capture stdout) | No | No |
| `active` | Yes | Yes | Project files only | Yes |
| `full` | Yes | Yes (unrestricted) | Yes | Yes |

In `notify` mode, shell commands execute but their side effects are limited to reading. The agent receives stdout/stderr but cannot write to the filesystem. This is enforced by restricting the tool set provided to the agent.

## Serve Integration

### Auto-Registration

When `heartbeat start` detects that the serve builtin is running, it auto-registers a dashboard route:

```
/_heartbeat → heartbeat_dashboard handler
```

If the serve builtin starts after the heartbeat, the heartbeat detects it via tuple subscription (`serve_started` event) and registers then.

The user can also manually register at a custom path:

```bash
serve endpoint /monitor heartbeat_dashboard
```

### Dashboard Handler

The dashboard uses a two-layer approach matching the serve builtin's function-handler model:

1. **`heartbeat render`** — A builtin subcommand (Rust) that queries the reflection DB, renders HTML, and writes it to stdout. This is the heavy lifting.
2. **`heartbeat_dashboard`** — A trivial shell function that calls `heartbeat render`. Registered as the serve endpoint handler.

```bash
# Auto-registered by heartbeat start:
heartbeat_dashboard() { heartbeat render; }
serve endpoint /_heartbeat heartbeat_dashboard --method GET
```

This keeps the serve integration clean (it's just a function) while the actual DB queries and HTML rendering happen in Rust via the `heartbeat render` subcommand.

### Dashboard Page

Single HTML page, no JavaScript framework. Auto-refreshes via `<meta http-equiv="refresh">` matching the heartbeat interval.

**Three sections:**

#### Current Checklist

Renders HEARTBEAT.md as HTML (Markdown → HTML conversion). Shows:
- Each checklist item with checkbox state
- Time until next scheduled run
- Current security mode
- Interval setting

#### Run History

Reverse-chronological table of heartbeat runs:

| Time | Duration | Status | Findings |
|------|----------|--------|----------|
| 14:30 | 8.2s | PASS | 0 warn, 0 alert |
| 14:00 | 12.1s | WARN | 1 warn, 0 alert |
| 13:30 | 9.8s | ALERT | 0 warn, 1 alert |

Each row is expandable (click/anchor) to show the full agent output for that run.

#### Live Findings

The most recent run's full output, rendered as HTML:
- `PASS:` items in green
- `WARN:` items in yellow
- `ALERT:` items in red
- Full raw agent output below in a `<pre>` block

### HTML Rendering

The dashboard handler generates HTML server-side using string templating (no external template engine). The HTML is self-contained with inline CSS. Minimal, functional design — no framework, no build step.

```rust
fn render_dashboard(checklist: &str, runs: &[HeartbeatRun]) -> String {
    format!(r#"<!DOCTYPE html>
<html><head>
<meta charset="utf-8">
<meta http-equiv="refresh" content="{interval_secs}">
<title>Colony Heartbeat</title>
<style>/* inline CSS */</style>
</head><body>
{checklist_html}
{history_html}
{findings_html}
</body></html>"#,
    ...)
}
```

## Storage

### Reflection DB

HeartbeatRun records stored in the shared `~/.colony/reflection/observations.db` (same DB as colony-reflection and colony-history). New table:

```sql
CREATE TABLE IF NOT EXISTS heartbeat_runs (
    id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    status TEXT NOT NULL,        -- 'pass', 'warn', 'alert'
    findings_json TEXT NOT NULL, -- JSON array of Finding structs
    raw_output TEXT NOT NULL,
    checklist_path TEXT NOT NULL,
    interval_secs INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_heartbeat_timestamp ON heartbeat_runs(timestamp DESC);
```

Keep the last 7 days of runs (configurable). Prune on each write.

**DB ownership:** The heartbeat module opens the shared DB directly via `rusqlite::Connection::open()` (same path as `ObservationStore::default_path()`). It creates its own table via `CREATE TABLE IF NOT EXISTS` — does not depend on colony-reflection's migration system. WAL mode is already set by whichever crate opens the DB first.

## Panel Binary

### `colony-heartbeat-bin`

Panel binary showing heartbeat status. Subscribes to `heartbeat_complete` tuples.

**Display:**

```
♥ Heartbeat :30m  Last: 14:30 (PASS)  Next: 15:00
  PASS  Tests passing (147/147)
  PASS  No new TODOs
  WARN  Dev server slow (890ms)
  PASS  Disk usage normal
```

- Status line: interval, last run time + status, next run time
- Most recent findings with severity colors
- Key bindings: `Enter` to trigger immediate run, `l` to show log

## Crate Structure

The heartbeat scheduler lives in `colony-exec` as a module alongside the builtin (avoids adding another crate to the 30+ workspace):

```
crates/colony-exec/src/
├── heartbeat/
│   ├── mod.rs          — HeartbeatScheduler: start/stop/run/status
│   ├── runner.rs       — Per-tick execution: context building, agent invocation, finding extraction
│   ├── storage.rs      — HeartbeatRun read/write to reflection DB
│   └── dashboard.rs    — HTML rendering for serve integration
```

New panel binary:

```
crates/colony-heartbeat-bin/
├── src/main.rs         — Panel protocol, tuple subscription, rendering
└── Cargo.toml
```

## Integration Points

| System | Integration |
|--------|------------|
| F030 (Special Files) | Discovers HEARTBEAT.md in directory hierarchy |
| colony-agent | Invoked per-tick with checklist context and configured tools |
| colony-reflection | Stores HeartbeatRun records in shared DB |
| serve builtin | Auto-registers /_heartbeat dashboard route |
| Tuple space | Emits heartbeat_complete and heartbeat_finding tuples |
| Panel system | colony-heartbeat-bin subscribes to heartbeat tuples |

## Out of Scope (Future)

- Multiple HEARTBEAT.md files for different concerns (one per project)
- Remote heartbeat execution (run checks on remote machines)
- Notification forwarding (Telegram, Slack, email) — handle via tuple subscribers
- Heartbeat chains (one heartbeat triggers another)
- Diff-based findings (compare current run to previous)
- Custom finding severity levels beyond PASS/WARN/ALERT
