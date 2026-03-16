//! SQLite-backed storage for command history.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::record::HistoryRecord;
use crate::search::SearchQuery;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during history store operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid pattern: {0}")]
    InvalidPattern(String),
}

/// Convenience alias for store operations.
pub type StoreResult<T> = Result<T, StoreError>;

// ---------------------------------------------------------------------------
// SearchResult
// ---------------------------------------------------------------------------

/// A single search result row.
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

// ---------------------------------------------------------------------------
// Export types
// ---------------------------------------------------------------------------

/// Supported export formats.
pub enum ExportFormat {
    Json,
    Jsonl,
    Csv,
}

/// A record suitable for export (uses redacted command text).
#[derive(Debug, Clone, Serialize)]
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

/// Summary of commands executed by a single agent.
#[derive(Debug, Clone)]
pub struct AgentSummary {
    /// Agent executor name.
    pub executor: String,
    /// Total command count for this agent.
    pub total: u64,
    /// Risk level breakdown: (risk_level, count).
    pub risk_breakdown: Vec<(String, u64)>,
}

// ---------------------------------------------------------------------------
// HistoryStore
// ---------------------------------------------------------------------------

/// Persistent SQLite store for shell command history.
pub struct HistoryStore {
    conn: Mutex<Connection>,
    db_path: PathBuf,
}

impl HistoryStore {
    /// Open (or create) a history database at `db_path`.
    ///
    /// Creates parent directories if needed, enables WAL mode, sets file
    /// permissions to 0o600, and runs migrations.
    pub fn open(db_path: &Path) -> StoreResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        // PRAGMA journal_mode returns a result row, so we use
        // pragma_update_and_check which handles the return value.
        let _: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;

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

