# colony-history Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace colony-shell's flat-file history with a SQLite-backed `colony-history` crate that captures structured metadata, provides FTS5 search, agent tracking, privacy-aware redaction, and stats.

**Architecture:** New `colony-history` crate writes to the shared `~/.colony/reflection/observations.db` SQLite file (WAL mode). An async recording pipeline sends `HistoryRecord` structs to a background writer thread via bounded channel. The existing `History` VecDeque stays as a session cache for `!!`/`!N` expansion. FTS5 external-content table indexes commands for Ctrl+R search. `colony-secure` gains a `redact()` method for secret sanitization. The `history` builtin gets new subcommands. The panel binary gets browse/stats modes.

**Tech Stack:** Rust, rusqlite (bundled, FTS5), uuid, tracing, chrono, serde/serde_json, ratatui (panel), crossterm (panel)

**Spec:** `docs/superpowers/specs/2026-03-13-history-store-design.md`

---

## File Structure

### New crate: `crates/colony-history/`

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Crate manifest |
| `src/lib.rs` | Public API re-exports: `HistoryStore`, `HistoryRecord`, `SearchResult`, `HistoryStats` |
| `src/store.rs` | SQLite operations: open, migrate, insert, query, WAL setup, `Arc<Mutex<Connection>>` |
| `src/record.rs` | `HistoryRecord` struct with builder pattern |
| `src/privacy.rs` | `PrivacyFilter`: space-prefix check, regex exclusions, `colony-secure` redaction integration |
| `src/executor.rs` | `ExecutorDetector`: identify command origin (user, colony-agent, external agent) |
| `src/risk.rs` | `RiskScorer`: static rule-based risk assessment for agent commands |
| `src/search.rs` | `SearchQuery` builder, `SearchResult` type, FTS5 query construction |
| `src/stats.rs` | Aggregation queries: command frequency, directory patterns, agent breakdown, time distribution |
| `src/writer.rs` | Background writer thread: bounded channel receiver, batch inserts |
| `src/migrations/001_create_command_history.sql` | Initial schema DDL |

### Modified files

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace root, line 3-35) | Add `colony-history` to `[workspace.members]`; add `colony-history` to `[workspace.dependencies]` (line ~120) |
| `crates/colony-exec/Cargo.toml` | Add `colony-history` dependency |
| `crates/colony-exec/src/builtins.rs` | Extend `builtin_history()` (line 5151) to dispatch new subcommands to `colony-history` |
| `crates/colony-exec/src/history.rs` | Add `HistoryStore` integration: send records to background writer after `add()` |
| `crates/colony-secure/src/registry.rs` | Add `fn redact(&self, input: &str) -> String` method to `SecureRegistry` (after line 215) |
| `crates/colony-history-bin/src/main.rs` | Rewrite to read from `HistoryStore`, add browse/stats mode toggle |

---

## Chunk 1: Crate Skeleton and Data Model

### Task 1: Create colony-history crate skeleton

**Files:**
- Create: `crates/colony-history/Cargo.toml`
- Create: `crates/colony-history/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "colony-history"
version = "0.1.0"
edition = "2021"

[dependencies]
rusqlite = { workspace = true }
uuid = { workspace = true }
tracing = { workspace = true }
chrono = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
dirs = { workspace = true }
nix = { workspace = true }
regex = "1"

[dev-dependencies]
tempfile = "3"
```

**Note:** `regex` and `tempfile` are not workspace dependencies — use direct version specs. `nix` (with `hostname` feature, already in workspace) provides `gethostname()` — no need for a separate `hostname` crate.

- [ ] **Step 2: Create src/lib.rs with module declarations**

```rust
mod record;
mod store;

pub use record::HistoryRecord;
pub use store::HistoryStore;
```

- [ ] **Step 3: Add to workspace members in root Cargo.toml**

In `/Cargo.toml`:
1. Add `"crates/colony-history"` to the `[workspace.members]` array (after line 35)
2. Add `colony-history = { path = "crates/colony-history" }` to `[workspace.dependencies]` (after line 127)
3. **Critical:** Change the `rusqlite` features from `["bundled"]` to `["bundled-full"]` (line 95). The `bundled` feature alone does NOT enable FTS5. `bundled-full` includes FTS3, FTS5, and other extensions.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p colony-history`
Expected: compiles with warnings about unused modules

- [ ] **Step 5: Commit**

```bash
git add crates/colony-history/ Cargo.toml Cargo.lock
git commit -m "feat(history): create colony-history crate skeleton"
```

### Task 2: Define HistoryRecord struct

**Files:**
- Create: `crates/colony-history/src/record.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

In `crates/colony-history/src/record.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_builder_minimal() {
        let record = HistoryRecord::builder()
            .command("ls -la")
            .cwd("/home/user")
            .exit_code(0)
            .duration_ms(42)
            .session_id("sess-123")
            .build()
            .unwrap();

        assert_eq!(record.command, "ls -la");
        assert_eq!(record.cwd, "/home/user");
        assert_eq!(record.exit_code, 0);
        assert_eq!(record.executor, Executor::User);
        assert!(!record.id.is_empty());
    }

    #[test]
    fn test_record_builder_with_agent() {
        let record = HistoryRecord::builder()
            .command("rm -rf build/")
            .cwd("/project")
            .exit_code(0)
            .duration_ms(100)
            .session_id("sess-456")
            .executor(Executor::ColonyAgent)
            .agent_session_id("agent-789")
            .agent_goal("clean build artifacts")
            .build()
            .unwrap();

        assert_eq!(record.executor, Executor::ColonyAgent);
        assert_eq!(record.agent_session_id.as_deref(), Some("agent-789"));
    }

    #[test]
    fn test_record_builder_missing_required_field() {
        let result = HistoryRecord::builder()
            .command("ls")
            .build();

        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p colony-history -- test_record_builder`
Expected: FAIL — `HistoryRecord` and `Executor` not defined

- [ ] **Step 3: Implement HistoryRecord and builder**

In `crates/colony-history/src/record.rs`:

```rust
use chrono::Utc;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Executor {
    User,
    ColonyAgent,
    External(String),
}

impl Executor {
    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::ColonyAgent => "colony-agent",
            Self::External(name) => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    Critical,
    High,
    Medium,
    Low,
    Safe,
}

impl RiskLevel {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Safe => "safe",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistoryRecord {
    pub id: String,
    pub command: String,
    pub command_redacted: String,
    pub cwd: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub executor: Executor,
    pub agent_session_id: Option<String>,
    pub agent_goal: Option<String>,
    pub risk_level: Option<RiskLevel>,
    pub session_id: String,
    pub git_branch: Option<String>,
    pub hostname: String,
    pub timestamp: i64,
    pub hidden: bool,
    pub bookmarked: bool,
    pub note: Option<String>,
    pub tags: Option<String>,
}

#[derive(Debug, Default)]
pub struct HistoryRecordBuilder {
    command: Option<String>,
    command_redacted: Option<String>,
    cwd: Option<String>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    executor: Option<Executor>,
    agent_session_id: Option<String>,
    agent_goal: Option<String>,
    risk_level: Option<RiskLevel>,
    session_id: Option<String>,
    git_branch: Option<String>,
    hostname: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),
}

impl HistoryRecord {
    pub fn builder() -> HistoryRecordBuilder {
        HistoryRecordBuilder::default()
    }
}

impl HistoryRecordBuilder {
    pub fn command(mut self, cmd: impl Into<String>) -> Self {
        self.command = Some(cmd.into());
        self
    }

    pub fn command_redacted(mut self, cmd: impl Into<String>) -> Self {
        self.command_redacted = Some(cmd.into());
        self
    }

    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    pub fn duration_ms(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    pub fn executor(mut self, executor: Executor) -> Self {
        self.executor = Some(executor);
        self
    }

    pub fn agent_session_id(mut self, id: impl Into<String>) -> Self {
        self.agent_session_id = Some(id.into());
        self
    }

    pub fn agent_goal(mut self, goal: impl Into<String>) -> Self {
        self.agent_goal = Some(goal.into());
        self
    }

    pub fn risk_level(mut self, level: RiskLevel) -> Self {
        self.risk_level = Some(level);
        self
    }

    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    pub fn git_branch(mut self, branch: impl Into<String>) -> Self {
        self.git_branch = Some(branch.into());
        self
    }

    pub fn hostname(mut self, host: impl Into<String>) -> Self {
        self.hostname = Some(host.into());
        self
    }

    pub fn build(self) -> Result<HistoryRecord, RecordError> {
        let command = self.command.ok_or(RecordError::MissingField("command"))?;
        let cwd = self.cwd.ok_or(RecordError::MissingField("cwd"))?;
        let exit_code = self.exit_code.ok_or(RecordError::MissingField("exit_code"))?;
        let duration_ms = self.duration_ms.ok_or(RecordError::MissingField("duration_ms"))?;
        let session_id = self.session_id.ok_or(RecordError::MissingField("session_id"))?;

        let command_redacted = self.command_redacted.unwrap_or_else(|| command.clone());
        let hostname = self.hostname.unwrap_or_else(|| {
            nix::unistd::gethostname()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string())
        });

        Ok(HistoryRecord {
            id: Uuid::new_v4().to_string(),
            command,
            command_redacted,
            cwd,
            exit_code,
            duration_ms,
            executor: self.executor.unwrap_or(Executor::User),
            agent_session_id: self.agent_session_id,
            agent_goal: self.agent_goal,
            risk_level: self.risk_level,
            session_id,
            git_branch: self.git_branch,
            hostname,
            timestamp: Utc::now().timestamp_millis(),
            hidden: false,
            bookmarked: false,
            note: None,
            tags: None,
        })
    }
}
```

