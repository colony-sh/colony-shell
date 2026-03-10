# Reflection Data Collection Layer — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Collect all 5 observation types (commands, errors, agent chat, file activity, context usage) into a local SQLite database with full session context.

**Architecture:** New `colony-reflection` crate with `ReflectionObserver` trait, `SqliteObserver` implementation, and typed observation structs. The `Executor` in `colony-exec` gets an `Arc<dyn ReflectionObserver>` field. Hook points in 5 crates emit observations through the trait.

**Tech Stack:** Rust, rusqlite (SQLite), serde_json, chrono, uuid, tokio, async-trait

---

### Task 1: Create colony-reflection crate with types

**Files:**
- Create: `crates/colony-reflection/Cargo.toml`
- Create: `crates/colony-reflection/src/lib.rs`
- Create: `crates/colony-reflection/src/types.rs`
- Modify: `Cargo.toml` (workspace root, lines 3-21 members list, and workspace.dependencies)

**Step 1: Create the crate directory**

```bash
mkdir -p crates/colony-reflection/src
```

**Step 2: Write Cargo.toml**

Create `crates/colony-reflection/Cargo.toml`:

```toml
[package]
name = "colony-reflection"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
async-trait = "0.1"
rusqlite = { version = "0.31", features = ["bundled"] }
dirs.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
tempfile = "3"
```

**Step 3: Write types.rs**

Create `crates/colony-reflection/src/types.rs`:

```rust
//! Observation types for the reflection data collection layer

use chrono::{Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Instant;

/// Observation category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationCategory {
    Command,
    Error,
    AgentChat,
    FileActivity,
    ContextUsage,
}

impl ObservationCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Error => "error",
            Self::AgentChat => "agent_chat",
            Self::FileActivity => "file_activity",
            Self::ContextUsage => "context_usage",
        }
    }
}

/// Time of day bucket
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Morning,    // 6-12
    Afternoon,  // 12-17
    Evening,    // 17-21
    Night,      // 21-6
}

impl TimeOfDay {
    pub fn now() -> Self {
        let hour = Local::now().hour();
        match hour {
            6..=11 => Self::Morning,
            12..=16 => Self::Afternoon,
            17..=20 => Self::Evening,
            _ => Self::Night,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Morning => "morning",
            Self::Afternoon => "afternoon",
            Self::Evening => "evening",
            Self::Night => "night",
        }
    }
}

/// Day of week
fn day_of_week_str() -> &'static str {
    match Local::now().weekday() {
        chrono::Weekday::Mon => "monday",
        chrono::Weekday::Tue => "tuesday",
        chrono::Weekday::Wed => "wednesday",
        chrono::Weekday::Thu => "thursday",
        chrono::Weekday::Fri => "friday",
        chrono::Weekday::Sat => "saturday",
        chrono::Weekday::Sun => "sunday",
    }
}

/// Session context attached to every observation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub session_id: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub command_count: u32,
    pub time_since_last_ms: Option<i64>,
    pub time_of_day: String,
    pub day_of_week: String,
    pub project: Option<String>,
    pub focus_duration_ms: i64,
    pub energy_level: f64,
}

/// Tracks session state for deriving context
pub struct SessionTracker {
    pub session_id: String,
    start_time: Instant,
    command_count: u32,
    last_observation: Option<Instant>,
    error_window: VecDeque<Instant>,
    inter_command_times: VecDeque<u128>,
}

const ERROR_WINDOW_SIZE: usize = 20;
const TIMING_WINDOW_SIZE: usize = 20;

impl SessionTracker {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            start_time: Instant::now(),
            command_count: 0,
            last_observation: None,
            error_window: VecDeque::with_capacity(ERROR_WINDOW_SIZE),
            inter_command_times: VecDeque::with_capacity(TIMING_WINDOW_SIZE),
        }
    }

    /// Build session context for the current observation
    pub fn build_context(&mut self) -> SessionContext {
        let now = Instant::now();
        let time_since_last = self.last_observation.map(|t| now.duration_since(t).as_millis() as i64);

        if let Some(ms) = time_since_last {
            if self.inter_command_times.len() >= TIMING_WINDOW_SIZE {
                self.inter_command_times.pop_front();
            }
            self.inter_command_times.push_back(ms as u128);
        }

        self.command_count += 1;
        self.last_observation = Some(now);

        let focus_duration_ms = now.duration_since(self.start_time).as_millis() as i64;

        SessionContext {
            session_id: self.session_id.clone(),
            cwd: std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()),
            git_branch: detect_git_branch(),
            command_count: self.command_count,
            time_since_last_ms: time_since_last,
            time_of_day: TimeOfDay::now().as_str().to_string(),
            day_of_week: day_of_week_str().to_string(),
            project: detect_project_name(),
            focus_duration_ms,
            energy_level: self.compute_energy(),
        }
    }

    /// Record an error for energy level tracking
    pub fn record_error(&mut self) {
        let now = Instant::now();
        if self.error_window.len() >= ERROR_WINDOW_SIZE {
            self.error_window.pop_front();
        }
        self.error_window.push_back(now);
    }

    /// Energy = 1.0 - (error_rate + slowness). Clamped to [0.0, 1.0].
    fn compute_energy(&self) -> f64 {
        // Error rate: errors in window / window size
        let error_rate = self.error_window.len() as f64 / ERROR_WINDOW_SIZE as f64;

        // Slowness: avg inter-command time normalized (>30s = slow)
        let slowness = if self.inter_command_times.is_empty() {
            0.0
        } else {
            let avg_ms = self.inter_command_times.iter().sum::<u128>() as f64
                / self.inter_command_times.len() as f64;
            (avg_ms / 30_000.0).min(1.0)
        };

        (1.0 - (error_rate * 0.5 + slowness * 0.5)).clamp(0.0, 1.0)
    }
}

// -- Category-specific observation data --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandData {
    pub name: String,
    pub args: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: i64,
    pub pipe_position: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    pub kind: String,
    pub message: String,
    pub recovery_action: Option<String>,
    pub command_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentChatData {
    pub turns: u32,
    pub topic: Option<String>,
    pub resolution: String,
    pub tokens_used: Option<u64>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileActivityData {
    pub path: String,
    pub action: String,
    pub size_delta: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUsageData {
    pub sources: Vec<String>,
    pub tokens: Option<u64>,
    pub channel: String,
}

/// A complete observation ready for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub timestamp: i64,
    pub category: ObservationCategory,
    pub data: serde_json::Value,
    pub context: SessionContext,
}

impl Observation {
    pub fn new(category: ObservationCategory, data: serde_json::Value, context: SessionContext) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            category,
            data,
            context,
        }
    }
}

// -- Helpers --

fn detect_git_branch() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn detect_project_name() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    cwd.file_name().map(|n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_of_day() {
        // Just verify it doesn't panic
        let _ = TimeOfDay::now();
    }

    #[test]
    fn test_session_tracker_builds_context() {
        let mut tracker = SessionTracker::new("test-session".to_string());
        let ctx = tracker.build_context();
        assert_eq!(ctx.session_id, "test-session");
        assert_eq!(ctx.command_count, 1);
        assert!(ctx.energy_level >= 0.0 && ctx.energy_level <= 1.0);
    }

    #[test]
    fn test_session_tracker_energy_degrades_with_errors() {
        let mut tracker = SessionTracker::new("test".to_string());
        let ctx1 = tracker.build_context();
        let initial_energy = ctx1.energy_level;

        for _ in 0..10 {
            tracker.record_error();
        }

        let ctx2 = tracker.build_context();
        assert!(ctx2.energy_level < initial_energy);
    }

    #[test]
    fn test_observation_new() {
        let ctx = SessionContext {
            session_id: "s1".to_string(),
            cwd: Some("/tmp".to_string()),
            git_branch: None,
            command_count: 1,
            time_since_last_ms: None,
            time_of_day: "morning".to_string(),
            day_of_week: "monday".to_string(),
            project: Some("test".to_string()),
            focus_duration_ms: 1000,
            energy_level: 0.9,
        };

        let data = serde_json::to_value(CommandData {
            name: "ls".to_string(),
            args: vec!["-la".to_string()],
            exit_code: 0,
            duration_ms: 50,
            pipe_position: None,
        })
        .unwrap();

        let obs = Observation::new(ObservationCategory::Command, data, ctx);
        assert!(!obs.id.is_empty());
        assert_eq!(obs.category, ObservationCategory::Command);
    }
}
```

