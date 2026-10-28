//! SQLite-backed storage for heartbeat runs.
//!
//! Uses the shared reflection database (`~/.colony/reflection/observations.db`)
//! with a dedicated `heartbeat_runs` table. Creates the table on first use
//! via `CREATE TABLE IF NOT EXISTS`. Prunes runs older than 7 days on each
//! insert to keep the database lean.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

use super::types::{Finding, HeartbeatError, HeartbeatRun, HeartbeatStatus};

/// Default retention period in days.
const DEFAULT_RETENTION_DAYS: i64 = 7;

/// SQLite store for heartbeat run records.
pub struct HeartbeatStore {
    conn: Mutex<Connection>,
}

impl HeartbeatStore {
    /// Open (or create) the heartbeat store at the given database path.
    ///
    /// Creates parent directories and the `heartbeat_runs` table if they
    /// do not already exist. Enables WAL mode for concurrent readers.
    pub fn open(db_path: &Path) -> Result<Self, HeartbeatError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| HeartbeatError::Database(format!("create dirs: {e}")))?;
        }

        let conn = Connection::open(db_path)
            .map_err(|e| HeartbeatError::Database(format!("open db: {e}")))?;

        // WAL mode for concurrent reads (sticky per-database)
        let _: String = conn
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
            .map_err(|e| HeartbeatError::Database(format!("WAL mode: {e}")))?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Open an in-memory database for testing.
    pub fn open_in_memory() -> Result<Self, HeartbeatError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| HeartbeatError::Database(format!("open in-memory: {e}")))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Default filesystem path for the shared reflection database.
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".colony/reflection/observations.db")
    }

    /// Create the heartbeat_runs table and indices if they do not exist.
    fn init_schema(&self) -> Result<(), HeartbeatError> {
        let conn = self.conn.lock().expect("heartbeat store mutex poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS heartbeat_runs (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                status TEXT NOT NULL,
                findings_json TEXT NOT NULL,
                raw_output TEXT NOT NULL,
                checklist_path TEXT NOT NULL,
                interval_secs INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_heartbeat_timestamp
                ON heartbeat_runs(timestamp DESC);",
        )
        .map_err(|e| HeartbeatError::Database(format!("init schema: {e}")))?;
        Ok(())
    }

    /// Insert a heartbeat run and prune old records.
    pub fn insert_run(&self, run: &HeartbeatRun) -> Result<(), HeartbeatError> {
        let findings_json = serde_json::to_string(&run.findings)
            .map_err(|e| HeartbeatError::Database(format!("serialize findings: {e}")))?;

        let conn = self.conn.lock().expect("heartbeat store mutex poisoned");
        conn.execute(
            "INSERT INTO heartbeat_runs
                (id, timestamp, duration_ms, status, findings_json, raw_output, checklist_path, interval_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                run.id,
                run.timestamp,
                run.duration_ms,
                run.status.to_string(),
                findings_json,
                run.raw_output,
                run.checklist_path,
                run.interval_secs,
            ],
        )
        .map_err(|e| HeartbeatError::Database(format!("insert run: {e}")))?;

        // Prune old runs within the same lock
        Self::prune_locked(&conn, DEFAULT_RETENTION_DAYS)?;

        Ok(())
    }

    /// Retrieve the most recent runs, ordered newest first.
    pub fn recent_runs(&self, limit: usize) -> Result<Vec<HeartbeatRun>, HeartbeatError> {
        let conn = self.conn.lock().expect("heartbeat store mutex poisoned");
        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, duration_ms, status, findings_json, raw_output, checklist_path, interval_secs
                 FROM heartbeat_runs
                 ORDER BY timestamp DESC
                 LIMIT ?1",
            )
            .map_err(|e| HeartbeatError::Database(format!("prepare recent: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![limit as i64], Self::row_to_run)
            .map_err(|e| HeartbeatError::Database(format!("query recent: {e}")))?;

        let mut runs = Vec::new();
        for row in rows {
            let run = row.map_err(|e| HeartbeatError::Database(format!("read row: {e}")))?;
            runs.push(run);
        }
        Ok(runs)
    }

    /// Retrieve a specific run by ID.
    pub fn get_run(&self, run_id: &str) -> Result<Option<HeartbeatRun>, HeartbeatError> {
        let conn = self.conn.lock().expect("heartbeat store mutex poisoned");
        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, duration_ms, status, findings_json, raw_output, checklist_path, interval_secs
                 FROM heartbeat_runs
                 WHERE id = ?1",
            )
            .map_err(|e| HeartbeatError::Database(format!("prepare get: {e}")))?;

        let mut rows = stmt
            .query_map(rusqlite::params![run_id], Self::row_to_run)
            .map_err(|e| HeartbeatError::Database(format!("query get: {e}")))?;

        match rows.next() {
            Some(Ok(run)) => Ok(Some(run)),
            Some(Err(e)) => Err(HeartbeatError::Database(format!("read row: {e}"))),
            None => Ok(None),
        }
    }

    /// Prune runs older than the specified number of days.
    pub fn prune_old(&self, days: i64) -> Result<usize, HeartbeatError> {
        let conn = self.conn.lock().expect("heartbeat store mutex poisoned");
        Self::prune_locked(&conn, days)
    }

    /// Internal prune that operates on an already-locked connection.
    fn prune_locked(conn: &Connection, days: i64) -> Result<usize, HeartbeatError> {
        let cutoff = chrono::Utc::now().timestamp() - (days * 86400);
        let deleted = conn
            .execute(
                "DELETE FROM heartbeat_runs WHERE timestamp < ?1",
                rusqlite::params![cutoff],
            )
            .map_err(|e| HeartbeatError::Database(format!("prune: {e}")))?;
        Ok(deleted)
    }

    /// Convert a database row to a `HeartbeatRun`.
    fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<HeartbeatRun> {
        let id: String = row.get(0)?;
        let timestamp: i64 = row.get(1)?;
        let duration_ms: u64 = row.get(2)?;
        let status_str: String = row.get(3)?;
        let findings_json: String = row.get(4)?;
        let raw_output: String = row.get(5)?;
        let checklist_path: String = row.get(6)?;
        let interval_secs: u64 = row.get(7)?;

        let status = HeartbeatStatus::from_str_lossy(&status_str);
        let findings: Vec<Finding> = serde_json::from_str(&findings_json).unwrap_or_default();

        Ok(HeartbeatRun {
            id,
            timestamp,
            duration_ms,
            status,
            findings,
            raw_output,
            checklist_path,
            interval_secs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heartbeat::types::{Finding, Severity};

    fn make_run(id: &str, status: HeartbeatStatus) -> HeartbeatRun {
        HeartbeatRun {
            id: id.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            duration_ms: 1234,
            status,
            findings: vec![Finding {
                severity: Severity::Pass,
                message: "Tests passing".to_string(),
            }],
            raw_output: "PASS: Tests passing (42/42)".to_string(),
            checklist_path: "/tmp/HEARTBEAT.md".to_string(),
            interval_secs: 1800,
        }
    }

    #[test]
    fn test_open_in_memory() {
        let store = HeartbeatStore::open_in_memory().expect("open in-memory");
        let runs = store.recent_runs(10).expect("recent");
        assert!(runs.is_empty());
    }

    #[test]
    fn test_insert_and_retrieve() {
        let store = HeartbeatStore::open_in_memory().expect("open in-memory");
        let run = make_run("run-1", HeartbeatStatus::Pass);
        store.insert_run(&run).expect("insert");

        let runs = store.recent_runs(10).expect("recent");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, "run-1");
        assert_eq!(runs[0].status, HeartbeatStatus::Pass);
        assert_eq!(runs[0].findings.len(), 1);
    }

    #[test]
    fn test_get_run_by_id() {
        let store = HeartbeatStore::open_in_memory().expect("open in-memory");
        let run = make_run("run-abc", HeartbeatStatus::Warn);
        store.insert_run(&run).expect("insert");

        let found = store.get_run("run-abc").expect("get").expect("found");
        assert_eq!(found.id, "run-abc");
        assert_eq!(found.status, HeartbeatStatus::Warn);

        let missing = store.get_run("nonexistent").expect("get");
        assert!(missing.is_none());
    }

    #[test]
    fn test_recent_runs_ordering() {
        let store = HeartbeatStore::open_in_memory().expect("open in-memory");
        let now = chrono::Utc::now().timestamp();

        let mut run1 = make_run("run-1", HeartbeatStatus::Pass);
        run1.timestamp = now - 60;
        store.insert_run(&run1).expect("insert 1");

        let mut run2 = make_run("run-2", HeartbeatStatus::Alert);
        run2.timestamp = now - 30;
        store.insert_run(&run2).expect("insert 2");

        let runs = store.recent_runs(10).expect("recent");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].id, "run-2"); // most recent first
        assert_eq!(runs[1].id, "run-1");
    }

    #[test]
    fn test_recent_runs_limit() {
        let store = HeartbeatStore::open_in_memory().expect("open in-memory");
        let now = chrono::Utc::now().timestamp();

        for i in 0..5 {
            let mut run = make_run(&format!("run-{i}"), HeartbeatStatus::Pass);
            run.timestamp = now - (5 - i as i64); // all recent
            store.insert_run(&run).expect("insert");
        }

        let runs = store.recent_runs(3).expect("recent");
        assert_eq!(runs.len(), 3);
    }

    #[test]
    fn test_prune_old() {
        let store = HeartbeatStore::open_in_memory().expect("open in-memory");
        let now = chrono::Utc::now().timestamp();

        // Insert a very old run (10 days ago)
        let mut old_run = make_run("old", HeartbeatStatus::Pass);
        old_run.timestamp = now - (10 * 86400);
        store.insert_run(&old_run).expect("insert old");
        // The auto-prune on insert already removed it (7-day retention),
        // but let's verify manually:

        // Insert a recent run
        let recent_run = make_run("recent", HeartbeatStatus::Pass);
        store.insert_run(&recent_run).expect("insert recent");

        let runs = store.recent_runs(10).expect("recent");
        // Old run was pruned, only recent remains
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, "recent");
    }
}