`nix` is already a workspace dependency with the `hostname` feature — no additional crate needed.

- [ ] **Step 4: Update lib.rs exports**

```rust
mod record;
mod store;

pub use record::{Executor, HistoryRecord, HistoryRecordBuilder, RecordError, RiskLevel};
pub use store::HistoryStore;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p colony-history -- test_record_builder`
Expected: 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/colony-history/
git commit -m "feat(history): add HistoryRecord struct with builder pattern"
```

### Task 3: Implement HistoryStore with SQLite schema

**Files:**
- Create: `crates/colony-history/src/store.rs`
- Create: `crates/colony-history/src/migrations/001_create_command_history.sql`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Create migration SQL file**

Create `crates/colony-history/src/migrations/001_create_command_history.sql`:

```sql
CREATE TABLE IF NOT EXISTS command_history (
    id TEXT PRIMARY KEY,
    command TEXT NOT NULL,
    command_redacted TEXT NOT NULL,
    cwd TEXT NOT NULL,
    exit_code INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    executor TEXT NOT NULL DEFAULT 'user',
    agent_session_id TEXT,
    agent_goal TEXT,
    risk_level TEXT,
    session_id TEXT NOT NULL,
    git_branch TEXT,
    hostname TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    hidden INTEGER NOT NULL DEFAULT 0,
    bookmarked INTEGER NOT NULL DEFAULT 0,
    note TEXT,
    tags TEXT
);