**Step 4: Write lib.rs**

Create `crates/colony-reflection/src/lib.rs`:

```rust
//! Reflection data collection for Colony Shell
//!
//! Collects observations about shell usage: commands, errors, agent chats,
//! file activity, and context usage. Stores in local SQLite for later
//! pattern detection and insight generation.

mod types;

pub use types::{
    AgentChatData, CommandData, ContextUsageData, ErrorData, FileActivityData,
    Observation, ObservationCategory, SessionContext, SessionTracker,
};
```

**Step 5: Add to workspace**

In root `Cargo.toml`, add `"crates/colony-reflection"` to workspace members (after line 17, before `"crates/colony-shell"`), and add `colony-reflection = { path = "crates/colony-reflection" }` to workspace.dependencies (after line 97).

**Step 6: Verify it compiles**

```bash
cd /Users/mcfearsome/src/github.com/colony-sh/colony-shell && cargo check -p colony-reflection
```

Expected: compiles with no errors.

**Step 7: Run tests**

```bash
cargo test -p colony-reflection
```

Expected: 4 tests pass.

**Step 8: Commit**

```bash
git add crates/colony-reflection/ Cargo.toml
git commit -m "feat(reflection): add colony-reflection crate with observation types"
```

---

### Task 2: SQLite storage layer

**Files:**
- Create: `crates/colony-reflection/src/storage.rs`
- Modify: `crates/colony-reflection/src/lib.rs`

**Step 1: Write storage.rs with tests**

Create `crates/colony-reflection/src/storage.rs`:

```rust
//! SQLite storage for observations

use crate::{Observation, ObservationCategory};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type StorageResult<T> = Result<T, StorageError>;

/// SQLite-backed observation store
pub struct ObservationStore {
    conn: Mutex<Connection>,
    db_path: PathBuf,
}

impl ObservationStore {
    /// Open or create the observation database
    pub fn open(db_path: &Path) -> StorageResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        let store = Self {
            conn: Mutex::new(conn),
            db_path: db_path.to_path_buf(),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Open an in-memory database (for testing)
    pub fn open_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
            db_path: PathBuf::from(":memory:"),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Default path: ~/.colony/reflection/observations.db
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".colony")
            .join("reflection")
            .join("observations.db")
    }

    fn init_schema(&self) -> StorageResult<()> {
        let conn = self.conn.lock().expect("lock poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS observations (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                category TEXT NOT NULL,
                data TEXT NOT NULL,
                session_id TEXT NOT NULL,
                cwd TEXT,
                git_branch TEXT,
                command_count INTEGER,
                time_since_last_ms INTEGER,
                time_of_day TEXT,
                day_of_week TEXT,
                project TEXT,
                focus_duration_ms INTEGER,
                energy_level REAL
            );
            CREATE INDEX IF NOT EXISTS idx_obs_category ON observations(category);
            CREATE INDEX IF NOT EXISTS idx_obs_session ON observations(session_id);
            CREATE INDEX IF NOT EXISTS idx_obs_timestamp ON observations(timestamp);",
        )?;
        Ok(())
    }

    /// Insert an observation
    pub fn insert(&self, obs: &Observation) -> StorageResult<()> {
        let data_json = serde_json::to_string(&obs.data)?;
        let conn = self.conn.lock().expect("lock poisoned");
        conn.execute(
            "INSERT INTO observations (
                id, timestamp, category, data, session_id,
                cwd, git_branch, command_count, time_since_last_ms,
                time_of_day, day_of_week, project, focus_duration_ms, energy_level
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                obs.id,
                obs.timestamp,
                obs.category.as_str(),
                data_json,
                obs.context.session_id,
                obs.context.cwd,
                obs.context.git_branch,
                obs.context.command_count,
                obs.context.time_since_last_ms,
                obs.context.time_of_day,
                obs.context.day_of_week,
                obs.context.project,
                obs.context.focus_duration_ms,
                obs.context.energy_level,
            ],
        )?;
        debug!("Stored observation {} ({})", obs.id, obs.category.as_str());
        Ok(())
    }

    /// Count observations, optionally filtered by category
    pub fn count(&self, category: Option<ObservationCategory>) -> StorageResult<u64> {
        let conn = self.conn.lock().expect("lock poisoned");
        let count: u64 = match category {
            Some(cat) => conn.query_row(
                "SELECT COUNT(*) FROM observations WHERE category = ?1",
                params![cat.as_str()],
                |row| row.get(0),
            )?,
            None => conn.query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))?,
        };
        Ok(count)
    }

    /// Query observations by session
    pub fn by_session(&self, session_id: &str) -> StorageResult<Vec<Observation>> {
        let conn = self.conn.lock().expect("lock poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, category, data, session_id, cwd, git_branch,
                    command_count, time_since_last_ms, time_of_day, day_of_week,
                    project, focus_duration_ms, energy_level
             FROM observations WHERE session_id = ?1 ORDER BY timestamp",
        )?;

        let rows = stmt.query_map(params![session_id], |row| {
            Ok(self.row_to_observation(row))
        })?;

        let mut results = Vec::new();
        for row in rows {
            match row? {
                Ok(obs) => results.push(obs),
                Err(e) => warn!("Failed to deserialize observation: {}", e),
            }
        }
        Ok(results)
    }

    fn row_to_observation(&self, row: &rusqlite::Row) -> StorageResult<Observation> {
        let category_str: String = row.get(2).map_err(StorageError::Database)?;
        let data_str: String = row.get(3).map_err(StorageError::Database)?;

        let category = match category_str.as_str() {
            "command" => ObservationCategory::Command,
            "error" => ObservationCategory::Error,
            "agent_chat" => ObservationCategory::AgentChat,
            "file_activity" => ObservationCategory::FileActivity,
            "context_usage" => ObservationCategory::ContextUsage,
            other => return Err(StorageError::Serialization(
                serde_json::Error::custom(format!("unknown category: {}", other)),
            )),
        };

        Ok(Observation {
            id: row.get(0).map_err(StorageError::Database)?,
            timestamp: row.get(1).map_err(StorageError::Database)?,
            category,
            data: serde_json::from_str(&data_str)?,
            context: crate::SessionContext {
                session_id: row.get(4).map_err(StorageError::Database)?,
                cwd: row.get(5).map_err(StorageError::Database)?,
                git_branch: row.get(6).map_err(StorageError::Database)?,
                command_count: row.get::<_, Option<u32>>(7).map_err(StorageError::Database)?.unwrap_or(0),
                time_since_last_ms: row.get(8).map_err(StorageError::Database)?,
                time_of_day: row.get::<_, Option<String>>(9).map_err(StorageError::Database)?.unwrap_or_default(),
                day_of_week: row.get::<_, Option<String>>(10).map_err(StorageError::Database)?.unwrap_or_default(),
                project: row.get(11).map_err(StorageError::Database)?,
                focus_duration_ms: row.get::<_, Option<i64>>(12).map_err(StorageError::Database)?.unwrap_or(0),
                energy_level: row.get::<_, Option<f64>>(13).map_err(StorageError::Database)?.unwrap_or(1.0),
            },
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandData, SessionContext};

    fn test_context() -> SessionContext {
        SessionContext {
            session_id: "test-session".to_string(),
            cwd: Some("/tmp/test".to_string()),
            git_branch: Some("main".to_string()),
            command_count: 1,
            time_since_last_ms: None,
            time_of_day: "morning".to_string(),
            day_of_week: "monday".to_string(),
            project: Some("test-project".to_string()),
            focus_duration_ms: 5000,
            energy_level: 0.85,
        }
    }

    #[test]
    fn test_open_in_memory() {
        let store = ObservationStore::open_in_memory().unwrap();
        assert_eq!(store.count(None).unwrap(), 0);
    }

    #[test]
    fn test_insert_and_count() {
        let store = ObservationStore::open_in_memory().unwrap();
        let data = serde_json::to_value(CommandData {
            name: "ls".to_string(),
            args: vec!["-la".to_string()],
            exit_code: 0,
            duration_ms: 42,
            pipe_position: None,
        })
        .unwrap();

        let obs = Observation::new(ObservationCategory::Command, data, test_context());
        store.insert(&obs).unwrap();

        assert_eq!(store.count(None).unwrap(), 1);
        assert_eq!(store.count(Some(ObservationCategory::Command)).unwrap(), 1);
        assert_eq!(store.count(Some(ObservationCategory::Error)).unwrap(), 0);
    }

    #[test]
    fn test_query_by_session() {
        let store = ObservationStore::open_in_memory().unwrap();

        let data = serde_json::to_value(CommandData {
            name: "echo".to_string(),
            args: vec!["hello".to_string()],
            exit_code: 0,
            duration_ms: 5,
            pipe_position: None,
        })
        .unwrap();

        let obs = Observation::new(ObservationCategory::Command, data, test_context());
        store.insert(&obs).unwrap();

        let results = store.by_session("test-session").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, ObservationCategory::Command);

        let empty = store.by_session("other-session").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_insert_all_categories() {
        let store = ObservationStore::open_in_memory().unwrap();
        let ctx = test_context();

        let categories = [
            (ObservationCategory::Command, serde_json::json!({"name": "ls", "args": [], "exit_code": 0, "duration_ms": 10})),
            (ObservationCategory::Error, serde_json::json!({"kind": "CommandNotFound", "message": "foo not found"})),
            (ObservationCategory::AgentChat, serde_json::json!({"turns": 3, "resolution": "solved"})),
            (ObservationCategory::FileActivity, serde_json::json!({"path": "/tmp/f", "action": "modified"})),
            (ObservationCategory::ContextUsage, serde_json::json!({"sources": ["file.rs"], "channel": "context"})),
        ];

        for (cat, data) in categories {
            let obs = Observation::new(cat, data, ctx.clone());
            store.insert(&obs).unwrap();
        }

        assert_eq!(store.count(None).unwrap(), 5);
    }
}
```

