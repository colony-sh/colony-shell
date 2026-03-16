//! SQLite-backed listener registry for the event router.
//!
//! Stores `event_listeners` and `event_handler_runs` in the shared reflection
//! database (`~/.colony/reflection/observations.db`). Provides CRUD operations
//! for listener management and execution logging.
//!
//! Uses WAL mode for concurrent access from the `on` builtin (writer) and the
//! `colony-events` daemon (reader).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Default retention period for handler runs in days.
const DEFAULT_RETENTION_DAYS: i64 = 7;

/// Errors from registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("listener not found: {0}")]
    NotFound(String),

    #[error("ambiguous prefix: {0} matches multiple listeners")]
    AmbiguousPrefix(String),
}

/// Type alias for registry results.
pub type RegistryResult<T> = Result<T, RegistryError>;

/// How a handler executes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandlerType {
    /// `colony-shell -c '<handler>'` with tuple env vars
    CommandEval,
    /// Execute script directly with tuple values as args
    Script,
    /// `colony-shell -c '<funcname> "$@"'` with exported function env
    Function,
}

impl HandlerType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::CommandEval => "command",
            Self::Script => "script",
            Self::Function => "function",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "script" => Self::Script,
            "function" => Self::Function,
            _ => Self::CommandEval,
        }
    }
}

impl std::fmt::Display for HandlerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A registered event listener.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventListener {
    pub id: String,
    pub pattern: String,
    pub handler: String,
    pub handler_type: HandlerType,
    pub filter: String,
    pub once: bool,
    pub enabled: bool,
    pub description: String,
    pub created_at: i64,
}

/// A record of a handler execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerRun {
    pub id: String,
    pub listener_id: String,
    pub pattern: String,
    pub tuple_values: Vec<String>,
    pub handler: String,
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
    pub stdout_snippet: Option<String>,
    pub stderr_snippet: Option<String>,
    pub timestamp: i64,
}

/// SQLite-backed event listener registry.
pub struct EventRegistry {
    conn: Mutex<Connection>,
    #[allow(dead_code)]
    db_path: PathBuf,
}

impl EventRegistry {
    /// Open (or create) the registry at the given database path.
    ///
    /// Creates parent directories and schema tables if they do not exist.
    /// Enables WAL mode for concurrent readers (daemon + shell).
    pub fn open(db_path: &Path) -> RegistryResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;

        // WAL mode for concurrent access (sticky per-database)
        let _: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;