    /// Default filesystem path for the history database.
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".colony/reflection/observations.db")
    }

    /// Getter for the database path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Apply schema migrations.
    ///
    /// Executes DDL statements individually because `execute_batch`
    /// cannot handle FTS5 virtual table creation (which internally
    /// returns results).
    fn run_migrations(&self) -> StoreResult<()> {
        let conn = self.conn.lock().expect("lock poisoned");

        // Main table
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS command_history (
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
            CREATE INDEX IF NOT EXISTS idx_history_session ON command_history(session_id);",
        )?;

        // FTS5 virtual table -- must be executed separately because
        // CREATE VIRTUAL TABLE internally returns results which
        // execute_batch cannot handle.
        // Check if FTS table already exists before creating it.
        let fts_exists: bool = conn
            .prepare(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='command_history_fts'",
            )?
            .query_row([], |row| row.get::<_, i64>(0))
            .map(|c| c > 0)?;

        if !fts_exists {
            conn.execute(
                "CREATE VIRTUAL TABLE command_history_fts USING fts5(
                    command,
                    content=command_history,
                    content_rowid=rowid
                )",
                [],
            )?;
        }

        // Triggers to keep FTS index in sync
        conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS command_history_ai AFTER INSERT ON command_history BEGIN
                INSERT INTO command_history_fts(rowid, command) VALUES (new.rowid, new.command);
            END;

            CREATE TRIGGER IF NOT EXISTS command_history_ad AFTER DELETE ON command_history BEGIN
                INSERT INTO command_history_fts(command_history_fts, rowid, command) VALUES('delete', old.rowid, old.command);
            END;

            CREATE TRIGGER IF NOT EXISTS command_history_au AFTER UPDATE ON command_history BEGIN
                INSERT INTO command_history_fts(command_history_fts, rowid, command) VALUES('delete', old.rowid, old.command);
                INSERT INTO command_history_fts(rowid, command) VALUES (new.rowid, new.command);
            END;",
        )?;

        // Migration tracking table
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _colony_history_migrations (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            );",
        )?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Write operations
    // -----------------------------------------------------------------------

    /// Insert a single history record.
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

    // -----------------------------------------------------------------------
    // Read operations
    // -----------------------------------------------------------------------

    /// Count all records in the store.
    pub fn count(&self) -> StoreResult<u64> {
        let conn = self.conn.lock().expect("lock poisoned");
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM command_history", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Count records with timestamp >= the given value.
    pub fn count_since(&self, timestamp: i64) -> StoreResult<u64> {
        let conn = self.conn.lock().expect("lock poisoned");
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM command_history WHERE timestamp >= ?1",
            params![timestamp],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// Return the earliest timestamp across all records.
    pub fn earliest_timestamp(&self) -> StoreResult<Option<i64>> {
        let conn = self.conn.lock().expect("lock poisoned");
        let ts: Option<i64> =
            conn.query_row("SELECT MIN(timestamp) FROM command_history", [], |row| {
                row.get(0)
            })?;
        Ok(ts)
    }

    /// Full-text search using FTS5.
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

    /// Advanced search with optional filters.
    pub fn search_advanced(&self, query: &SearchQuery) -> StoreResult<Vec<SearchResult>> {
        let conn = self.conn.lock().expect("lock poisoned");

        let mut sql = String::from(
            "SELECT h.id, h.command_redacted, h.cwd, h.executor, h.exit_code,
                    h.duration_ms, h.timestamp, h.bookmarked
             FROM command_history_fts f
             JOIN command_history h ON h.rowid = f.rowid
             WHERE command_history_fts MATCH ?1
               AND h.hidden = 0",
        );
        let mut param_idx = 2u32;

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
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(query.text.clone())];
        if let Some(ref cwd) = query.cwd {
            params_vec.push(Box::new(cwd.clone()));
        }
        if let Some(ref executor) = query.executor {
            params_vec.push(Box::new(executor.clone()));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

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

    // -----------------------------------------------------------------------
    // Aggregation queries
    // -----------------------------------------------------------------------

    /// Count commands grouped by executor.
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

    /// Top directories by command count.
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

    /// Command counts grouped by hour of day (local time).
    pub fn hourly_distribution(&self) -> StoreResult<Vec<(u32, u64)>> {
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

    /// Agent command summary grouped by executor with risk breakdown.
    pub fn agent_summary(&self) -> StoreResult<Vec<AgentSummary>> {
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

        let mut result: Vec<AgentSummary> = Vec::new();
        for (executor, risk, count) in &rows {
            let risk_str = risk.as_deref().unwrap_or("unknown").to_string();
            if let Some(entry) = result.iter_mut().find(|s| s.executor == *executor) {
                entry.total += count;
                entry.risk_breakdown.push((risk_str, *count));
            } else {
                result.push(AgentSummary {
                    executor: executor.clone(),
                    total: *count,
                    risk_breakdown: vec![(risk_str, *count)],
                });
            }
        }
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Mutation operations
    // -----------------------------------------------------------------------

    /// Delete all records older than the given timestamp.
    pub fn delete_before(&self, timestamp: i64) -> StoreResult<u64> {
        let conn = self.conn.lock().expect("lock poisoned");
        let count = conn.execute(
            "DELETE FROM command_history WHERE timestamp < ?1",
            params![timestamp],
        )?;
        Ok(count as u64)
    }

    /// Delete records whose command matches a regex pattern.
    ///
    /// SQLite has no built-in REGEXP -- we match in Rust and delete by
    /// collected IDs.
    pub fn delete_by_pattern(&self, pattern: &str) -> StoreResult<u64> {
        let re =
            regex::Regex::new(pattern).map_err(|e| StoreError::InvalidPattern(e.to_string()))?;

        let conn = self.conn.lock().expect("lock poisoned");
        let mut stmt = conn.prepare("SELECT id, command FROM command_history")?;
        let ids: Vec<String> = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let cmd: String = row.get(1)?;
                Ok((id, cmd))
            })?
            .filter_map(Result::ok)
            .filter(|(_, cmd)| re.is_match(cmd))
            .map(|(id, _)| id)
            .collect();

        let mut deleted = 0u64;
        for id in &ids {
            deleted +=
                conn.execute("DELETE FROM command_history WHERE id = ?1", params![id])? as u64;
        }
        Ok(deleted)
    }

    /// Toggle the bookmark flag on a record. Returns the new state.
    pub fn toggle_bookmark(&self, id: &str) -> StoreResult<bool> {
        let conn = self.conn.lock().expect("lock poisoned");
        let current: i32 = conn.query_row(
            "SELECT bookmarked FROM command_history WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        let new_val = i32::from(current == 0);
        conn.execute(
            "UPDATE command_history SET bookmarked = ?1 WHERE id = ?2",
            params![new_val, id],
        )?;
        Ok(new_val == 1)
    }

    /// Set a note on a record.
    pub fn set_note(&self, id: &str, note: &str) -> StoreResult<()> {
        let conn = self.conn.lock().expect("lock poisoned");
        conn.execute(
            "UPDATE command_history SET note = ?1 WHERE id = ?2",
            params![note, id],
        )?;
        Ok(())
    }

    /// Set tags on a record (comma-separated string).
    pub fn set_tags(&self, id: &str, tags: &str) -> StoreResult<()> {
        let conn = self.conn.lock().expect("lock poisoned");
        conn.execute(
            "UPDATE command_history SET tags = ?1 WHERE id = ?2",
            params![tags, id],
        )?;
        Ok(())
    }

    /// Return the current journal mode.
    pub fn journal_mode(&self) -> StoreResult<String> {
        let conn = self.conn.lock().expect("lock poisoned");
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        Ok(mode)
    }

    // -----------------------------------------------------------------------
    // Export
    // -----------------------------------------------------------------------

    /// Export all records in the specified format, using redacted command text.
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
            ExportFormat::Jsonl => Ok(records
                .iter()
                .map(|r| serde_json::to_string(r).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n")),
            ExportFormat::Csv => {
                let mut out = String::from(
                    "id,command,cwd,exit_code,duration_ms,executor,timestamp,bookmarked,note,tags\n",
                );
                for r in &records {
                    out.push_str(&format!(
                        "{},{},{},{},{},{},{},{},{},{}\n",
                        csv_escape(&r.id),
                        csv_escape(&r.command),
                        csv_escape(&r.cwd),
                        r.exit_code,
                        r.duration_ms,
                        csv_escape(&r.executor),
                        r.timestamp,
                        r.bookmarked,
                        csv_escape(&r.note.clone().unwrap_or_default()),
                        csv_escape(&r.tags.clone().unwrap_or_default()),
                    ));
                }
                Ok(out)
            }
        }
    }
}

/// Escape a string value for CSV output.
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::HistoryRecord;
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
        let count = store.count().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_and_count() {
        let (store, _dir) = test_store();
        let record = sample_record("ls -la");
        store.insert(&record).unwrap();

        let count = store.count().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_search_fts() {
        let (store, _dir) = test_store();
        store
            .insert(&sample_record("cargo build --release"))
            .unwrap();
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

    #[test]
    fn test_count_since() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("ls")).unwrap();

        let count = store.count_since(0).unwrap();
        assert_eq!(count, 1);

        // Far-future timestamp should yield zero
        let count = store.count_since(i64::MAX - 1000).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_earliest_timestamp_empty() {
        let (store, _dir) = test_store();
        let ts = store.earliest_timestamp().unwrap();
        assert!(ts.is_none());
    }

    #[test]
    fn test_earliest_timestamp_with_data() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("ls")).unwrap();
        let ts = store.earliest_timestamp().unwrap();
        assert!(ts.is_some());
    }

    #[test]
    fn test_executor_counts() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("ls")).unwrap();
        store.insert(&sample_record("pwd")).unwrap();

        let counts = store.executor_counts().unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].0, "user");
        assert_eq!(counts[0].1, 2);
    }

    #[test]
    fn test_top_directories() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("ls")).unwrap();

        let dirs = store.top_directories(5).unwrap();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].0, "/tmp");
    }

    #[test]
    fn test_toggle_bookmark() {
        let (store, _dir) = test_store();
        let record = sample_record("ls");
        let id = record.id.clone();
        store.insert(&record).unwrap();

        let bookmarked = store.toggle_bookmark(&id).unwrap();
        assert!(bookmarked);

        let unbookmarked = store.toggle_bookmark(&id).unwrap();
        assert!(!unbookmarked);
    }

    #[test]
    fn test_set_note() {
        let (store, _dir) = test_store();
        let record = sample_record("ls");
        let id = record.id.clone();
        store.insert(&record).unwrap();

        store.set_note(&id, "important command").unwrap();
    }

    #[test]
    fn test_set_tags() {
        let (store, _dir) = test_store();
        let record = sample_record("ls");
        let id = record.id.clone();
        store.insert(&record).unwrap();

        store.set_tags(&id, "debug,test").unwrap();
    }

    #[test]
    fn test_delete_before() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("ls")).unwrap();
        store.insert(&sample_record("pwd")).unwrap();

        // Delete everything before far-future timestamp
        let deleted = store.delete_before(i64::MAX).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_delete_by_pattern() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("cargo build")).unwrap();
        store.insert(&sample_record("cargo test")).unwrap();
        store.insert(&sample_record("ls -la")).unwrap();

        let deleted = store.delete_by_pattern("^cargo").unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn test_export_json() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("echo hello")).unwrap();

        let json = store.export_all(ExportFormat::Json).unwrap();
        assert!(json.contains("echo hello"));
    }

    #[test]
    fn test_export_csv() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("echo hello")).unwrap();

        let csv = store.export_all(ExportFormat::Csv).unwrap();
        assert!(csv.starts_with("id,command,cwd"));
        assert!(csv.contains("echo hello"));
    }

    #[test]
    fn test_export_jsonl() {
        let (store, _dir) = test_store();
        store.insert(&sample_record("echo hello")).unwrap();
        store.insert(&sample_record("ls")).unwrap();

        let jsonl = store.export_all(ExportFormat::Jsonl).unwrap();
        let lines: Vec<&str> = jsonl.lines().collect();
        assert_eq!(lines.len(), 2);
    }
}