**Step 2: Update lib.rs to export storage**

Modify `crates/colony-reflection/src/lib.rs` — add after the `mod types;` line:

```rust
mod storage;

pub use storage::{ObservationStore, StorageError, StorageResult};
```

**Step 3: Run tests**

```bash
cargo test -p colony-reflection
```

Expected: 8 tests pass (4 types + 4 storage).

**Step 4: Commit**

```bash
git add crates/colony-reflection/
git commit -m "feat(reflection): add SQLite observation storage"
```

---

### Task 3: ReflectionObserver trait and SqliteObserver

**Files:**
- Create: `crates/colony-reflection/src/observer.rs`
- Modify: `crates/colony-reflection/src/lib.rs`

**Step 1: Write observer.rs**

Create `crates/colony-reflection/src/observer.rs`:

```rust
//! ReflectionObserver trait and SqliteObserver implementation

use crate::storage::ObservationStore;
use crate::types::{
    AgentChatData, CommandData, ContextUsageData, ErrorData, FileActivityData,
    Observation, ObservationCategory, SessionTracker,
};
use async_trait::async_trait;
use std::sync::Mutex;
use tracing::warn;

/// Trait for receiving observations from the shell
#[async_trait]
pub trait ReflectionObserver: Send + Sync {
    async fn observe_command(&self, cmd: CommandData);
    async fn observe_error(&self, err: ErrorData);
    async fn observe_agent_chat(&self, chat: AgentChatData);
    async fn observe_file_activity(&self, file: FileActivityData);
    async fn observe_context_usage(&self, ctx: ContextUsageData);
}

/// No-op observer for when reflection is disabled
pub struct NoopObserver;

#[async_trait]
impl ReflectionObserver for NoopObserver {
    async fn observe_command(&self, _cmd: CommandData) {}
    async fn observe_error(&self, _err: ErrorData) {}
    async fn observe_agent_chat(&self, _chat: AgentChatData) {}
    async fn observe_file_activity(&self, _file: FileActivityData) {}
    async fn observe_context_usage(&self, _ctx: ContextUsageData) {}
}

/// SQLite-backed observer that persists all observations
pub struct SqliteObserver {
    store: ObservationStore,
    tracker: Mutex<SessionTracker>,
}

impl SqliteObserver {
    /// Create observer with default database path
    pub fn new(session_id: String) -> Result<Self, crate::StorageError> {
        let store = ObservationStore::open(&ObservationStore::default_path())?;
        Ok(Self {
            store,
            tracker: Mutex::new(SessionTracker::new(session_id)),
        })
    }

    /// Create observer with custom store (for testing)
    pub fn with_store(store: ObservationStore, session_id: String) -> Self {
        Self {
            store,
            tracker: Mutex::new(SessionTracker::new(session_id)),
        }
    }

    fn record(&self, category: ObservationCategory, data: serde_json::Value) {
        let context = {
            let mut tracker = self.tracker.lock().expect("lock poisoned");
            if category == ObservationCategory::Error {
                tracker.record_error();
            }
            tracker.build_context()
        };

        let obs = Observation::new(category, data, context);
        if let Err(e) = self.store.insert(&obs) {
            warn!("Failed to store observation: {}", e);
        }
    }

    /// Get reference to the underlying store (for querying)
    pub fn store(&self) -> &ObservationStore {
        &self.store
    }
}

#[async_trait]
impl ReflectionObserver for SqliteObserver {
    async fn observe_command(&self, cmd: CommandData) {
        let data = serde_json::to_value(cmd).expect("CommandData is always serializable");
        self.record(ObservationCategory::Command, data);
    }

    async fn observe_error(&self, err: ErrorData) {
        let data = serde_json::to_value(err).expect("ErrorData is always serializable");
        self.record(ObservationCategory::Error, data);
    }

    async fn observe_agent_chat(&self, chat: AgentChatData) {
        let data = serde_json::to_value(chat).expect("AgentChatData is always serializable");
        self.record(ObservationCategory::AgentChat, data);
    }

    async fn observe_file_activity(&self, file: FileActivityData) {
        let data = serde_json::to_value(file).expect("FileActivityData is always serializable");
        self.record(ObservationCategory::FileActivity, data);
    }

    async fn observe_context_usage(&self, ctx: ContextUsageData) {
        let data = serde_json::to_value(ctx).expect("ContextUsageData is always serializable");
        self.record(ObservationCategory::ContextUsage, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_observer_records_command() {
        let store = ObservationStore::open_in_memory().unwrap();
        let observer = SqliteObserver::with_store(store, "test-session".to_string());

        observer
            .observe_command(CommandData {
                name: "ls".to_string(),
                args: vec!["-la".to_string()],
                exit_code: 0,
                duration_ms: 42,
                pipe_position: None,
            })
            .await;

        assert_eq!(observer.store().count(None).unwrap(), 1);
    }

    #[tokio::test]
    async fn test_sqlite_observer_records_all_types() {
        let store = ObservationStore::open_in_memory().unwrap();
        let observer = SqliteObserver::with_store(store, "test".to_string());

        observer
            .observe_command(CommandData {
                name: "echo".to_string(),
                args: vec![],
                exit_code: 0,
                duration_ms: 1,
                pipe_position: None,
            })
            .await;

        observer
            .observe_error(ErrorData {
                kind: "CommandNotFound".to_string(),
                message: "foo not found".to_string(),
                recovery_action: None,
                command_context: Some("foo".to_string()),
            })
            .await;

        observer
            .observe_agent_chat(AgentChatData {
                turns: 3,
                topic: Some("debugging".to_string()),
                resolution: "solved".to_string(),
                tokens_used: Some(1500),
                model: Some("claude-sonnet".to_string()),
            })
            .await;

        observer
            .observe_file_activity(FileActivityData {
                path: "/tmp/test.rs".to_string(),
                action: "modified".to_string(),
                size_delta: Some(42),
            })
            .await;

        observer
            .observe_context_usage(ContextUsageData {
                sources: vec!["file.rs".to_string()],
                tokens: Some(500),
                channel: "context".to_string(),
            })
            .await;

        assert_eq!(observer.store().count(None).unwrap(), 5);
    }

    #[tokio::test]
    async fn test_noop_observer_does_nothing() {
        let observer = NoopObserver;
        observer
            .observe_command(CommandData {
                name: "ls".to_string(),
                args: vec![],
                exit_code: 0,
                duration_ms: 1,
                pipe_position: None,
            })
            .await;
        // No panic, no storage — just verifies it compiles and runs
    }

    #[tokio::test]
    async fn test_error_degrades_energy() {
        let store = ObservationStore::open_in_memory().unwrap();
        let observer = SqliteObserver::with_store(store, "test".to_string());

        // First command — fresh session, high energy
        observer
            .observe_command(CommandData {
                name: "ls".to_string(),
                args: vec![],
                exit_code: 0,
                duration_ms: 1,
                pipe_position: None,
            })
            .await;

        let results = observer.store().by_session("test").unwrap();
        let initial_energy = results[0].context.energy_level;

        // Record several errors
        for _ in 0..10 {
            observer
                .observe_error(ErrorData {
                    kind: "CommandNotFound".to_string(),
                    message: "bad".to_string(),
                    recovery_action: None,
                    command_context: None,
                })
                .await;
        }

        // Next command should have lower energy
        observer
            .observe_command(CommandData {
                name: "ls".to_string(),
                args: vec![],
                exit_code: 0,
                duration_ms: 1,
                pipe_position: None,
            })
            .await;

        let results = observer.store().by_session("test").unwrap();
        let final_energy = results.last().unwrap().context.energy_level;
        assert!(final_energy < initial_energy);
    }
}
```

