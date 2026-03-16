//! SQLite-backed storage for observations.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use thiserror::Error;

use crate::{Observation, ObservationCategory, SessionContext};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during observation storage operations.
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenience alias for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

// ---------------------------------------------------------------------------
// ObservationStore
// ---------------------------------------------------------------------------

/// Persistent SQLite store for shell observations.
pub struct ObservationStore {
    conn: Mutex<Connection>,
    db_path: PathBuf,
}

impl ObservationStore {
    /// Open (or create) an observation database at `db_path`.
    ///
    /// Creates parent directories if they do not exist and initialises the
    /// schema on the newly opened connection.
    pub fn open(db_path: &Path) -> StorageResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        // Enable WAL mode for concurrent reads (e.g., panel binaries reading
        // while the shell process writes).  WAL is sticky per-database, so
        // whichever crate opens first sets it.
        let _: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;

        let store = Self {
            conn: Mutex::new(conn),
            db_path: db_path.to_path_buf(),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Open an in-memory database — useful for tests.
    pub fn open_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
            db_path: PathBuf::from(":memory:"),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Default filesystem path for the observation database.
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".colony/reflection/observations.db")
    }

    /// Getter for the database path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Create the observations table and indices if they do not exist.
    fn init_schema(&self) -> StorageResult<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
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

    /// Insert a single observation into the store.
    pub fn insert(&self, obs: &Observation) -> StorageResult<()> {
        let data_json = serde_json::to_string(&obs.data)?;
        let category_str = obs.category.as_str();
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "INSERT INTO observations (
                id, timestamp, category, data, session_id, cwd, git_branch,
                command_count, time_since_last_ms, time_of_day, day_of_week,
                project, focus_duration_ms, energy_level
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                obs.id,
                obs.timestamp,
                category_str,
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
        Ok(())
    }

    /// Count observations, optionally filtered by category.
    pub fn count(&self, category: Option<ObservationCategory>) -> StorageResult<u64> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let count: u64 = match category {
            Some(cat) => conn.query_row(
                "SELECT COUNT(*) FROM observations WHERE category = ?1",
                rusqlite::params![cat.as_str()],
                |row| row.get(0),
            )?,
            None => conn.query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))?,
        };
        Ok(count)
    }

    /// Query all observations for a given session, ordered by timestamp.
    pub fn by_session(&self, session_id: &str) -> StorageResult<Vec<Observation>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, category, data, session_id, cwd, git_branch,
                    command_count, time_since_last_ms, time_of_day, day_of_week,
                    project, focus_duration_ms, energy_level
             FROM observations
             WHERE session_id = ?1
             ORDER BY timestamp",
        )?;

        let rows = stmt.query_map(rusqlite::params![session_id], |row| {
            self.row_to_observation_sql(row)
        })?;

        let mut results = Vec::new();
        for row in rows {
            let obs = row?;
            results.push(obs);
        }
        Ok(results)
    }

    /// Deserialize a single database row into an [`Observation`].
    ///
    /// Returns `rusqlite::Result` so it integrates with `query_map` closures.
    fn row_to_observation_sql(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<Observation> {
        let id: String = row.get(0)?;
        let timestamp: u64 = row.get(1)?;
        let category_str: String = row.get(2)?;
        let data_str: String = row.get(3)?;
        let session_id: String = row.get(4)?;
        let cwd: Option<String> = row.get(5)?;
        let git_branch: Option<String> = row.get(6)?;
        let command_count: Option<u64> = row.get(7)?;
        let time_since_last_ms: Option<u64> = row.get(8)?;
        let time_of_day: Option<String> = row.get(9)?;
        let day_of_week: Option<String> = row.get(10)?;
        let project: Option<String> = row.get(11)?;
        let focus_duration_ms: Option<u64> = row.get(12)?;
        let energy_level: Option<f64> = row.get(13)?;

        let category = match category_str.as_str() {
            "command" => ObservationCategory::Command,
            "error" => ObservationCategory::Error,
            "agent_chat" => ObservationCategory::AgentChat,
            "file_activity" => ObservationCategory::FileActivity,
            "context_usage" => ObservationCategory::ContextUsage,
            other => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::from(format!("unknown category: {other}")),
                ));
            }
        };

        let data: serde_json::Value =
            serde_json::from_str(&data_str).unwrap_or(serde_json::Value::Null);

        Ok(Observation {
            id,
            timestamp,
            category,
            data,
            context: SessionContext {
                session_id,
                cwd: cwd.unwrap_or_default(),
                git_branch,
                command_count: command_count.unwrap_or(0),
                time_since_last_ms: time_since_last_ms.unwrap_or(0),
                time_of_day: time_of_day.unwrap_or_default(),
                day_of_week: day_of_week.unwrap_or_default(),
                project,
                focus_duration_ms: focus_duration_ms.unwrap_or(0),
                energy_level: energy_level.unwrap_or(1.0),
            },
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandData;

    /// Build a minimal test observation.
    fn make_obs(category: ObservationCategory, session_id: &str) -> Observation {
        Observation {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: 1_700_000_000,
            category,
            data: serde_json::json!({"test": true}),
            context: SessionContext {
                session_id: session_id.to_string(),
                cwd: "/tmp".to_string(),
                git_branch: None,
                command_count: 1,
                time_since_last_ms: 0,
                time_of_day: "morning".to_string(),
                day_of_week: "monday".to_string(),
                project: None,
                focus_duration_ms: 1000,
                energy_level: 1.0,
            },
        }
    }

    #[test]
    fn test_open_in_memory() {
        let store = ObservationStore::open_in_memory().expect("open in-memory");
        let count = store.count(None).expect("count");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_and_count() {
        let store = ObservationStore::open_in_memory().expect("open in-memory");

        let cmd_data = CommandData {
            input: "ls -la".to_string(),
            exit_code: 0,
            duration_ms: 42,
            pipeline_length: 1,
        };
        let obs = Observation {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: 1_700_000_000,
            category: ObservationCategory::Command,
            data: serde_json::to_value(&cmd_data).expect("serialize"),
            context: SessionContext {
                session_id: "sess-1".to_string(),
                cwd: "/tmp".to_string(),
                git_branch: None,
                command_count: 1,
                time_since_last_ms: 0,
                time_of_day: "morning".to_string(),
                day_of_week: "monday".to_string(),
                project: None,
                focus_duration_ms: 1000,
                energy_level: 1.0,
            },
        };

        store.insert(&obs).expect("insert");
        assert_eq!(store.count(None).expect("total count"), 1);
        assert_eq!(
            store
                .count(Some(ObservationCategory::Command))
                .expect("cmd count"),
            1
        );
        assert_eq!(
            store
                .count(Some(ObservationCategory::Error))
                .expect("err count"),
            0
        );
    }

    #[test]
    fn test_query_by_session() {
        let store = ObservationStore::open_in_memory().expect("open in-memory");

        let obs = make_obs(ObservationCategory::Command, "sess-a");
        store.insert(&obs).expect("insert");

        let found = store.by_session("sess-a").expect("query");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].context.session_id, "sess-a");

        let empty = store.by_session("sess-z").expect("query missing");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_insert_all_categories() {
        let store = ObservationStore::open_in_memory().expect("open in-memory");

        let categories = [
            ObservationCategory::Command,
            ObservationCategory::Error,
            ObservationCategory::AgentChat,
            ObservationCategory::FileActivity,
            ObservationCategory::ContextUsage,
        ];

        for cat in &categories {
            let obs = make_obs(cat.clone(), "sess-all");
            store.insert(&obs).expect("insert");
        }

        assert_eq!(store.count(None).expect("total"), 5);
    }
}