        let registry = Self {
            conn: Mutex::new(conn),
            db_path: db_path.to_path_buf(),
        };
        registry.init_schema()?;
        Ok(registry)
    }

    /// Open an in-memory database for testing.
    pub fn open_in_memory() -> RegistryResult<Self> {
        let conn = Connection::open_in_memory()?;
        let registry = Self {
            conn: Mutex::new(conn),
            db_path: PathBuf::from(":memory:"),
        };
        registry.init_schema()?;
        Ok(registry)
    }

    /// Default filesystem path for the shared reflection database.
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".colony/reflection/observations.db")
    }

    /// Create the event_listeners and event_handler_runs tables if needed.
    fn init_schema(&self) -> RegistryResult<()> {
        let conn = self.conn.lock().expect("registry mutex poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS event_listeners (
                id TEXT PRIMARY KEY,
                pattern TEXT NOT NULL,
                handler TEXT NOT NULL,
                handler_type TEXT NOT NULL,
                filter TEXT NOT NULL DEFAULT '',
                once INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                description TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS event_handler_runs (
                id TEXT PRIMARY KEY,
                listener_id TEXT NOT NULL,
                pattern TEXT NOT NULL,
                tuple_values TEXT NOT NULL,
                handler TEXT NOT NULL,
                exit_code INTEGER,
                duration_ms INTEGER NOT NULL,
                stdout_snippet TEXT,
                stderr_snippet TEXT,
                timestamp INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_event_runs_timestamp
                ON event_handler_runs(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_event_runs_listener
                ON event_handler_runs(listener_id);",
        )?;
        Ok(())
    }

    /// Register a new event listener. Returns the generated UUID.
    pub fn add_listener(
        &self,
        pattern: &str,
        handler: &str,
        handler_type: HandlerType,
        filter: &str,
        once: bool,
        description: &str,
    ) -> RegistryResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now_timestamp();

        let conn = self.conn.lock().expect("registry mutex poisoned");
        conn.execute(
            "INSERT INTO event_listeners
                (id, pattern, handler, handler_type, filter, once, enabled, description, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8)",
            rusqlite::params![
                id,
                pattern,
                handler,
                handler_type.as_str(),
                filter,
                once as i32,
                description,
                created_at,
            ],
        )?;

        Ok(id)
    }

    /// Remove a listener by ID or unique prefix.
    pub fn remove_listener(&self, id_or_prefix: &str) -> RegistryResult<bool> {
        let resolved = self.resolve_id(id_or_prefix)?;
        let conn = self.conn.lock().expect("registry mutex poisoned");
        let count = conn.execute(
            "DELETE FROM event_listeners WHERE id = ?1",
            rusqlite::params![resolved],
        )?;
        Ok(count > 0)
    }

    /// Remove all listeners. Returns the number removed.
    pub fn remove_all(&self) -> RegistryResult<usize> {
        let conn = self.conn.lock().expect("registry mutex poisoned");
        let count = conn.execute("DELETE FROM event_listeners", [])?;
        Ok(count)
    }

    /// Enable a listener by ID or unique prefix.
    pub fn enable_listener(&self, id_or_prefix: &str) -> RegistryResult<bool> {
        let resolved = self.resolve_id(id_or_prefix)?;
        let conn = self.conn.lock().expect("registry mutex poisoned");
        let count = conn.execute(
            "UPDATE event_listeners SET enabled = 1 WHERE id = ?1",
            rusqlite::params![resolved],
        )?;
        Ok(count > 0)
    }

    /// Disable a listener by ID or unique prefix.
    pub fn disable_listener(&self, id_or_prefix: &str) -> RegistryResult<bool> {
        let resolved = self.resolve_id(id_or_prefix)?;
        let conn = self.conn.lock().expect("registry mutex poisoned");
        let count = conn.execute(
            "UPDATE event_listeners SET enabled = 0 WHERE id = ?1",
            rusqlite::params![resolved],
        )?;
        Ok(count > 0)
    }

    /// List all registered listeners.
    pub fn list_listeners(&self) -> RegistryResult<Vec<EventListener>> {
        let conn = self.conn.lock().expect("registry mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, pattern, handler, handler_type, filter, once, enabled, description, created_at
             FROM event_listeners
             ORDER BY created_at",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(EventListener {
                id: row.get(0)?,
                pattern: row.get(1)?,
                handler: row.get(2)?,
                handler_type: HandlerType::from_str_lossy(&row.get::<_, String>(3)?),
                filter: row.get(4)?,
                once: row.get::<_, i32>(5)? != 0,
                enabled: row.get::<_, i32>(6)? != 0,
                description: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let mut listeners = Vec::new();
        for row in rows {
            listeners.push(row?);
        }
        Ok(listeners)
    }

    /// Get only enabled listeners (used by the daemon).
    pub fn enabled_listeners(&self) -> RegistryResult<Vec<EventListener>> {
        let conn = self.conn.lock().expect("registry mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, pattern, handler, handler_type, filter, once, enabled, description, created_at
             FROM event_listeners
             WHERE enabled = 1
             ORDER BY created_at",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(EventListener {
                id: row.get(0)?,
                pattern: row.get(1)?,
                handler: row.get(2)?,
                handler_type: HandlerType::from_str_lossy(&row.get::<_, String>(3)?),
                filter: row.get(4)?,
                once: row.get::<_, i32>(5)? != 0,
                enabled: row.get::<_, i32>(6)? != 0,
                description: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let mut listeners = Vec::new();
        for row in rows {
            listeners.push(row?);
        }
        Ok(listeners)
    }

    /// Export all listeners as TOML.
    pub fn export_listeners(&self) -> RegistryResult<String> {
        let listeners = self.list_listeners()?;
        let mut output = String::from("# Colony Event Listeners\n# Generated by `on --export`\n\n");

        for listener in &listeners {
            output.push_str(&format!("[[listener]]\n"));
            output.push_str(&format!("pattern = {:?}\n", listener.pattern));
            output.push_str(&format!("handler = {:?}\n", listener.handler));
            output.push_str(&format!("handler_type = {:?}\n", listener.handler_type.as_str()));
            if !listener.filter.is_empty() {
                output.push_str(&format!("filter = {:?}\n", listener.filter));
            }
            if listener.once {
                output.push_str("once = true\n");
            }
            if !listener.enabled {
                output.push_str("enabled = false\n");
            }
            if !listener.description.is_empty() {
                output.push_str(&format!("description = {:?}\n", listener.description));
            }
            output.push('\n');
        }

        Ok(output)
    }

    /// Import listeners from TOML content. Returns the number imported.
    pub fn import_listeners(&self, toml_content: &str) -> RegistryResult<usize> {
        let parsed: toml::Value = toml::from_str(toml_content)
            .map_err(|e| RegistryError::Serialization(format!("invalid TOML: {e}")))?;

        let Some(listeners) = parsed.get("listener").and_then(|v| v.as_array()) else {
            return Ok(0);
        };

        let mut count = 0;
        for entry in listeners {
            let Some(pattern) = entry.get("pattern").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(handler) = entry.get("handler").and_then(|v| v.as_str()) else {
                continue;
            };

            let handler_type = entry
                .get("handler_type")
                .and_then(|v| v.as_str())
                .map(HandlerType::from_str_lossy)
                .unwrap_or(HandlerType::CommandEval);

            let filter = entry
                .get("filter")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let once = entry
                .get("once")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let description = entry
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            self.add_listener(pattern, handler, handler_type, filter, once, description)?;

            // If imported as disabled, disable it
            let enabled = entry
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !enabled {
                // The last inserted listener — get its ID
                let listeners = self.list_listeners()?;
                if let Some(last) = listeners.last() {
                    self.disable_listener(&last.id)?;
                }
            }

            count += 1;
        }

        Ok(count)
    }

    /// Log a handler execution.
    pub fn log_handler_run(&self, run: &HandlerRun) -> RegistryResult<()> {
        let tuple_values_json = serde_json::to_string(&run.tuple_values)
            .map_err(|e| RegistryError::Serialization(e.to_string()))?;

        let conn = self.conn.lock().expect("registry mutex poisoned");
        conn.execute(
            "INSERT INTO event_handler_runs
                (id, listener_id, pattern, tuple_values, handler, exit_code, duration_ms,
                 stdout_snippet, stderr_snippet, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                run.id,
                run.listener_id,
                run.pattern,
                tuple_values_json,
                run.handler,
                run.exit_code,
                run.duration_ms,
                run.stdout_snippet,
                run.stderr_snippet,
                run.timestamp,
            ],
        )?;

        Ok(())
    }

    /// Retrieve recent handler runs, newest first.
    pub fn recent_runs(&self, limit: usize) -> RegistryResult<Vec<HandlerRun>> {
        let conn = self.conn.lock().expect("registry mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, listener_id, pattern, tuple_values, handler, exit_code, duration_ms,
                    stdout_snippet, stderr_snippet, timestamp
             FROM event_handler_runs
             ORDER BY timestamp DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
            let tuple_values_json: String = row.get(3)?;
            let tuple_values: Vec<String> =
                serde_json::from_str(&tuple_values_json).unwrap_or_default();

            Ok(HandlerRun {
                id: row.get(0)?,
                listener_id: row.get(1)?,
                pattern: row.get(2)?,
                tuple_values,
                handler: row.get(4)?,
                exit_code: row.get(5)?,
                duration_ms: row.get(6)?,
                stdout_snippet: row.get(7)?,
                stderr_snippet: row.get(8)?,
                timestamp: row.get(9)?,
            })
        })?;

        let mut runs = Vec::new();
        for row in rows {
            runs.push(row?);
        }
        Ok(runs)
    }

    /// Prune handler runs older than the given number of days.
    pub fn prune_old_runs(&self, days: i64) -> RegistryResult<usize> {
        let cutoff = now_timestamp() - (days * 86400);
        let conn = self.conn.lock().expect("registry mutex poisoned");
        let count = conn.execute(
            "DELETE FROM event_handler_runs WHERE timestamp < ?1",
            rusqlite::params![cutoff],
        )?;
        Ok(count)
    }

    /// Prune runs using default retention (7 days).
    pub fn prune_default(&self) -> RegistryResult<usize> {
        self.prune_old_runs(DEFAULT_RETENTION_DAYS)
    }

    /// Resolve an ID or unique prefix to a full listener ID.
    fn resolve_id(&self, id_or_prefix: &str) -> RegistryResult<String> {
        let conn = self.conn.lock().expect("registry mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id FROM event_listeners WHERE id LIKE ?1 || '%'",
        )?;

        let mut rows = stmt.query(rusqlite::params![id_or_prefix])?;
        let mut matches = Vec::new();

        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            matches.push(id);
        }

        match matches.len() {
            0 => Err(RegistryError::NotFound(id_or_prefix.to_string())),
            1 => Ok(matches.into_iter().next().expect("len checked")),
            _ => Err(RegistryError::AmbiguousPrefix(id_or_prefix.to_string())),
        }
    }
}