**Step 2: Update lib.rs**

Add after `mod storage;`:

```rust
mod observer;

pub use observer::{NoopObserver, ReflectionObserver, SqliteObserver};
```

**Step 3: Run tests**

```bash
cargo test -p colony-reflection
```

Expected: 12 tests pass.

**Step 4: Commit**

```bash
git add crates/colony-reflection/
git commit -m "feat(reflection): add ReflectionObserver trait and SqliteObserver"
```

---

### Task 4: Wire observer into Executor

**Files:**
- Modify: `crates/colony-exec/Cargo.toml`
- Modify: `crates/colony-exec/src/lib.rs` (lines 67-103, Executor struct and constructors)

**Step 1: Add colony-reflection dependency**

In `crates/colony-exec/Cargo.toml`, add to `[dependencies]`:

```toml
colony-reflection = { path = "../colony-reflection" }
```

**Step 2: Add observer field to Executor**

In `crates/colony-exec/src/lib.rs`, add import at top (near line 39):

```rust
use colony_reflection::ReflectionObserver;
use std::sync::Arc;
```

Modify the `Executor` struct (line 67) to add the observer field:

```rust
pub struct Executor {
    context: ExpansionContext,
    jobs: JobTable,
    tuple_space: TupleSpace,
    session_id: String,
    channels: ChannelManager,
    history: History,
    /// Reflection observer for collecting usage data
    observer: Arc<dyn ReflectionObserver>,
}
```

Update `Executor::new` (line 81):

```rust
    pub fn new(context: ExpansionContext) -> Self {
        let session_id = std::process::id().to_string();
        Self {
            context,
            jobs: JobTable::new(),
            tuple_space: TupleSpace::new(),
            session_id,
            channels: ChannelManager::new(),
            history: History::new(),
            observer: Arc::new(colony_reflection::NoopObserver),
        }
    }
```

Update `Executor::with_history` (line 93):

