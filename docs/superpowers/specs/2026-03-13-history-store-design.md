# colony-history: SQLite-Backed Shell History

**Date:** 2026-03-13
**Status:** Draft
**Crate:** `colony-history` (new)
**Repo:** colony-shell

## Overview

Replace colony-shell's flat-file history with a SQLite-backed store that captures structured metadata per command — exit codes, duration, executor origin, agent context, risk levels, and privacy-aware redaction. Provides instant fuzzy search via Ctrl+R, an upgraded history panel with browse/stats modes, and enhanced `history` builtin subcommands.

Inspired by [Suvadu](https://github.com/AppachiTech/suvadu), but built colony-native: leveraging the tuple space for reactivity, `colony-secure` for precise secret redaction, and the agent execution pipeline for intent-aware tracking rather than after-the-fact heuristics.

## Design Decisions

1. **Unified DB** — `colony-history` owns its tables in the shared `~/.colony/reflection/observations.db` alongside `colony-reflection`. Each crate owns its own tables; they can JOIN when needed for correlation. SQLite WAL mode handles concurrent access.

2. **Dual search surface** — Inline Ctrl+R for instant recall (in-process SQLite FTS5), upgraded history panel binary for browsing, bookmarks, and stats. Different tools for different jobs.

3. **Native agent tracking** — Commands from `colony-agent` are tagged at execution time with session ID, goal, and risk level. External agents (Claude Code, Codex, Cursor, etc.) detected via environment variables. Intent-aware risk scoring, not just command pattern matching.

4. **Privacy via colony-secure** — Secret redaction uses exact credential matching from `colony-secure` keychain integration, not regex heuristics. When `colony-secure` injects a credential, the pipeline knows which values to redact with zero false positives.

5. **Core dashboard in v1** — `history stats` subcommand and panel stats mode with command frequency, directory patterns, agent breakdown, and time distribution. No heatmaps or sparklines — those come later on top of data already being collected.

## Data Model

### `command_history` table

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT (UUID) | Primary key |
| `command` | TEXT | Full command text |
| `command_redacted` | TEXT | Secrets-redacted version for display/export |
| `cwd` | TEXT | Working directory at execution time |
| `exit_code` | INTEGER | Process exit status |
| `duration_ms` | INTEGER | Wall-clock execution time |
| `executor` | TEXT | `user`, `colony-agent`, or detected external agent name |
| `agent_session_id` | TEXT NULL | Links to agent session if executor != user |
| `agent_goal` | TEXT NULL | What the agent was trying to accomplish |
| `risk_level` | TEXT NULL | `critical`, `high`, `medium`, `low`, `safe` |
| `session_id` | TEXT | Shell session UUID |
| `git_branch` | TEXT NULL | Branch at execution time |
| `hostname` | TEXT | Machine hostname |
| `timestamp` | INTEGER | Unix epoch millis |
| `hidden` | INTEGER | Reserved for future use (e.g., soft-hide from default queries without full skip) |
| `bookmarked` | INTEGER | 0/1 |
| `note` | TEXT NULL | User-added annotation |
| `tags` | TEXT NULL | Comma-separated tags |

### Indices

- `idx_history_timestamp` on `(timestamp DESC)` — default ordering
- `idx_history_command` on `(command)` — dedup and exact match
- `idx_history_cwd` on `(cwd)` — directory-scoped search
- `idx_history_executor` on `(executor)` — agent filtering
- `idx_history_session` on `(session_id)` — session scoping
- FTS5 virtual table `command_history_fts` for full-text search (see below)

### FTS5 Virtual Table

```sql
CREATE VIRTUAL TABLE command_history_fts USING fts5(
    command,
    content=command_history,
    content_rowid=rowid
);
```

This is an external-content FTS5 table — it indexes `command` text but stores no content itself, referencing back to `command_history` via rowid. Triggers on INSERT/DELETE keep it in sync. Queries join the FTS table with the content table:

```sql
SELECT h.command, h.cwd, h.executor, h.timestamp
FROM command_history_fts f
JOIN command_history h ON h.rowid = f.rowid
WHERE command_history_fts MATCH ?
  AND h.hidden = 0
ORDER BY h.timestamp DESC
LIMIT 50
```

**Note:** FTS5 is available via the `bundled` feature of `rusqlite` (already enabled in the workspace). If the project ever switches to system SQLite, FTS5 availability must be verified.

### Migration Strategy

The crate owns a `migrations/` directory with numbered SQL files applied idempotently (`CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`). Each crate tracks its own migration state via a `_colony_history_migrations` table. On first open, `colony-history` creates its tables. `colony-reflection`'s existing tables are untouched — they coexist in the same file.

## Recording Pipeline

After `execute_command()` completes in `colony-exec`, before returning the exit status:

```
Command completes
  → Build HistoryRecord struct
  → Privacy filter (space-prefix, regex exclusions, colony-secure redaction)
  → Tag executor origin (user / colony-agent / external detection)
  → If colony-agent: attach agent_session_id, agent_goal, compute risk_level
  → Write to SQLite (async, non-blocking via bounded channel)
  → Emit tuple ("cmd_complete", session_id, command, exit_code, duration_ms, executor)
  → Append to in-memory VecDeque (for bash-compatible !!/!N expansion)
```

### Async Writes

SQLite inserts happen on a background thread via a bounded channel (capacity: 256). Command execution never blocks on DB I/O. If the channel is full (unlikely), the record is dropped and a `tracing::warn!` is emitted. A `history_drops` counter is tracked and surfaced in `history stats` output. The in-memory VecDeque is the synchronous fast path.

### Privacy Filtering

- **Space-prefix:** Commands starting with a space are skipped entirely — no record is written to the DB. This matches bash `HISTCONTROL=ignorespace` behavior and the existing `History::add()` logic in `colony-exec/src/history.rs`.
- **Regex exclusions:** Configurable patterns in `~/.colony/config.toml` under `[history.exclude]`. Matched commands are also skipped entirely — no record written.
- **Secret redaction:** For commands that ARE recorded, `colony-secure` redacts secrets. The `SecureRegistry` gains a new method `fn redact(&self, input: &str) -> String` that replaces any substring matching a known credential value with `***`. This keeps secret values encapsulated inside `colony-secure` rather than leaking them as `Vec<String>`. The raw `command` is stored for exact replay; `command_redacted` is the redacted version for display and export.
- **Contextual redaction:** When `colony-secure` injects a credential into a command's environment, the recording pipeline already knows which values are sensitive. Zero false positives.

### Executor Tagging

- **User:** Default. No special detection needed.
- **colony-agent:** The agent execution pipeline in `colony-exec` knows when a command originates from `colony-agent`. Tagged at execution time with `agent_session_id` and `agent_goal`.
- **External agents:** Best-effort detection via environment variables and parent process name. The detection list is configurable in `~/.colony/config.toml` under `[history.agent_detection]` so users can add/update patterns without code changes. Default patterns include common env vars (`$CLAUDE_CODE_ENTRYPOINT`, `$CODEX`, `$CURSOR_SESSION`) but these are heuristics, not stable APIs.

### Risk Scoring

For agent-originated commands, a rules engine scores risk:

- **Critical:** `rm -rf`, `sudo`, `chmod 777`, `DROP TABLE`, writes to `/etc`
- **High:** `git push --force`, `docker rm`, package install/uninstall
- **Medium:** File writes, git commits, network requests
- **Low:** File reads, git status/diff/log
- **Safe:** echo, pwd, ls, cat, history

In v1, risk scoring is static rule-based only (command pattern matching). Context-aware adjustment based on `agent_goal` is deferred — the `agent_goal` field is captured for future use but does not affect scoring yet.

## Search

### Inline Ctrl+R (In-Process, Fast Path)

The existing `InteractiveHistorySearch` struct is refactored to query SQLite:

1. User presses Ctrl+R → enter search mode
2. Each keystroke fires FTS5 query against `command_history_fts` joined with `command_history` (see Data Model for exact query)
3. Results rendered inline below prompt — command text, dimmed cwd and relative timestamp, executor icon (⚡ agent, › user)
4. Arrow keys navigate, Enter selects, Esc cancels
5. Ctrl+D toggles directory-scoped filter (current cwd only)
6. Ctrl+E toggles dedup (collapse identical commands, show count)

The VecDeque remains for `!!`/`!N` expansion and up-arrow recall. These are positional lookups that don't need the DB.

### `history` Builtin Enhancements

Existing flags unchanged (`-c`, `-d`, `-a`, `-r`, `-w`, `-n`). The existing `-d <index>` flag remains for single-entry deletion by position (bash compat); the new `history delete` subcommand is for bulk operations by date or pattern. New subcommands:

| Subcommand | Description |
|---|---|
| `history search <query>` | Non-interactive FTS search, prints results |
| `history stats` | Command frequency, directory patterns, agent breakdown, time distribution |
| `history bookmark [id]` | Toggle bookmark on a command (default: last command) |
| `history note <id> <text>` | Add annotation to a command |
| `history tag <id> <tag>` | Tag a command |
| `history agents` | Agent command summary — counts, risk breakdown per executor |
| `history delete --before <date>` | Bulk delete by date |
| `history delete --pattern <regex>` | Bulk delete by pattern |
| `history export [--format json\|csv\|jsonl]` | Export with redacted commands |

### `history stats` Output

```
Commands: 14,238 (since 2025-11-03)
Today: 87  │  This week: 312  │  This month: 1,204

By executor:
  user          11,891 (83.5%)
  colony-agent   1,847 (13.0%)
  claude-code      500  (3.5%)

Top directories:
  ~/src/colony-sh/colony-shell    3,201
  ~/src/colony-sh/colony-nerve    2,118
  ~/src/colony-sh/nerve-tools       844

Peak hours: 09-11, 14-16
```

## History Panel Binary

The existing `colony-history-bin` is upgraded with two modes toggled via Tab:

### Browse Mode (Default)

- Reads from SQLite on launch, subscribes to `cmd_complete` tuples for live updates
- Each row: command, exit code (red if non-zero), duration, executor badge, relative timestamp
- `/` to search (FTS5 query, same as Ctrl+R but richer display)
- `b` toggle bookmark, `n` add note, `t` add tag
- `a` filter agent-only commands, `d` filter current directory
- Enter copies command to clipboard or injects into prompt

### Stats Mode

- Command frequency bar chart (top 10 commands)
- Hourly distribution histogram
- Agent breakdown with risk level counts
- Per-directory command counts
- Rendered with ratatui widgets

Panel protocol unchanged — same stdin/stdout JSON framing as all other colony panel binaries.

## Crate Structure

```
crates/colony-history/
├── src/
│   ├── lib.rs           — Public API: HistoryStore, HistoryRecord, SearchQuery
│   ├── store.rs         — SQLite operations, FTS5, migrations
│   ├── record.rs        — HistoryRecord struct, builder
│   ├── privacy.rs       — Redaction, exclusion patterns, colony-secure integration
│   ├── executor.rs      — Executor detection, agent tagging
│   ├── risk.rs          — Risk scoring rules engine
│   ├── search.rs        — Search query builder, result types
│   ├── stats.rs         — Aggregation queries for stats subcommand
│   └── migrations/
│       └── 001_create_command_history.sql
└── Cargo.toml
```

## Integration Points

| Crate | Change |
|---|---|
| `colony-exec` | After command execution, build `HistoryRecord` and send to `HistoryStore` via channel. Extend `execute_builtin()` with new `history` subcommands. |
| `colony-exec/history.rs` | Keeps VecDeque for `!!`/`!N` expansion and up-arrow. No longer source of truth — just a session cache. Remove `save_to_file`/`load_from_file` as primary path (keep for flat-file compat write). |
| `colony-reflection` | No changes. Continues recording observations in its own tables. Can JOIN against `command_history` when needed. |
| `colony-secure` | Add `fn redact(&self, input: &str) -> String` to `SecureRegistry`. Replaces substrings matching stored credential values with `***`. Keeps secrets encapsulated — no raw values leave the crate. |
| `colony-history-bin` | Rewrite rendering to read from `HistoryStore`. Add browse/stats mode toggle. |
| `colony-shell` (REPL) | Refactor `InteractiveHistorySearch` to query `HistoryStore` for Ctrl+R. `HistoryStore` is shared as `Arc<HistoryStore>` between the recording pipeline and the search path. Ctrl+R queries use `spawn_blocking` since SQLite is synchronous. |
| `colony-tuple` | No changes — `cmd_complete` tuple gets an extra `executor` field (additive). |

## Concurrency and Storage

### WAL Mode

`colony-history` enables `PRAGMA journal_mode=WAL` on connection open. This allows concurrent reads from the panel binary while the shell process writes. `colony-reflection` should also be updated to enable WAL since they share the DB file — whichever crate opens first sets the journal mode, and WAL is sticky per-database.

### Ownership Model

`HistoryStore` wraps a `Mutex<rusqlite::Connection>` internally and is shared as `Arc<HistoryStore>` between:
- The recording pipeline (write path, background thread)
- The Ctrl+R search (read path, via `spawn_blocking`)
- The `history` builtin subcommands (read path, synchronous)

The panel binary opens its own read-only connection to the same DB file.

### Security

The DB file at `~/.colony/reflection/observations.db` contains raw command text including potential secrets in the `command` column. Protections:
- File permissions set to `0o600` (owner-only) on creation.
- The `command_redacted` column is the safe version for display, export, and search results.
- Raw `command` storage enables exact replay at the cost of secrets on disk. Encryption-at-rest is out of scope for v1.

## Backward Compatibility

- **VecDeque stays** — `!!`, `!N`, `!-N`, `!prefix` expansion and up-arrow recall work from the in-memory cache as before.
- **Flat file written** — `~/.colony_history` continues to be appended for bash compatibility. Write-only; SQLite DB is source of truth.
- **Tuple protocol** — Extra `executor` field on `cmd_complete` is additive. Existing subscribers ignore it.
- **Existing builtins** — All current `history` flags (`-c`, `-d`, `-a`, `-r`, `-w`, `-n`) unchanged.

## Out of Scope (Future)

- Activity heatmaps and sparklines
- Session timeline visualization
- Auto-tagging based on working directory patterns
- Cross-machine sync
- Import from other shell history tools (atuin, mcfly, etc.)
- Integration with `colony-memory` vector embeddings for semantic command search