/// Get current Unix timestamp in seconds.
fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs() as i64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let reg = EventRegistry::open_in_memory().expect("open in-memory");
        let listeners = reg.list_listeners().expect("list");
        assert!(listeners.is_empty());
    }

    #[test]
    fn test_add_and_list_listener() {
        let reg = EventRegistry::open_in_memory().expect("open");
        let id = reg
            .add_listener("http_request", "echo $2", HandlerType::CommandEval, "", false, "test listener")
            .expect("add");

        assert!(!id.is_empty());

        let listeners = reg.list_listeners().expect("list");
        assert_eq!(listeners.len(), 1);
        assert_eq!(listeners[0].pattern, "http_request");
        assert_eq!(listeners[0].handler, "echo $2");
        assert_eq!(listeners[0].handler_type, HandlerType::CommandEval);
        assert!(listeners[0].enabled);
        assert!(!listeners[0].once);
    }

    #[test]
    fn test_remove_listener() {
        let reg = EventRegistry::open_in_memory().expect("open");
        let id = reg
            .add_listener("test", "handler", HandlerType::CommandEval, "", false, "")
            .expect("add");

        assert!(reg.remove_listener(&id).expect("remove"));
        assert!(reg.list_listeners().expect("list").is_empty());
    }

    #[test]
    fn test_remove_by_prefix() {
        let reg = EventRegistry::open_in_memory().expect("open");
        let id = reg
            .add_listener("test", "handler", HandlerType::CommandEval, "", false, "")
            .expect("add");

        // Use first 8 chars as prefix
        let prefix = &id[..8];
        assert!(reg.remove_listener(prefix).expect("remove by prefix"));
        assert!(reg.list_listeners().expect("list").is_empty());
    }

    #[test]
    fn test_remove_all() {
        let reg = EventRegistry::open_in_memory().expect("open");
        reg.add_listener("a", "h1", HandlerType::CommandEval, "", false, "")
            .expect("add 1");
        reg.add_listener("b", "h2", HandlerType::CommandEval, "", false, "")
            .expect("add 2");

        let count = reg.remove_all().expect("remove all");
        assert_eq!(count, 2);
        assert!(reg.list_listeners().expect("list").is_empty());
    }

    #[test]
    fn test_enable_disable() {
        let reg = EventRegistry::open_in_memory().expect("open");
        let id = reg
            .add_listener("test", "handler", HandlerType::CommandEval, "", false, "")
            .expect("add");

        reg.disable_listener(&id).expect("disable");
        let listeners = reg.list_listeners().expect("list");
        assert!(!listeners[0].enabled);

        reg.enable_listener(&id).expect("enable");
        let listeners = reg.list_listeners().expect("list");
        assert!(listeners[0].enabled);
    }

    #[test]
    fn test_enabled_listeners() {
        let reg = EventRegistry::open_in_memory().expect("open");
        let id1 = reg
            .add_listener("a", "h1", HandlerType::CommandEval, "", false, "")
            .expect("add 1");
        reg.add_listener("b", "h2", HandlerType::CommandEval, "", false, "")
            .expect("add 2");

        reg.disable_listener(&id1).expect("disable");

        let enabled = reg.enabled_listeners().expect("enabled");
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].pattern, "b");
    }

    #[test]
    fn test_once_flag() {
        let reg = EventRegistry::open_in_memory().expect("open");
        reg.add_listener("deploy", "notify", HandlerType::CommandEval, "", true, "")
            .expect("add");

        let listeners = reg.list_listeners().expect("list");
        assert!(listeners[0].once);
    }

    #[test]
    fn test_filter_stored() {
        let reg = EventRegistry::open_in_memory().expect("open");
        reg.add_listener(
            "http_request",
            "alert",
            HandlerType::CommandEval,
            "$4 >= 500",
            false,
            "",
        )
        .expect("add");

        let listeners = reg.list_listeners().expect("list");
        assert_eq!(listeners[0].filter, "$4 >= 500");
    }

    #[test]
    fn test_log_handler_run() {
        let reg = EventRegistry::open_in_memory().expect("open");

        let run = HandlerRun {
            id: uuid::Uuid::new_v4().to_string(),
            listener_id: "listener-1".to_string(),
            pattern: "http_request".to_string(),
            tuple_values: vec!["GET".to_string(), "/api".to_string(), "200".to_string()],
            handler: "echo ok".to_string(),
            exit_code: Some(0),
            duration_ms: 12,
            stdout_snippet: Some("ok".to_string()),
            stderr_snippet: None,
            timestamp: now_timestamp(),
        };

        reg.log_handler_run(&run).expect("log");

        let runs = reg.recent_runs(10).expect("recent");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].listener_id, "listener-1");
        assert_eq!(runs[0].tuple_values, vec!["GET", "/api", "200"]);
        assert_eq!(runs[0].exit_code, Some(0));
    }

    #[test]
    fn test_export_import() {
        let reg = EventRegistry::open_in_memory().expect("open");
        reg.add_listener(
            "http_request",
            "echo $2",
            HandlerType::CommandEval,
            "$4 >= 500",
            false,
            "error alerter",
        )
        .expect("add");

        let exported = reg.export_listeners().expect("export");
        assert!(exported.contains("http_request"));
        assert!(exported.contains("echo $2"));

        // Import into a fresh registry
        let reg2 = EventRegistry::open_in_memory().expect("open 2");
        let count = reg2.import_listeners(&exported).expect("import");
        assert_eq!(count, 1);

        let listeners = reg2.list_listeners().expect("list");
        assert_eq!(listeners.len(), 1);
        assert_eq!(listeners[0].pattern, "http_request");
        assert_eq!(listeners[0].filter, "$4 >= 500");
    }

    #[test]
    fn test_not_found() {
        let reg = EventRegistry::open_in_memory().expect("open");
        let result = reg.remove_listener("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RegistryError::NotFound(_)));
    }

    #[test]
    fn test_prune_old_runs() {
        let reg = EventRegistry::open_in_memory().expect("open");

        // Insert an old run (10 days ago)
        let old_run = HandlerRun {
            id: uuid::Uuid::new_v4().to_string(),
            listener_id: "l1".to_string(),
            pattern: "test".to_string(),
            tuple_values: vec![],
            handler: "echo".to_string(),
            exit_code: Some(0),
            duration_ms: 1,
            stdout_snippet: None,
            stderr_snippet: None,
            timestamp: now_timestamp() - (10 * 86400),
        };
        reg.log_handler_run(&old_run).expect("log old");

        // Insert a recent run
        let recent_run = HandlerRun {
            id: uuid::Uuid::new_v4().to_string(),
            listener_id: "l1".to_string(),
            pattern: "test".to_string(),
            tuple_values: vec![],
            handler: "echo".to_string(),
            exit_code: Some(0),
            duration_ms: 1,
            stdout_snippet: None,
            stderr_snippet: None,
            timestamp: now_timestamp(),
        };
        reg.log_handler_run(&recent_run).expect("log recent");

        let pruned = reg.prune_default().expect("prune");
        assert_eq!(pruned, 1);

        let runs = reg.recent_runs(10).expect("recent");
        assert_eq!(runs.len(), 1);
    }
}