```rust
    pub fn with_history(context: ExpansionContext, history: History) -> Self {
        let session_id = std::process::id().to_string();
        Self {
            context,
            jobs: JobTable::new(),
            tuple_space: TupleSpace::new(),
            session_id,
            channels: ChannelManager::new(),
            history,
            observer: Arc::new(colony_reflection::NoopObserver),
        }
    }
```

Add a new constructor and accessor after `with_history`:

```rust
    /// Set the reflection observer
    pub fn set_observer(&mut self, observer: Arc<dyn ReflectionObserver>) {
        self.observer = observer;
    }

    /// Get reference to reflection observer
    pub fn observer(&self) -> &Arc<dyn ReflectionObserver> {
        &self.observer
    }
```

**Step 3: Verify it compiles**

```bash
cargo check -p colony-exec
```

Expected: compiles. Existing tests still pass since NoopObserver is the default.

**Step 4: Run tests**

```bash
cargo test -p colony-exec
```

Expected: all existing tests pass.

**Step 5: Commit**

```bash
git add crates/colony-exec/Cargo.toml crates/colony-exec/src/lib.rs
git commit -m "feat(reflection): wire ReflectionObserver into Executor"
```

---

### Task 5: Hook command observations

**Files:**
- Modify: `crates/colony-exec/src/command.rs` (lines 22-34, execute_command function)
- Modify: `crates/colony-exec/src/lib.rs` (re-export Arc<dyn ReflectionObserver>)

**Step 1: Modify execute_command to emit observations**

In `crates/colony-exec/src/command.rs`, add imports at top:

```rust
use colony_reflection::{CommandData, ReflectionObserver};
use std::sync::Arc;
use std::time::Instant;
```

Change the `execute_command` function signature to accept the observer, and wrap execution with timing:

```rust
pub async fn execute_command(
    cmd: &SimpleCommand,
    context: &mut ExpansionContext,
    observer: &Arc<dyn ReflectionObserver>,
) -> Result<ExitStatus> {
    let cmd_clone = cmd.clone();
    let mut context_clone = context.clone();

    let start = Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        execute_command_blocking(&cmd_clone, &mut context_clone)
    })
    .await
    .map_err(|e| ExecutionError::IoError(std::io::Error::other(e)))?;

    let duration_ms = start.elapsed().as_millis() as i64;

    // Extract command name for observation
    let cmd_name = cmd.name.raw_text().unwrap_or_default();
    let cmd_args: Vec<String> = cmd.args.iter().filter_map(|w| w.raw_text()).collect();

    let exit_code = match &result {
        Ok(status) => status.code(),
        Err(_) => -1,
    };

    observer
        .observe_command(CommandData {
            name: cmd_name,
            args: cmd_args,
            exit_code,
            duration_ms,
            pipe_position: None,
        })
        .await;

    result
}
```

**Step 2: Update all call sites of execute_command**

Search for all places that call `execute_command` and pass the observer through. The main call sites are in `colony-shell/src/shell/execution/` and `colony-exec/src/command_sub.rs`. Each call site needs the observer passed through.

In `crates/colony-exec/src/command_sub.rs`, update the `execute_and_capture` function to accept and pass the observer.

In `crates/colony-shell/src/shell/execution/`, the control flow calls into the executor which has the observer — thread it through.

> **Note to implementer:** The exact call sites will vary. Use `grep -rn "execute_command(" crates/` to find all call sites. Each one needs the observer parameter added. This may cascade through several functions. The Executor already has `self.observer` — pass `&self.observer` at each call.

**Step 3: Verify compilation**

```bash
cargo check
```

Fix any call site compilation errors by threading the observer through.

**Step 4: Run tests**

```bash
cargo test -p colony-exec
```

Expected: existing tests pass (they use NoopObserver via default Executor).

**Step 5: Commit**

```bash
git add -A
git commit -m "feat(reflection): hook command observations into execute_command"
```

---

### Task 6: Hook error observations

**Files:**
- Modify: `crates/colony-exec/src/command.rs`

**Step 1: Add error observation to execute_command**

In `execute_command`, after the `observer.observe_command(...)` call, add error observation when the result is `Err`:

```rust
    if let Err(ref e) = result {
        observer
            .observe_error(colony_reflection::ErrorData {
                kind: error_kind_str(e),
                message: e.to_string(),
                recovery_action: None,
                command_context: Some(cmd_name.clone()),
            })
            .await;
    }

    result
```

Add a helper function in the same file:

```rust
fn error_kind_str(e: &ExecutionError) -> String {
    match e {
        ExecutionError::CommandNotFound(_) => "CommandNotFound",
        ExecutionError::PermissionDenied(_) => "PermissionDenied",
        ExecutionError::ExpansionError(_) => "ExpansionError",
        ExecutionError::ForkError(_) => "ForkError",
        ExecutionError::ExecError(_) => "ExecError",
        ExecutionError::WaitError(_) => "WaitError",
        ExecutionError::IoError(_) => "IoError",
        ExecutionError::InvalidUtf8 => "InvalidUtf8",
        ExecutionError::RedirectError(_) => "RedirectError",
        ExecutionError::AmbiguousRedirect => "AmbiguousRedirect",
        ExecutionError::InvalidRedirect(_) => "InvalidRedirect",
        ExecutionError::Return(_) => "Return",
        ExecutionError::Break => "Break",
        ExecutionError::Continue => "Continue",
        ExecutionError::Exit(_) => "Exit",
        ExecutionError::Errexit(_) => "Errexit",
    }
    .to_string()
}
```

**Step 2: Run tests**

```bash
cargo test -p colony-exec
```

**Step 3: Commit**

```bash
git add crates/colony-exec/
git commit -m "feat(reflection): hook error observations"
```

