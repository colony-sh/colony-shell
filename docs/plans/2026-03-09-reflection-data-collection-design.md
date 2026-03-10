# Reflection Data Collection Layer — Design

## Goal

Start collecting all 5 observation types (commands, errors, agent chat, file activity, context usage) into a local SQLite database so the reflection engine has data to work with when pattern detection and insight generation are built.

## Architecture

New `colony-reflection` crate with three modules:

- **`observer.rs`** — `ReflectionObserver` trait + `SqliteObserver` implementation
- **`storage.rs`** — SQLite schema, connection management, write/query operations
- **`types.rs`** — Observation structs, session context, enums for all 5 categories

The `Executor` gets an `Arc<dyn ReflectionObserver>` field. Hook points in `colony-exec` call trait methods after command execution, on errors, after agent interactions, on file events, and on context usage.

## Storage

Single SQLite DB at `~/.colony/reflection/observations.db`. Two tables:

```sql
CREATE TABLE observations (
    id TEXT PRIMARY KEY,          -- UUID
    timestamp INTEGER NOT NULL,   -- unix millis
    category TEXT NOT NULL,       -- command|error|agent_chat|file_activity|context_usage
    data TEXT NOT NULL,           -- JSON blob (category-specific fields)
    session_id TEXT NOT NULL,
    cwd TEXT,
    git_branch TEXT,
    command_count INTEGER,        -- nth command in session
    time_since_last_ms INTEGER,   -- millis since previous observation
    time_of_day TEXT,             -- morning|afternoon|evening|night
    day_of_week TEXT,             -- monday..sunday
    project TEXT,                 -- derived from cwd
    focus_duration_ms INTEGER,    -- time in current session
    energy_level REAL             -- derived: error_rate + inter-command timing
);

CREATE INDEX idx_obs_category ON observations(category);
CREATE INDEX idx_obs_session ON observations(session_id);
CREATE INDEX idx_obs_timestamp ON observations(timestamp);
```

JSON `data` blobs per category:

- **command**: `{name, args, exit_code, duration_ms, pipe_position}`
- **error**: `{kind, message, recovery_action, command_context}`
- **agent_chat**: `{turns, topic, resolution, tokens_used, model}`
- **file_activity**: `{path, action, size_delta}`
- **context_usage**: `{sources, tokens, channel}`

## Observer Trait

```rust
#[async_trait]
pub trait ReflectionObserver: Send + Sync {
    async fn observe_command(&self, cmd: CommandObservation);
    async fn observe_error(&self, err: ErrorObservation);
    async fn observe_agent_chat(&self, chat: AgentChatObservation);
    async fn observe_file_activity(&self, file: FileActivityObservation);
    async fn observe_context_usage(&self, ctx: ContextUsageObservation);
}
```

## Hook Points

1. **Commands** — `colony-exec/src/command.rs:execute_command()` — wrap execution with timing, record name/args/exit_code/duration after completion
2. **Errors** — `colony-exec/src/error.rs` + every `Err(ExecutionError::*)` return site — record error kind, message, command that triggered it
3. **Agent chat** — `colony-agent` crate — after agent turn completes, record turns/topic/resolution/tokens
4. **File activity** — `colony-filetree` crate — on watch/edit/delete events
5. **Context usage** — `colony-channels` — when context channel (fd 3) is written to

## Session State

A `SessionTracker` struct lives in the observer, updated on each observation:

```rust
struct SessionTracker {
    session_id: String,
    start_time: Instant,
    command_count: u32,
    last_observation: Instant,
    recent_error_count: u32,
    recent_error_window: VecDeque<Instant>,
}
```

`energy_level` = inverse of (recent_error_rate + avg_inter_command_delay). Fast commands with few errors = high energy.

## Defaults

Collection is on by default. A future `reflect config disable` command can set a flag in `~/.colony/reflection/config.toml` that the observer checks.

## Out of Scope

- Pattern detection
- Insight generation
- The `reflect` builtin command
- Tuple space / `@reflect` channel integration
- LanceDB / embeddings

These all layer on top of the observation data this design collects.