CREATE INDEX IF NOT EXISTS idx_history_timestamp ON command_history(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_history_command ON command_history(command);
CREATE INDEX IF NOT EXISTS idx_history_cwd ON command_history(cwd);
CREATE INDEX IF NOT EXISTS idx_history_executor ON command_history(executor);
CREATE INDEX IF NOT EXISTS idx_history_session ON command_history(session_id);

CREATE VIRTUAL TABLE IF NOT EXISTS command_history_fts USING fts5(
    command,
    content=command_history,
    content_rowid=rowid
);

-- Triggers to keep FTS index in sync with content table
CREATE TRIGGER IF NOT EXISTS command_history_ai AFTER INSERT ON command_history BEGIN
    INSERT INTO command_history_fts(rowid, command) VALUES (new.rowid, new.command);
END;

CREATE TRIGGER IF NOT EXISTS command_history_ad AFTER DELETE ON command_history BEGIN
    INSERT INTO command_history_fts(command_history_fts, rowid, command) VALUES('delete', old.rowid, old.command);
END;

CREATE TRIGGER IF NOT EXISTS command_history_au AFTER UPDATE ON command_history BEGIN
    INSERT INTO command_history_fts(command_history_fts, rowid, command) VALUES('delete', old.rowid, old.command);
    INSERT INTO command_history_fts(rowid, command) VALUES (new.rowid, new.command);
END;

-- Migration tracking
CREATE TABLE IF NOT EXISTS _colony_history_migrations (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at INTEGER NOT NULL
);
```

- [ ] **Step 2: Write failing tests for HistoryStore**

In `crates/colony-history/src/store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Executor, HistoryRecord};
    use tempfile::TempDir;

    fn test_store() -> (HistoryStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = HistoryStore::open(&db_path).unwrap();
        (store, dir)
    }

    fn sample_record(command: &str) -> HistoryRecord {
        HistoryRecord::builder()
            .command(command)
            .cwd("/tmp")
            .exit_code(0)
            .duration_ms(10)
            .session_id("test-session")
            .build()
            .unwrap()
    }

    #[test]
    fn test_open_creates_tables() {
        let (store, _dir) = test_store();
        // Should not panic — tables created successfully
        let count = store.count().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_and_retrieve() {
        let (store, _dir) = test_store();
        let record = sample_record("ls -la");
        store.insert(&record).unwrap();

        let count = store.count().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_search_fts() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("cargo build --release")).unwrap();
        store.insert(&sample_record("cargo test")).unwrap();
        store.insert(&sample_record("ls -la")).unwrap();

        let results = store.search_fts("cargo", 50).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_wal_mode_enabled() {
        let (store, _dir) = test_store();
        let mode = store.journal_mode().unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p colony-history -- test_open test_insert test_search test_wal`
Expected: FAIL — `HistoryStore` not implemented

- [ ] **Step 4: Implement HistoryStore**

In `crates/colony-history/src/store.rs`:

```rust
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection};
use tracing::warn;

use crate::record::HistoryRecord;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type StoreResult<T> = Result<T, StoreError>;

pub struct HistoryStore {
    conn: Mutex<Connection>,
    db_path: PathBuf,
}

impl HistoryStore {
    pub fn open(db_path: &Path) -> StoreResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        // Set file permissions to 0600 (owner-only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(db_path, perms);
        }

        let store = Self {
            conn: Mutex::new(conn),
            db_path: db_path.to_path_buf(),
        };
        store.run_migrations()?;
        Ok(store)
    }

    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".colony/reflection/observations.db")
    }

    fn run_migrations(&self) -> StoreResult<()> {
        let conn = self.conn.lock().expect("lock poisoned");
        let migration_sql = include_str!("migrations/001_create_command_history.sql");
        conn.execute_batch(migration_sql)?;
        Ok(())
    }

    pub fn insert(&self, record: &HistoryRecord) -> StoreResult<()> {
        let conn = self.conn.lock().expect("lock poisoned");
        conn.execute(
            "INSERT INTO command_history (
                id, command, command_redacted, cwd, exit_code, duration_ms,
                executor, agent_session_id, agent_goal, risk_level,
                session_id, git_branch, hostname, timestamp,
                hidden, bookmarked, note, tags
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18
            )",
            params![
                record.id,
                record.command,
                record.command_redacted,
                record.cwd,
                record.exit_code,
                record.duration_ms as i64,
                record.executor.as_str(),
                record.agent_session_id,
                record.agent_goal,
                record.risk_level.as_ref().map(|r| r.as_str()),
                record.session_id,
                record.git_branch,
                record.hostname,
                record.timestamp,
                record.hidden as i32,
                record.bookmarked as i32,
                record.note,
                record.tags,
            ],
        )?;
        Ok(())
    }

    pub fn count(&self) -> StoreResult<u64> {
        let conn = self.conn.lock().expect("lock poisoned");
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM command_history",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn search_fts(&self, query: &str, limit: u32) -> StoreResult<Vec<SearchResult>> {
        let conn = self.conn.lock().expect("lock poisoned");
        let mut stmt = conn.prepare(
            "SELECT h.id, h.command_redacted, h.cwd, h.executor, h.exit_code,
                    h.duration_ms, h.timestamp, h.bookmarked
             FROM command_history_fts f
             JOIN command_history h ON h.rowid = f.rowid
             WHERE command_history_fts MATCH ?1
               AND h.hidden = 0
             ORDER BY h.timestamp DESC
             LIMIT ?2",
        )?;

        let results = stmt
            .query_map(params![query, limit], |row| {
                Ok(SearchResult {
                    id: row.get(0)?,
                    command: row.get(1)?,
                    cwd: row.get(2)?,
                    executor: row.get(3)?,
                    exit_code: row.get(4)?,
                    duration_ms: row.get::<_, i64>(5)? as u64,
                    timestamp: row.get(6)?,
                    bookmarked: row.get::<_, i32>(7)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    pub fn journal_mode(&self) -> StoreResult<String> {
        let conn = self.conn.lock().expect("lock poisoned");
        let mode: String = conn.query_row(
            "PRAGMA journal_mode",
            [],
            |row| row.get(0),
        )?;
        Ok(mode)
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub executor: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub timestamp: i64,
    pub bookmarked: bool,
}
```

- [ ] **Step 5: Update lib.rs exports**

```rust
mod record;
mod store;

pub use record::{Executor, HistoryRecord, HistoryRecordBuilder, RecordError, RiskLevel};
pub use store::{HistoryStore, SearchResult, StoreError, StoreResult};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p colony-history`
Expected: 7 tests PASS (4 store + 3 record)

- [ ] **Step 7: Commit**

```bash
git add crates/colony-history/
git commit -m "feat(history): implement HistoryStore with SQLite schema and FTS5"
```

---

## Chunk 2: Privacy, Executor Detection, and Risk Scoring

### Task 4: Add redact() to SecureRegistry

**Files:**
- Modify: `crates/colony-secure/src/registry.rs` (after line 215)
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

Add to the existing test module in `crates/colony-secure/src/registry.rs`:

```rust
#[test]
fn test_redact_replaces_known_secrets() {
    let registry = SecureRegistry::with_defaults().unwrap();
    registry.set_secure_value("API_KEY", "sk-abc123xyz");

    let input = "curl -H 'Authorization: Bearer sk-abc123xyz' https://api.example.com";
    let redacted = registry.redact(input);

    assert!(!redacted.contains("sk-abc123xyz"));
    assert!(redacted.contains("***"));
    assert!(redacted.contains("https://api.example.com"));
}

#[test]
fn test_redact_no_secrets_returns_unchanged() {
    let registry = SecureRegistry::with_defaults().unwrap();
    let input = "echo hello world";
    let redacted = registry.redact(input);
    assert_eq!(redacted, input);
}
```

**Note:** `set_secure_value` takes 2 args `(name, value)` — no `SecureVarSource` param, no `Result` return.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-secure -- test_redact`
Expected: FAIL — `redact` method not defined

- [ ] **Step 3: Implement redact()**

Add to `SecureRegistry` impl block in `crates/colony-secure/src/registry.rs`:

```rust
/// Redact known secret values from a string.
/// Replaces any substring matching a stored credential value with `***`.
/// Secret values never leave this crate — only the sanitized result is returned.
pub fn redact(&self, input: &str) -> String {
    let vars = self.secure_vars.read(); // parking_lot::RwLock — never poisons, no Result
    let mut result = input.to_string();
    for var in vars.values() {
        if !var.value.is_empty() {
            result = result.replace(&var.value, "***");
        }
    }
    result
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p colony-secure -- test_redact`
Expected: 2 tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/colony-secure/src/registry.rs
git commit -m "feat(secure): add redact() method to SecureRegistry"
```

### Task 5: Implement PrivacyFilter

**Files:**
- Create: `crates/colony-history/src/privacy.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

In `crates/colony-history/src/privacy.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_space_prefix_skipped() {
        let filter = PrivacyFilter::new(vec![]);
        assert_eq!(filter.should_record(" secret command"), PrivacyAction::Skip);
    }

    #[test]
    fn test_regex_exclusion_skipped() {
        let filter = PrivacyFilter::new(
            vec![regex::Regex::new(r"^(export|set)\s+.*PASSWORD").unwrap()],
        );
        assert_eq!(
            filter.should_record("export DB_PASSWORD=hunter2"),
            PrivacyAction::Skip,
        );
    }

    #[test]
    fn test_normal_command_recorded() {
        let filter = PrivacyFilter::new(vec![]);
        assert_eq!(filter.should_record("ls -la"), PrivacyAction::Record);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-history -- test_space test_regex test_normal`
Expected: FAIL — `PrivacyFilter` not defined

- [ ] **Step 3: Implement PrivacyFilter**

In `crates/colony-history/src/privacy.rs`:

```rust
use regex::Regex;

#[derive(Debug, PartialEq, Eq)]
pub enum PrivacyAction {
    /// Command should not be recorded at all
    Skip,
    /// Command should be recorded — caller handles redaction separately
    Record,
}

pub struct PrivacyFilter {
    exclusion_patterns: Vec<Regex>,
}

impl PrivacyFilter {
    pub fn new(exclusion_patterns: Vec<Regex>) -> Self {
        Self { exclusion_patterns }
    }

    /// Check whether a command should be recorded or skipped.
    /// Does NOT perform redaction — that's the caller's responsibility
    /// via SecureRegistry::redact() when building the HistoryRecord.
    pub fn should_record(&self, command: &str) -> PrivacyAction {
        // Space-prefix: skip entirely
        if command.starts_with(' ') {
            return PrivacyAction::Skip;
        }

        // Regex exclusions: skip entirely
        for pattern in &self.exclusion_patterns {
            if pattern.is_match(command) {
                return PrivacyAction::Skip;
            }
        }

        PrivacyAction::Record
    }
}
```

**Design note:** Redaction is NOT part of PrivacyFilter. The filter only decides skip vs. record. Redaction happens when the caller builds the `HistoryRecord`, calling `SecureRegistry::redact()` to produce `command_redacted`. This avoids the filter needing to hold a reference to `SecureRegistry` and prevents double-redaction bugs.

- [ ] **Step 4: Update lib.rs**

Add `mod privacy;` and `pub use privacy::{PrivacyAction, PrivacyFilter};`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p colony-history -- test_space test_regex test_normal`
Expected: 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/colony-history/src/privacy.rs crates/colony-history/src/lib.rs
git commit -m "feat(history): implement PrivacyFilter with space-prefix and regex exclusions"
```

### Task 6: Implement ExecutorDetector

**Files:**
- Create: `crates/colony-history/src/executor.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

In `crates/colony-history/src/executor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_user_default() {
        let detector = ExecutorDetector::new(vec![]);
        let result = detector.detect(&HashMap::new());
        assert_eq!(result, crate::record::Executor::User);
    }

    #[test]
    fn test_detects_claude_code() {
        let detector = ExecutorDetector::default();
        let mut env = HashMap::new();
        env.insert("CLAUDE_CODE_ENTRYPOINT".to_string(), "1".to_string());
        let result = detector.detect(&env);
        assert_eq!(result, crate::record::Executor::External("claude-code".to_string()));
    }

    #[test]
    fn test_custom_detection_pattern() {
        let detector = ExecutorDetector::new(vec![
            AgentPattern { env_var: "MY_AGENT".to_string(), name: "my-agent".to_string() },
        ]);
        let mut env = HashMap::new();
        env.insert("MY_AGENT".to_string(), "true".to_string());
        let result = detector.detect(&env);
        assert_eq!(result, crate::record::Executor::External("my-agent".to_string()));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-history -- test_user test_detects test_custom`
Expected: FAIL — `ExecutorDetector` not defined

- [ ] **Step 3: Implement ExecutorDetector**

In `crates/colony-history/src/executor.rs`:

```rust
use std::collections::HashMap;

use crate::record::Executor;

#[derive(Debug, Clone)]
pub struct AgentPattern {
    pub env_var: String,
    pub name: String,
}

pub struct ExecutorDetector {
    patterns: Vec<AgentPattern>,
}

impl Default for ExecutorDetector {
    fn default() -> Self {
        Self::new(vec![
            AgentPattern { env_var: "CLAUDE_CODE_ENTRYPOINT".to_string(), name: "claude-code".to_string() },
            AgentPattern { env_var: "CODEX".to_string(), name: "codex".to_string() },
            AgentPattern { env_var: "CURSOR_SESSION".to_string(), name: "cursor".to_string() },
            AgentPattern { env_var: "WINDSURF_SESSION".to_string(), name: "windsurf".to_string() },
            AgentPattern { env_var: "AIDER".to_string(), name: "aider".to_string() },
        ])
    }
}

impl ExecutorDetector {
    pub fn new(patterns: Vec<AgentPattern>) -> Self {
        Self { patterns }
    }

    pub fn detect(&self, env: &HashMap<String, String>) -> Executor {
        for pattern in &self.patterns {
            if env.contains_key(&pattern.env_var) {
                return Executor::External(pattern.name.clone());
            }
        }
        Executor::User
    }
}
```

- [ ] **Step 4: Update lib.rs**

Add `mod executor;` and `pub use executor::{AgentPattern, ExecutorDetector};`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p colony-history -- test_user test_detects test_custom`
Expected: 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/colony-history/src/executor.rs crates/colony-history/src/lib.rs
git commit -m "feat(history): implement ExecutorDetector for agent identification"
```

### Task 7: Implement RiskScorer

**Files:**
- Create: `crates/colony-history/src/risk.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

In `crates/colony-history/src/risk.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critical_commands() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("rm -rf /"), RiskLevel::Critical);
        assert_eq!(scorer.score("sudo apt install foo"), RiskLevel::Critical);
        assert_eq!(scorer.score("chmod 777 /etc/passwd"), RiskLevel::Critical);
    }

    #[test]
    fn test_high_commands() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("git push --force origin main"), RiskLevel::High);
        assert_eq!(scorer.score("docker rm container1"), RiskLevel::High);
    }

    #[test]
    fn test_safe_commands() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("echo hello"), RiskLevel::Safe);
        assert_eq!(scorer.score("ls -la"), RiskLevel::Safe);
        assert_eq!(scorer.score("pwd"), RiskLevel::Safe);
    }

    #[test]
    fn test_low_commands() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("cat README.md"), RiskLevel::Low);
        assert_eq!(scorer.score("git status"), RiskLevel::Low);
        assert_eq!(scorer.score("git diff"), RiskLevel::Low);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-history -- test_critical test_high test_safe test_low`
Expected: FAIL — `RiskScorer` not defined

- [ ] **Step 3: Implement RiskScorer**

In `crates/colony-history/src/risk.rs`:

```rust
use regex::Regex;

use crate::record::RiskLevel;

struct RiskRule {
    pattern: Regex,
    level: RiskLevel,
}

pub struct RiskScorer {
    rules: Vec<RiskRule>,
}

impl Default for RiskScorer {
    fn default() -> Self {
        let rules = vec![
            // Critical
            (r"(?:^|\s)rm\s+.*-r.*-f|(?:^|\s)rm\s+.*-rf", RiskLevel::Critical),
            (r"(?:^|\s)sudo\s", RiskLevel::Critical),
            (r"chmod\s+777", RiskLevel::Critical),
            (r"(?i)DROP\s+TABLE", RiskLevel::Critical),
            (r"(?:>|>>)\s*/etc/", RiskLevel::Critical),
            // High
            (r"git\s+push\s+--force", RiskLevel::High),
            (r"git\s+push\s+-f\b", RiskLevel::High),
            (r"docker\s+rm\b", RiskLevel::High),
            (r"(?:npm|yarn|pnpm|cargo)\s+(?:install|add|remove|uninstall)", RiskLevel::High),
            (r"pip\s+(?:install|uninstall)", RiskLevel::High),
            // Medium
            (r"git\s+commit", RiskLevel::Medium),
            (r"(?:>|>>)\s", RiskLevel::Medium),
            (r"curl\s|wget\s|fetch\s", RiskLevel::Medium),
            (r"cp\s|mv\s", RiskLevel::Medium),
            // Low
            (r"(?:cat|head|tail|less|more)\s", RiskLevel::Low),
            (r"git\s+(?:status|diff|log|show|branch)", RiskLevel::Low),
            (r"(?:find|grep|rg|ag)\s", RiskLevel::Low),
            // Safe (explicit matches)
            (r"^(?:echo|printf|pwd|ls|tree|history|type|which|alias)\b", RiskLevel::Safe),
        ];

        let rules = rules
            .into_iter()
            .map(|(pattern, level)| RiskRule {
                pattern: Regex::new(pattern).expect("invalid risk pattern"),
                level,
            })
            .collect();

        Self { rules }
    }
}

impl RiskScorer {
    pub fn score(&self, command: &str) -> RiskLevel {
        // Rules are ordered by severity — return the first (highest) match
        for rule in &self.rules {
            if rule.pattern.is_match(command) {
                return rule.level.clone();
            }
        }
        // Default: medium for unknown commands
        RiskLevel::Medium
    }
}
```

Add `Clone` derive to `RiskLevel` in `record.rs`.

- [ ] **Step 4: Update lib.rs**

Add `mod risk;` and `pub use risk::RiskScorer;`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p colony-history -- test_critical test_high test_safe test_low`
Expected: 4 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/colony-history/src/risk.rs crates/colony-history/src/lib.rs crates/colony-history/src/record.rs
git commit -m "feat(history): implement static RiskScorer for agent commands"
```

---

## Chunk 3: Background Writer and Search

### Task 8: Implement background writer

**Files:**
- Create: `crates/colony-history/src/writer.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

In `crates/colony-history/src/writer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::HistoryRecord;
    use tempfile::TempDir;

    #[test]
    fn test_writer_processes_records() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = Arc::new(HistoryStore::open(&db_path).unwrap());

        let (writer, sender) = BackgroundWriter::new(store.clone());
        let handle = writer.spawn();

        let record = HistoryRecord::builder()
            .command("echo hello")
            .cwd("/tmp")
            .exit_code(0)
            .duration_ms(5)
            .session_id("test")
            .build()
            .unwrap();

        sender.send(record).unwrap();
        drop(sender); // Close channel to signal shutdown
        handle.join().unwrap();

        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn test_writer_reports_drop_count() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = Arc::new(HistoryStore::open(&db_path).unwrap());

        let (writer, _sender) = BackgroundWriter::new(store);
        assert_eq!(writer.drop_count(), 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-history -- test_writer`
Expected: FAIL — `BackgroundWriter` not defined

- [ ] **Step 3: Implement BackgroundWriter**

In `crates/colony-history/src/writer.rs`:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use tracing::warn;

use crate::record::HistoryRecord;
use crate::store::HistoryStore;

const CHANNEL_CAPACITY: usize = 256;

/// Combined handle for the background writer system.
/// Owns the sender channel and background thread.
/// On drop, closes the channel and joins the thread to ensure all records are flushed.
pub struct HistoryWriter {
    sender: Option<std::sync::mpsc::SyncSender<HistoryRecord>>,
    drops: Arc<AtomicU64>,
    handle: Option<JoinHandle<()>>,
}

impl HistoryWriter {
    pub fn start(store: Arc<HistoryStore>) -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel(CHANNEL_CAPACITY);
        let drops = Arc::new(AtomicU64::new(0));
        let drops_clone = drops.clone();

        let handle = thread::Builder::new()
            .name("colony-history-writer".to_string())
            .spawn(move || {
                for record in receiver {
                    if let Err(e) = store.insert(&record) {
                        warn!("failed to write history record: {e}");
                        drops_clone.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .expect("failed to spawn history writer thread");

        Self {
            sender: Some(sender),
            drops,
            handle: Some(handle),
        }
    }

    pub fn send(&self, record: HistoryRecord) {
        if let Some(ref sender) = self.sender {
            if sender.try_send(record).is_err() {
                self.drops.fetch_add(1, Ordering::Relaxed);
                warn!("history write channel full — record dropped");
            }
        }
    }

    pub fn drop_count(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }
}

impl Drop for HistoryWriter {
    fn drop(&mut self) {
        // Drop sender first to close the channel and signal shutdown
        self.sender.take();
        // Then join the thread to ensure all records are flushed
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
```

- [ ] **Step 4: Tests already use HistoryWriter — verify they pass**

The tests in Step 1 use `HistoryWriter` directly. The `Drop` impl joins the thread, so no `sleep()` needed — just `drop(writer)` and then assert.

Replace the test in Step 1 with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::HistoryRecord;
    use crate::store::HistoryStore;
    use tempfile::TempDir;

    fn sample_record(cmd: &str) -> HistoryRecord {
        HistoryRecord::builder()
            .command(cmd)
            .cwd("/tmp")
            .exit_code(0)
            .duration_ms(5)
            .session_id("test")
            .build()
            .unwrap()
    }

    #[test]
    fn test_writer_processes_records() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = Arc::new(HistoryStore::open(&db_path).unwrap());

        let writer = HistoryWriter::start(store.clone());
        writer.send(sample_record("echo hello"));
        writer.send(sample_record("ls -la"));

        // Drop joins the thread — all records flushed deterministically
        drop(writer);

        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn test_writer_drop_count_starts_zero() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = Arc::new(HistoryStore::open(&db_path).unwrap());

        let writer = HistoryWriter::start(store);
        assert_eq!(writer.drop_count(), 0);
    }
}
```

- [ ] **Step 5: Update lib.rs**

Add `mod writer;` and `pub use writer::HistoryWriter;`

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p colony-history -- test_writer`
Expected: 2 tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/colony-history/src/writer.rs crates/colony-history/src/lib.rs
git commit -m "feat(history): implement background writer with bounded channel"
```

### Task 9: Implement search query builder and stats

**Files:**
- Create: `crates/colony-history/src/search.rs`
- Create: `crates/colony-history/src/stats.rs`
- Modify: `crates/colony-history/src/store.rs` (add query methods)
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests for search**

In `crates/colony-history/src/search.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_query_builder() {
        let query = SearchQuery::new("cargo build")
            .cwd_filter("/project")
            .executor_filter("user")
            .limit(20)
            .dedup(true);

        assert_eq!(query.text, "cargo build");
        assert_eq!(query.cwd, Some("/project".to_string()));
        assert_eq!(query.limit, 20);
        assert!(query.dedup);
    }
}
```

- [ ] **Step 2: Implement SearchQuery**

In `crates/colony-history/src/search.rs`:

```rust
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: String,
    pub cwd: Option<String>,
    pub executor: Option<String>,
    pub limit: u32,
    pub dedup: bool,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            cwd: None,
            executor: None,
            limit: 50,
            dedup: false,
        }
    }

    pub fn cwd_filter(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn executor_filter(mut self, executor: impl Into<String>) -> Self {
        self.executor = Some(executor.into());
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    pub fn dedup(mut self, dedup: bool) -> Self {
        self.dedup = dedup;
        self
    }
}
```

- [ ] **Step 3: Write failing tests for stats**

In `crates/colony-history/src/stats.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::HistoryRecord;
    use crate::store::HistoryStore;
    use tempfile::TempDir;

    fn sample_record(cmd: &str, cwd: &str, executor: &str) -> HistoryRecord {
        let mut record = HistoryRecord::builder()
            .command(cmd)
            .cwd(cwd)
            .exit_code(0)
            .duration_ms(10)
            .session_id("test");

        if executor != "user" {
            record = record.executor(crate::record::Executor::External(executor.to_string()));
        }

        record.build().unwrap()
    }

    #[test]
    fn test_stats_total_count() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = HistoryStore::open(&db_path).unwrap();
        store.insert(&sample_record("ls", "/tmp", "user")).unwrap();
        store.insert(&sample_record("pwd", "/home", "user")).unwrap();

        let stats = HistoryStats::compute(&store).unwrap();
        assert_eq!(stats.total_commands, 2);
    }
}
```

- [ ] **Step 4: Implement HistoryStats**

In `crates/colony-history/src/stats.rs`:

```rust
use crate::store::{HistoryStore, StoreResult};

#[derive(Debug)]
pub struct HistoryStats {
    pub total_commands: u64,
    pub earliest_timestamp: Option<i64>,
    pub today_count: u64,
    pub week_count: u64,
    pub month_count: u64,
    pub executor_counts: Vec<(String, u64)>,
    pub top_directories: Vec<(String, u64)>,
    pub peak_hours: Vec<(u32, u64)>,
    pub drop_count: u64,
}

impl HistoryStats {
    pub fn compute(store: &HistoryStore) -> StoreResult<Self> {
        let total_commands = store.count()?;

        let now = chrono::Utc::now().timestamp_millis();
        let day_ago = now - 86_400_000;
        let week_ago = now - 604_800_000;
        let month_ago = now - 2_592_000_000;

        let today_count = store.count_since(day_ago)?;
        let week_count = store.count_since(week_ago)?;
        let month_count = store.count_since(month_ago)?;

        let earliest_timestamp = store.earliest_timestamp()?;
        let executor_counts = store.executor_counts()?;
        let top_directories = store.top_directories(10)?;
        let peak_hours = store.hourly_distribution()?;

        Ok(Self {
            total_commands,
            earliest_timestamp,
            today_count,
            week_count,
            month_count,
            executor_counts,
            top_directories,
            peak_hours,
            drop_count: 0,
        })
    }
}
```

- [ ] **Step 5: Add aggregation query methods to HistoryStore**

Add to `crates/colony-history/src/store.rs`:

```rust
pub fn count_since(&self, timestamp: i64) -> StoreResult<u64> {
    let conn = self.conn.lock().expect("lock poisoned");
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM command_history WHERE timestamp >= ?1",
        params![timestamp],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

pub fn earliest_timestamp(&self) -> StoreResult<Option<i64>> {
    let conn = self.conn.lock().expect("lock poisoned");
    let ts: Option<i64> = conn.query_row(
        "SELECT MIN(timestamp) FROM command_history",
        [],
        |row| row.get(0),
    )?;
    Ok(ts)
}

pub fn executor_counts(&self) -> StoreResult<Vec<(String, u64)>> {
    let conn = self.conn.lock().expect("lock poisoned");
    let mut stmt = conn.prepare(
        "SELECT executor, COUNT(*) as cnt FROM command_history
         GROUP BY executor ORDER BY cnt DESC",
    )?;
    let results = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(results)
}

pub fn top_directories(&self, limit: u32) -> StoreResult<Vec<(String, u64)>> {
    let conn = self.conn.lock().expect("lock poisoned");
    let mut stmt = conn.prepare(
        "SELECT cwd, COUNT(*) as cnt FROM command_history
         GROUP BY cwd ORDER BY cnt DESC LIMIT ?1",
    )?;
    let results = stmt
        .query_map(params![limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(results)
}

pub fn hourly_distribution(&self) -> StoreResult<Vec<(u32, u64)>> {
    // Compute hours in local time by applying UTC offset
    let local_offset_secs = chrono::Local::now().offset().local_minus_utc() as i64;

    let conn = self.conn.lock().expect("lock poisoned");
    let mut stmt = conn.prepare(
        "SELECT CAST(((timestamp / 1000 + ?1) % 86400) / 3600 AS INTEGER) as hour,
                COUNT(*) as cnt
         FROM command_history
         GROUP BY hour ORDER BY hour",
    )?;
    let results = stmt
        .query_map(params![local_offset_secs], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, i64>(1)? as u64))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(results)
}

pub fn search_advanced(&self, query: &crate::search::SearchQuery) -> StoreResult<Vec<SearchResult>> {
    let conn = self.conn.lock().expect("lock poisoned");

    let mut sql = String::from(
        "SELECT h.id, h.command_redacted, h.cwd, h.executor, h.exit_code,
                h.duration_ms, h.timestamp, h.bookmarked
         FROM command_history_fts f
         JOIN command_history h ON h.rowid = f.rowid
         WHERE command_history_fts MATCH ?1
           AND h.hidden = 0",
    );
    let mut param_idx = 2;

    if query.cwd.is_some() {
        sql.push_str(&format!(" AND h.cwd = ?{param_idx}"));
        param_idx += 1;
    }
    if query.executor.is_some() {
        sql.push_str(&format!(" AND h.executor = ?{param_idx}"));
    }

    if query.dedup {
        sql.push_str(" GROUP BY h.command");
    }

    sql.push_str(&format!(" ORDER BY h.timestamp DESC LIMIT {}", query.limit));

    let mut stmt = conn.prepare(&sql)?;

    // Build params dynamically
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(query.text.clone()),
    ];
    if let Some(ref cwd) = query.cwd {
        params_vec.push(Box::new(cwd.clone()));
    }
    if let Some(ref executor) = query.executor {
        params_vec.push(Box::new(executor.clone()));
    }

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let results = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(SearchResult {
                id: row.get(0)?,
                command: row.get(1)?,
                cwd: row.get(2)?,
                executor: row.get(3)?,
                exit_code: row.get(4)?,
                duration_ms: row.get::<_, i64>(5)? as u64,
                timestamp: row.get(6)?,
                bookmarked: row.get::<_, i32>(7)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

pub fn delete_before(&self, timestamp: i64) -> StoreResult<u64> {
    let conn = self.conn.lock().expect("lock poisoned");
    let count = conn.execute(
        "DELETE FROM command_history WHERE timestamp < ?1",
        params![timestamp],
    )?;
    Ok(count as u64)
}

pub fn delete_by_pattern(&self, pattern: &str) -> StoreResult<u64> {
    // SQLite has no built-in REGEXP — match in Rust, delete by collected IDs
    let re = regex::Regex::new(pattern)
        .map_err(|e| StoreError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let conn = self.conn.lock().expect("lock poisoned");
    let mut stmt = conn.prepare("SELECT id, command FROM command_history")?;
    let ids: Vec<String> = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let cmd: String = row.get(1)?;
            Ok((id, cmd))
        })?
        .filter_map(|r| r.ok())
        .filter(|(_, cmd)| re.is_match(cmd))
        .map(|(id, _)| id)
        .collect();

    let mut deleted = 0u64;
    for id in &ids {
        deleted += conn.execute("DELETE FROM command_history WHERE id = ?1", params![id])? as u64;
    }
    Ok(deleted)
}

pub fn toggle_bookmark(&self, id: &str) -> StoreResult<bool> {
    let conn = self.conn.lock().expect("lock poisoned");
    let current: i32 = conn.query_row(
        "SELECT bookmarked FROM command_history WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    let new_val = if current == 0 { 1 } else { 0 };
    conn.execute(
        "UPDATE command_history SET bookmarked = ?1 WHERE id = ?2",
        params![new_val, id],
    )?;
    Ok(new_val == 1)
}

pub fn set_note(&self, id: &str, note: &str) -> StoreResult<()> {
    let conn = self.conn.lock().expect("lock poisoned");
    conn.execute(
        "UPDATE command_history SET note = ?1 WHERE id = ?2",
        params![note, id],
    )?;
    Ok(())
}

pub fn set_tags(&self, id: &str, tags: &str) -> StoreResult<()> {
    let conn = self.conn.lock().expect("lock poisoned");
    conn.execute(
        "UPDATE command_history SET tags = ?1 WHERE id = ?2",
        params![tags, id],
    )?;
    Ok(())
}
```

- [ ] **Step 6: Update lib.rs**

Add `mod search;`, `mod stats;`, and exports.

- [ ] **Step 7: Run all tests**

Run: `cargo test -p colony-history`
Expected: all tests PASS

- [ ] **Step 8: Commit**

```bash
git add crates/colony-history/
git commit -m "feat(history): implement search query builder, stats, and store query methods"
```

---

## Chunk 4: Integration with colony-exec

### Task 10: Add colony-history dependency to colony-exec

**Files:**
- Modify: `crates/colony-exec/Cargo.toml`

- [ ] **Step 1: Add dependency**

Add `colony-history = { workspace = true }` to `[dependencies]` in `crates/colony-exec/Cargo.toml`.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/Cargo.toml
git commit -m "chore(exec): add colony-history dependency"
```

### Task 11: Extend execute_builtin with HistoryStore parameter

**Files:**
- Modify: `crates/colony-exec/src/builtins.rs` (lines 154-162, 272, 5151-5169)

This task threads `Option<&Arc<HistoryStore>>` through the builtin execution path so the `history` subcommands can access the store.

- [ ] **Step 1: Add HistoryStore parameter to execute_builtin()**

In `crates/colony-exec/src/builtins.rs`, modify the `execute_builtin` function signature (line 154) to add `history_store: Option<&std::sync::Arc<colony_history::HistoryStore>>` parameter.

- [ ] **Step 2: Thread it to builtin_history()**

Modify the match arm at line 272 to pass `history_store`:

```rust
"history" => builtin_history(&args, history, history_store),
```

- [ ] **Step 3: Update builtin_history() to dispatch new subcommands**

In `crates/colony-exec/src/builtins.rs`, modify `builtin_history()` (line 5151):

```rust
fn builtin_history(
    args: &[String],
    history: Option<&mut crate::history::History>,
    history_store: Option<&std::sync::Arc<colony_history::HistoryStore>>,
) -> Result<BuiltinResult> {
    // Check for new subcommands first
    if let Some(subcommand) = args.first().map(|s| s.as_str()) {
        match subcommand {
            "search" => return history_search(args, history_store),
            "stats" => return history_stats(history_store),
            "bookmark" => return history_bookmark(args, history_store),
            "note" => return history_note(args, history_store),
            "tag" => return history_tag(args, history_store),
            "agents" => return history_agents(history_store),
            "delete" => return history_delete(args, history_store),
            "export" => return history_export(args, history_store),
            _ => {}
        }
    }

    // Fall through to existing behavior
    match history {
        Some(h) => {
            let exit_code = crate::history::execute_history_builtin(args, h);
            if exit_code == 0 {
                Ok(BuiltinResult::Success(0))
            } else {
                Ok(BuiltinResult::Failure(exit_code))
            }
        }
        None => {
            eprintln!("history: not available in subshell");
            Ok(BuiltinResult::Failure(1))
        }
    }
}
```

- [ ] **Step 4: Implement history_search subcommand**

```rust
fn history_search(
    args: &[String],
    store: Option<&std::sync::Arc<colony_history::HistoryStore>>,
) -> Result<BuiltinResult> {
    let Some(store) = store else {
        eprintln!("history: store not available");
        return Ok(BuiltinResult::Failure(1));
    };
    let query_text = args[1..].join(" ");
    if query_text.is_empty() {
        eprintln!("history search: missing query");
        return Ok(BuiltinResult::Failure(1));
    }

    match store.search_fts(&query_text, 50) {
        Ok(results) => {
            for r in &results {
                let ts = chrono::DateTime::from_timestamp_millis(r.timestamp)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default();
                println!("{} [{}] {} ({})", r.id, r.executor, r.command, ts);
            }
            if results.is_empty() {
                println!("No results found.");
            }
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("history search: {e}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}
```

- [ ] **Step 5: Implement history_stats subcommand**

```rust
fn history_stats(
    store: Option<&std::sync::Arc<colony_history::HistoryStore>>,
) -> Result<BuiltinResult> {
    let Some(store) = store else {
        eprintln!("history: store not available");
        return Ok(BuiltinResult::Failure(1));
    };

    match colony_history::HistoryStats::compute(store) {
        Ok(stats) => {
            let since = stats.earliest_timestamp
                .and_then(|ts| chrono::DateTime::from_timestamp_millis(ts))
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "unknown".to_string());

            println!("Commands: {} (since {})", stats.total_commands, since);
            println!(
                "Today: {}  │  This week: {}  │  This month: {}",
                stats.today_count, stats.week_count, stats.month_count
            );

            if !stats.executor_counts.is_empty() {
                println!("\nBy executor:");
                for (executor, count) in &stats.executor_counts {
                    let pct = (*count as f64 / stats.total_commands as f64) * 100.0;
                    println!("  {:<16} {:>6} ({:.1}%)", executor, count, pct);
                }
            }

            if !stats.top_directories.is_empty() {
                println!("\nTop directories:");
                for (dir, count) in &stats.top_directories {
                    let display = dir.replace(
                        &dirs::home_dir().unwrap_or_default().to_string_lossy().to_string(),
                        "~",
                    );
                    println!("  {:<40} {:>6}", display, count);
                }
            }

            // Peak hours
            if !stats.peak_hours.is_empty() {
                let mut sorted = stats.peak_hours.clone();
                sorted.sort_by(|a, b| b.1.cmp(&a.1));
                let peaks: Vec<String> = sorted.iter().take(4).map(|(h, _)| {
                    format!("{:02}-{:02}", h, h + 1)
                }).collect();
                println!("\nPeak hours: {}", peaks.join(", "));
            }

            if stats.drop_count > 0 {
                println!("\nDropped records: {}", stats.drop_count);
            }

            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("history stats: {e}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}
```

- [ ] **Step 6: Implement remaining subcommands**

Add `history_bookmark`, `history_note`, `history_tag`, `history_delete`, `history_export` — each follows the same pattern as `history_search`, calling the corresponding `HistoryStore` method.

For `history_agents`, add an `agent_summary()` method to `HistoryStore`:

```rust
pub fn agent_summary(&self) -> StoreResult<Vec<(String, u64, Vec<(String, u64)>)>> {
    let conn = self.conn.lock().expect("lock poisoned");
    let mut stmt = conn.prepare(
        "SELECT executor, risk_level, COUNT(*) as cnt
         FROM command_history
         WHERE executor != 'user'
         GROUP BY executor, risk_level
         ORDER BY executor, cnt DESC",
    )?;
    let rows: Vec<(String, Option<String>, u64)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? as u64))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // Group by executor
    let mut result: Vec<(String, u64, Vec<(String, u64)>)> = Vec::new();
    for (executor, risk, count) in &rows {
        let risk_str = risk.as_deref().unwrap_or("unknown").to_string();
        if let Some(entry) = result.iter_mut().find(|(e, _, _)| e == executor) {
            entry.1 += count;
            entry.2.push((risk_str, *count));
        } else {
            result.push((executor.clone(), *count, vec![(risk_str, *count)]));
        }
    }
    Ok(result)
}
```

Then implement `history_agents()`:

```rust
fn history_agents(
    store: Option<&std::sync::Arc<colony_history::HistoryStore>>,
) -> Result<BuiltinResult> {
    let Some(store) = store else {
        eprintln!("history: store not available");
        return Ok(BuiltinResult::Failure(1));
    };
    match store.agent_summary() {
        Ok(summary) => {
            if summary.is_empty() {
                println!("No agent commands recorded.");
            }
            for (executor, total, risks) in &summary {
                println!("{} ({} commands):", executor, total);
                for (risk, count) in risks {
                    println!("  {:<10} {}", risk, count);
                }
            }
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("history agents: {e}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}
```

- [ ] **Step 7: Update all call sites of execute_builtin()**

There are ~18 call sites across these files — add the `history_store` parameter to each:

- `crates/colony-exec/src/lib.rs` — 1 site (pass through from caller)
- `crates/colony-exec/src/pipeline.rs` — 2 sites (pass through from caller)
- `crates/colony-exec/src/command_sub.rs` — 2 sites (pass `None` — subshells don't get the store)
- `crates/colony-exec/src/builtins.rs` — ~12 sites in tests (pass `None`)

The REPL in `colony-shell` passes `Some(&store)`. All subshell/command-substitution paths pass `None`.

Run: `cargo check -p colony-exec`
Expected: compiles

- [ ] **Step 8: Commit**

```bash
git add crates/colony-exec/
git commit -m "feat(history): extend history builtin with search, stats, bookmark, and other subcommands"
```

### Task 12: Wire recording pipeline into command execution

**Files:**
- Modify: `crates/colony-exec/src/history.rs`
- Modify: `crates/colony-shell/src/shell/engine.rs` (or wherever the REPL loop calls command execution)

- [ ] **Step 1: Add HistoryWriter to the shell engine**

The shell engine (REPL) creates an `Arc<HistoryStore>` and a `HistoryWriter` at startup. After each command execution, it builds a `HistoryRecord` and sends it to the writer.

This is the integration point where exit code, duration, and executor tagging come together. The exact file depends on where the REPL loop lives — likely `colony-shell/src/shell/engine.rs` or `colony-shell/src/repl.rs`.

- [ ] **Step 2: Capture timing around command execution**

Wrap command execution with `Instant::now()` / `elapsed()` to capture `duration_ms`.

- [ ] **Step 3: Build HistoryRecord after execution**

```rust
// PrivacyFilter only decides skip/record — redaction is done by SecureRegistry directly
if privacy_filter.should_record(&command_text) == PrivacyAction::Skip {
    // Still add to VecDeque for !!/!N (matches existing behavior), but don't persist
} else {
    let record = HistoryRecord::builder()
        .command(&command_text)
        .command_redacted(secure_registry.redact(&command_text))
        .cwd(&current_dir)
        .exit_code(exit_code)
        .duration_ms(duration.as_millis() as u64)
        .session_id(&session_id)
        .git_branch(git_branch.as_deref().unwrap_or(""))
        .build()?;

    history_writer.send(record);
}
```

- [ ] **Step 4: Verify it compiles and runs**

Run: `cargo check -p colony-shell`
Then run the shell manually and execute a few commands, then check `history stats`.

- [ ] **Step 5: Commit**

```bash
git add crates/colony-exec/ crates/colony-shell/
git commit -m "feat(history): wire recording pipeline into command execution loop"
```

---

## Chunk 5: Ctrl+R Search Refactor

### Task 13: Refactor InteractiveHistorySearch to use HistoryStore

**Files:**
- Modify: `crates/colony-exec/src/history.rs` (lines 642-833)

- [ ] **Step 1: Add SQLite search path to InteractiveHistorySearch**

Modify `InteractiveHistorySearch` to accept an optional `Arc<HistoryStore>`. When present, `update_results()` queries FTS5 instead of scanning the VecDeque.

```rust
pub struct InteractiveHistorySearch<'a> {
    history: &'a History,
    store: Option<Arc<colony_history::HistoryStore>>,
    query: String,
    results: Vec<HistorySearchResult>,
    selected_index: usize,
}

impl<'a> InteractiveHistorySearch<'a> {
    pub fn new(history: &'a History, store: Option<Arc<colony_history::HistoryStore>>) -> Self {
        Self {
            history,
            store,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn update_results(&mut self) {
        if self.query.is_empty() {
            self.results.clear();
            return;
        }

        // Prefer SQLite FTS5 if store is available
        if let Some(ref store) = self.store {
            match store.search_fts(&self.query, 50) {
                Ok(results) => {
                    self.results = results
                        .into_iter()
                        .map(|r| HistorySearchResult {
                            command: r.command,
                            index: 0,       // not positional
                            match_start: 0, // FTS5 handles relevance
                            match_end: 0,
                        })
                        .collect();
                    return;
                }
                Err(_) => {} // Fall through to VecDeque search
            }
        }

        // Fallback: in-memory fuzzy search
        self.results = self.history.fuzzy_search(&self.query, 50);
    }
}
```

- [ ] **Step 2: Verify up-arrow and !!/!N still use VecDeque**

Confirm that `History::previous()`, `History::next()`, and `History::expand()` are unchanged — they continue using the VecDeque.

- [ ] **Step 3: Test manually**

Run the shell, type Ctrl+R and search for a command. Verify results come from SQLite when available, and from VecDeque as fallback.

- [ ] **Step 4: Commit**

```bash
git add crates/colony-exec/src/history.rs
git commit -m "feat(history): refactor Ctrl+R search to use FTS5 with VecDeque fallback"
```

---

## Chunk 6: Export and Panel Binary Upgrade

### Task 14: Implement history export subcommand

**Files:**
- Modify: `crates/colony-history/src/store.rs` (add export method)
- Modify: `crates/colony-exec/src/builtins.rs` (wire up export)

- [ ] **Step 1: Add export method to HistoryStore**

In `crates/colony-history/src/store.rs`:

```rust
pub fn export_all(&self, format: ExportFormat) -> StoreResult<String> {
    let conn = self.conn.lock().expect("lock poisoned");
    let mut stmt = conn.prepare(
        "SELECT id, command_redacted, cwd, exit_code, duration_ms,
                executor, timestamp, bookmarked, note, tags
         FROM command_history
         ORDER BY timestamp ASC",
    )?;

    let records: Vec<ExportRecord> = stmt
        .query_map([], |row| {
            Ok(ExportRecord {
                id: row.get(0)?,
                command: row.get(1)?,
                cwd: row.get(2)?,
                exit_code: row.get(3)?,
                duration_ms: row.get::<_, i64>(4)? as u64,
                executor: row.get(5)?,
                timestamp: row.get(6)?,
                bookmarked: row.get::<_, i32>(7)? != 0,
                note: row.get(8)?,
                tags: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    match format {
        ExportFormat::Json => Ok(serde_json::to_string_pretty(&records).unwrap_or_default()),
        ExportFormat::Jsonl => {
            Ok(records.iter().map(|r| serde_json::to_string(r).unwrap_or_default()).collect::<Vec<_>>().join("\n"))
        }
        ExportFormat::Csv => {
            let mut out = String::from("id,command,cwd,exit_code,duration_ms,executor,timestamp,bookmarked,note,tags\n");
            for r in &records {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{}\n",
                    csv_escape(&r.id), csv_escape(&r.command), csv_escape(&r.cwd),
                    r.exit_code, r.duration_ms, csv_escape(&r.executor),
                    r.timestamp, r.bookmarked,
                    csv_escape(&r.note.clone().unwrap_or_default()),
                    csv_escape(&r.tags.clone().unwrap_or_default()),
                ));
            }
            Ok(out)
        }
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportRecord {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub executor: String,
    pub timestamp: i64,
    pub bookmarked: bool,
    pub note: Option<String>,
    pub tags: Option<String>,
}

pub enum ExportFormat {
    Json,
    Jsonl,
    Csv,
}
```

- [ ] **Step 2: Wire up in builtins.rs**

Implement `history_export()` to parse `--format` arg and call `store.export_all()`.

- [ ] **Step 3: Test manually**

Run: `history export --format json | head -20`
Expected: JSON array of redacted command records

- [ ] **Step 4: Commit**

```bash
git add crates/colony-history/ crates/colony-exec/
git commit -m "feat(history): implement history export in json, jsonl, and csv formats"
```

### Task 15: Upgrade colony-history-bin panel

**Files:**
- Modify: `crates/colony-history-bin/Cargo.toml` (add colony-history dep)
- Modify: `crates/colony-history-bin/src/main.rs` (rewrite to use HistoryStore)

- [ ] **Step 1: Add colony-history dependency**

Add `colony-history = { workspace = true }` to `crates/colony-history-bin/Cargo.toml`.

- [ ] **Step 2: Rewrite main.rs to read from SQLite**

Replace `load_history()` (which reads the flat file) with `HistoryStore::open()`. On launch, query the most recent 100 commands. On `cmd_complete` tuple, query the latest record.

- [ ] **Step 3: Add browse mode rendering**

Each row shows: command (truncated), exit code (red if non-zero), duration, executor badge, relative timestamp. Support key bindings: `/` search, `b` bookmark, `a` agent filter, `d` directory filter.

- [ ] **Step 4: Add stats mode rendering**

Tab toggles to stats mode. Render top commands bar chart, executor breakdown, directory counts using ratatui widgets.

- [ ] **Step 5: Test manually**

Run the panel alongside the shell. Execute commands and verify they appear live. Toggle between browse and stats mode.

- [ ] **Step 6: Commit**

```bash
git add crates/colony-history-bin/
git commit -m "feat(history): upgrade history panel with browse/stats modes and SQLite backend"
```

---

## Chunk 7: Final Integration and Cleanup

### Task 16: Add configuration for history exclusions and agent detection

**Files:**
- Modify: `crates/colony-history/src/privacy.rs` (load config)
- Modify: `crates/colony-history/src/executor.rs` (load config)

- [ ] **Step 1: Define config structure**

Expected `~/.colony/config.toml` sections:

```toml
[history.exclude]
patterns = [
    "^export.*PASSWORD",
    "^set.*SECRET",
]

[history.agent_detection]
patterns = [
    { env_var = "CLAUDE_CODE_ENTRYPOINT", name = "claude-code" },
    { env_var = "CODEX", name = "codex" },
]
```

- [ ] **Step 2: Load exclusion patterns from config in PrivacyFilter**

Add a `from_config()` constructor that reads `~/.colony/config.toml` and compiles the regex patterns.

- [ ] **Step 3: Load agent patterns from config in ExecutorDetector**

Add a `from_config()` constructor that reads `~/.colony/config.toml` agent detection patterns, merged with defaults.

- [ ] **Step 4: Test with config file**

Create a test config, verify exclusion patterns work, verify custom agent detection works.

- [ ] **Step 5: Commit**

```bash
git add crates/colony-history/
git commit -m "feat(history): add configurable exclusion patterns and agent detection"
```

### Task 17: End-to-end integration test

**Files:**
- Create: `crates/colony-history/tests/integration.rs`

- [ ] **Step 1: Write integration test**

```rust
use colony_history::*;
use tempfile::TempDir;
use std::sync::Arc;

#[test]
fn test_full_pipeline() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let store = Arc::new(HistoryStore::open(&db_path).unwrap());

    // Start background writer
    let writer = HistoryWriter::start(store.clone());

    // Privacy filter
    let filter = PrivacyFilter::new(vec![]);

    // Record some commands
    let commands = vec![
        ("cargo build --release", "/project", 0, 5000),
        ("cargo test", "/project", 0, 12000),
        (" secret-command", "/project", 0, 100), // space-prefix, should be skipped
        ("ls -la", "/home", 0, 10),
    ];

    for (cmd, cwd, exit_code, duration) in &commands {
        if filter.should_record(cmd) == PrivacyAction::Skip {
            continue;
        }
        let record = HistoryRecord::builder()
            .command(*cmd)
            .cwd(*cwd)
            .exit_code(*exit_code)
            .duration_ms(*duration as u64)
            .session_id("test-session")
            .build()
            .unwrap();
        writer.send(record);
    }

    // Drop joins the thread — all records flushed deterministically
    drop(writer);

    // Verify: 3 commands recorded (space-prefix skipped)
    assert_eq!(store.count().unwrap(), 3);

    // Search
    let results = store.search_fts("cargo", 50).unwrap();
    assert_eq!(results.len(), 2);

    // Stats
    let stats = HistoryStats::compute(&store).unwrap();
    assert_eq!(stats.total_commands, 3);
}
```

- [ ] **Step 2: Run integration test**

Run: `cargo test -p colony-history --test integration`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/colony-history/tests/
git commit -m "test(history): add end-to-end integration test for recording pipeline"
```

### Task 18: Update colony-reflection to enable WAL

**Files:**
- Modify: `crates/colony-reflection/src/storage.rs` (line 49)

- [ ] **Step 1: Add WAL pragma**

After `Connection::open(db_path)?` in `ObservationStore::open()`, add:

```rust
conn.execute_batch("PRAGMA journal_mode=WAL;")?;
```

- [ ] **Step 2: Verify existing tests still pass**

Run: `cargo test -p colony-reflection`
Expected: all tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/colony-reflection/src/storage.rs
git commit -m "fix(reflection): enable WAL journal mode for shared DB access"
```