---

### Task 7: Hook agent chat observations

**Files:**
- Modify: `crates/colony-agent/Cargo.toml`
- Modify: `crates/colony-agent/src/agent.rs` (around line 202, process method)

**Step 1: Add colony-reflection dependency**

In `crates/colony-agent/Cargo.toml`, add to `[dependencies]`:

```toml
colony-reflection = { path = "../colony-reflection" }
```

**Step 2: Add observer field to Agent**

In `crates/colony-agent/src/agent.rs`, add to the `Agent` struct (line 15):

```rust
    /// Reflection observer
    observer: Option<Arc<dyn colony_reflection::ReflectionObserver>>,
```

Add import: `use std::sync::Arc;`

Initialize as `None` in `with_config` and `with_provider` constructors.

Add setter:

```rust
    pub fn set_observer(&mut self, observer: Arc<dyn colony_reflection::ReflectionObserver>) {
        self.observer = Some(observer);
    }
```

**Step 3: Emit observation at end of process()**

In the `process` method (line 202), after the stream loop and before `Ok(response_text)`, add:

```rust
        // Record agent chat observation
        if let Some(ref observer) = self.observer {
            let turns = self.messages.len() as u32;
            observer
                .observe_agent_chat(colony_reflection::AgentChatData {
                    turns,
                    topic: None, // TODO: extract topic from first user message
                    resolution: if response_text.is_empty() {
                        "no_response".to_string()
                    } else {
                        "completed".to_string()
                    },
                    tokens_used: None, // TODO: track from stream events
                    model: Some(self.router.current_model().to_string()),
                })
                .await;
        }
```

**Step 4: Run tests**

```bash
cargo test -p colony-agent
```

**Step 5: Commit**

```bash
git add crates/colony-agent/
git commit -m "feat(reflection): hook agent chat observations"
```

---

### Task 8: Hook file activity observations

**Files:**
- Modify: `crates/colony-filetree/Cargo.toml`
- Modify: `crates/colony-filetree/src/watcher.rs` (FileSystemWatcher)

**Step 1: Add colony-reflection dependency**

In `crates/colony-filetree/Cargo.toml`, add:

```toml
colony-reflection = { path = "../colony-reflection" }
```

**Step 2: Add observer to FileSystemWatcher**

In `crates/colony-filetree/src/watcher.rs`, add an observer field:

```rust
use colony_reflection::ReflectionObserver;
use std::sync::Arc;

pub struct FileSystemWatcher {
    tree: TreeState,
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<WatchEvent>,
    observer: Option<Arc<dyn ReflectionObserver>>,
}
```

Initialize as `None` in `new()`. Add setter:

```rust
    pub fn set_observer(&mut self, observer: Arc<dyn ReflectionObserver>) {
        self.observer = Some(observer);
    }
```

**Step 3: Add method to process events with observations**

Add a method that processes the next watch event and emits an observation:

```rust
    /// Process the next watch event, emitting a reflection observation
    pub async fn next_event(&mut self) -> Option<WatchEvent> {
        let event = self.rx.recv().await?;

        if let Some(ref observer) = self.observer {
            let (path, action) = match &event {
                WatchEvent::Created(p) => (p.to_string_lossy().into_owned(), "created"),
                WatchEvent::Modified(p) => (p.to_string_lossy().into_owned(), "modified"),
                WatchEvent::Removed(p) => (p.to_string_lossy().into_owned(), "removed"),
                WatchEvent::Renamed { from, .. } => (from.to_string_lossy().into_owned(), "renamed"),
                WatchEvent::Error(_) => return Some(event),
            };

            observer
                .observe_file_activity(colony_reflection::FileActivityData {
                    path,
                    action: action.to_string(),
                    size_delta: None,
                })
                .await;
        }

        Some(event)
    }
```

**Step 4: Run tests**

```bash
cargo test -p colony-filetree
```

**Step 5: Commit**

```bash
git add crates/colony-filetree/
git commit -m "feat(reflection): hook file activity observations"
```

---

### Task 9: Hook context usage observations

**Files:**
- Modify: `crates/colony-channels/Cargo.toml`
- Modify: `crates/colony-channels/src/manager.rs`

**Step 1: Add colony-reflection dependency**

In `crates/colony-channels/Cargo.toml`, add:

```toml
colony-reflection = { path = "../colony-reflection" }
```

**Step 2: Add observer to ChannelManager**

In `crates/colony-channels/src/manager.rs`, add:

```rust
use colony_reflection::ReflectionObserver;
use std::sync::Arc;
```

Add field to `ChannelManager`:

```rust
pub struct ChannelManager {
    channels: HashMap<ChannelId, Box<dyn Channel>>,
    aliases: HashMap<String, ChannelId>,
    observer: Option<Arc<dyn ReflectionObserver>>,
}
```

Initialize as `None` in `new()`. Add setter:

```rust
    pub fn set_observer(&mut self, observer: Arc<dyn ReflectionObserver>) {
        self.observer = Some(observer);
    }
```

**Step 3: Add observation emission to write path**

Find the write method in ChannelManager (or wherever channel writes happen). After a successful write to the context channel (STDCONTEXT), emit:

```rust
    // After writing to context channel:
    if let Some(ref observer) = self.observer {
        let observer = Arc::clone(observer);
        let channel_name = self.aliases.iter()
            .find(|(_, &id)| id == channel_id)
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| channel_id.to_string());

        tokio::spawn(async move {
            observer
                .observe_context_usage(colony_reflection::ContextUsageData {
                    sources: vec![],
                    tokens: None,
                    channel: channel_name,
                })
                .await;
        });
    }
```

**Step 4: Run tests**

```bash
cargo test -p colony-channels
```

**Step 5: Commit**

```bash
git add crates/colony-channels/
git commit -m "feat(reflection): hook context usage observations"
```

---

### Task 10: Initialize SqliteObserver in shell startup

**Files:**
- Modify: `crates/colony-shell/Cargo.toml`
- Modify: `crates/colony-shell/src/main.rs` or wherever the shell initializes the Executor

**Step 1: Add colony-reflection dependency to colony-shell**

In `crates/colony-shell/Cargo.toml`:

```toml
colony-reflection = { path = "../colony-reflection" }
```

**Step 2: Find the shell startup and create the observer**

Find where `Executor::new()` or `Executor::with_history()` is called. After creating the executor, set the observer:

```rust
use colony_reflection::SqliteObserver;
use std::sync::Arc;

// After creating executor:
let session_id = std::process::id().to_string();
match SqliteObserver::new(session_id) {
    Ok(observer) => {
        let observer = Arc::new(observer);
        executor.set_observer(observer.clone());
        // Also set on channels and filetree watcher if available
    }
    Err(e) => {
        tracing::warn!("Failed to initialize reflection observer: {}", e);
        // Falls back to NoopObserver (default)
    }
}
```

**Step 3: Build the full project**

```bash
cargo build
```

**Step 4: Smoke test**

```bash
cargo run -- -c "echo hello && echo world"
```

Then check that observations were stored:

```bash
sqlite3 ~/.colony/reflection/observations.db "SELECT category, json_extract(data, '$.name'), json_extract(data, '$.exit_code') FROM observations ORDER BY timestamp DESC LIMIT 10;"
```

Expected: rows for "echo" commands with exit_code 0.

**Step 5: Commit**

```bash
git add crates/colony-shell/
git commit -m "feat(reflection): initialize SqliteObserver on shell startup"
```

---

### Task 11: End-to-end integration test

**Files:**
- Create: `crates/colony-reflection/tests/integration.rs`

**Step 1: Write integration test**

Create `crates/colony-reflection/tests/integration.rs`:

```rust
//! Integration test: full observation lifecycle

use colony_reflection::{
    AgentChatData, CommandData, ContextUsageData, ErrorData, FileActivityData,
    NoopObserver, ObservationCategory, ObservationStore, ReflectionObserver, SqliteObserver,
};

#[tokio::test]
async fn test_full_observation_lifecycle() {
    let store = ObservationStore::open_in_memory().unwrap();
    let observer = SqliteObserver::with_store(store, "integration-test".to_string());

    // Simulate a realistic session
    observer
        .observe_command(CommandData {
            name: "cd".to_string(),
            args: vec!["project/".to_string()],
            exit_code: 0,
            duration_ms: 1,
            pipe_position: None,
        })
        .await;

    observer
        .observe_command(CommandData {
            name: "cargo".to_string(),
            args: vec!["build".to_string()],
            exit_code: 0,
            duration_ms: 15000,
            pipe_position: None,
        })
        .await;

    observer
        .observe_error(ErrorData {
            kind: "CommandNotFound".to_string(),
            message: "typo_command not found".to_string(),
            recovery_action: None,
            command_context: Some("typo_command".to_string()),
        })
        .await;

    observer
        .observe_file_activity(FileActivityData {
            path: "src/main.rs".to_string(),
            action: "modified".to_string(),
            size_delta: Some(42),
        })
        .await;

    observer
        .observe_agent_chat(AgentChatData {
            turns: 4,
            topic: Some("fix build error".to_string()),
            resolution: "solved".to_string(),
            tokens_used: Some(2000),
            model: Some("claude-sonnet".to_string()),
        })
        .await;

    observer
        .observe_context_usage(ContextUsageData {
            sources: vec!["Cargo.toml".to_string(), "src/lib.rs".to_string()],
            tokens: Some(800),
            channel: "context".to_string(),
        })
        .await;

    // Verify counts
    let store = observer.store();
    assert_eq!(store.count(None).unwrap(), 6);
    assert_eq!(store.count(Some(ObservationCategory::Command)).unwrap(), 2);
    assert_eq!(store.count(Some(ObservationCategory::Error)).unwrap(), 1);
    assert_eq!(store.count(Some(ObservationCategory::AgentChat)).unwrap(), 1);
    assert_eq!(store.count(Some(ObservationCategory::FileActivity)).unwrap(), 1);
    assert_eq!(store.count(Some(ObservationCategory::ContextUsage)).unwrap(), 1);

    // Verify session query
    let all = store.by_session("integration-test").unwrap();
    assert_eq!(all.len(), 6);

    // Verify command counts increment
    assert_eq!(all[0].context.command_count, 1);
    assert_eq!(all[1].context.command_count, 2);

    // Verify timestamps are ordered
    for window in all.windows(2) {
        assert!(window[0].timestamp <= window[1].timestamp);
    }
}

#[tokio::test]
async fn test_noop_observer_is_zero_cost() {
    let observer = NoopObserver;
    // Should compile and run with no side effects
    for _ in 0..1000 {
        observer
            .observe_command(CommandData {
                name: "ls".to_string(),
                args: vec![],
                exit_code: 0,
                duration_ms: 1,
                pipe_position: None,
            })
            .await;
    }
}
```

**Step 2: Run integration tests**

```bash
cargo test -p colony-reflection --test integration
```

Expected: 2 tests pass.

**Step 3: Run all tests**

```bash
cargo test
```

Expected: all tests pass across all crates.

**Step 4: Commit**

```bash
git add crates/colony-reflection/tests/
git commit -m "test(reflection): add end-to-end integration test"
```
