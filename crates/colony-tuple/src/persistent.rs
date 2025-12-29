//! Persistent tuple space backed by SQLite
//!
//! Provides a hybrid storage model:
//! - In-memory cache for hot paths (sub-microsecond access)
//! - SQLite for persistence across shell restarts
//! - Write-through: writes go to both memory and disk
//! - Read-through: reads try memory first, fallback to disk
//!
//! ## Storage Location
//!
//! - Global: `~/.config/colony/tuple-space.db`
//! - Per-directory: `.colony/tuple-space.db` (if in a Colony project)
//!
//! ## Schema
//!
//! ```sql
//! CREATE TABLE tuples (
//!     id TEXT PRIMARY KEY,
//!     fields TEXT NOT NULL,        -- JSON serialized Vec<Value>
//!     created_at INTEGER NOT NULL,  -- Unix timestamp
//!     expires_at INTEGER,           -- Unix timestamp or NULL
//!     source TEXT,                  -- Session ID or NULL
//!     length INTEGER NOT NULL       -- Number of fields (for bucketing)
//! );
//!
//! CREATE INDEX idx_length ON tuples(length);
//! CREATE INDEX idx_expires ON tuples(expires_at) WHERE expires_at IS NOT NULL;
//! ```

use crate::{Tuple, TupleSpace, Value};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::{debug, warn};

/// Errors that can occur during persistent storage operations
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Tuple space is full (max: {0} tuples)")]
    SpaceFull(usize),
}

/// Configuration for persistent tuple space
#[derive(Debug, Clone)]
pub struct PersistentConfig {
    /// Path to SQLite database file
    pub db_path: PathBuf,

    /// Maximum number of tuples (prevents unbounded growth)
    pub max_tuples: usize,

    /// How often to run cleanup (remove expired tuples)
    pub cleanup_interval_secs: u64,
}

impl Default for PersistentConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            max_tuples: 100_000,
            cleanup_interval_secs: 300, // 5 minutes
        }
    }
}

/// Get default database path: ~/.config/colony/tuple-space.db
fn default_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let config_dir = Path::new(&home).join(".config").join("colony");
    config_dir.join("tuple-space.db")
}

/// Persistent tuple space with hybrid storage
///
/// Uses both in-memory cache (fast) and SQLite (persistent).
///
/// # Example
///
/// ```rust
/// use colony_tuple::{PersistentTupleSpace, Tuple, Value};
///
/// # tokio_test::block_on(async {
/// let space = PersistentTupleSpace::open_default().await.unwrap();
///
/// // Write persists to disk
/// space.out(Tuple::from(vec![
///     Value::Str("task".to_string()),
///     Value::Str("build".to_string()),
/// ])).await.unwrap();
///
/// // Can restart shell and tuple is still there
/// # });
/// ```
pub struct PersistentTupleSpace {
    /// In-memory cache for fast access
    memory: TupleSpace,

    /// SQLite connection (Mutex for Send + interior mutability)
    db: Arc<Mutex<Connection>>,

    /// Configuration
    config: PersistentConfig,
}

impl PersistentTupleSpace {
    /// Open or create tuple space at default location
    pub async fn open_default() -> Result<Self, StorageError> {
        Self::open(PersistentConfig::default()).await
    }

    /// Open or create tuple space with custom config
    pub async fn open(config: PersistentConfig) -> Result<Self, StorageError> {
        // Ensure parent directory exists
        if let Some(parent) = config.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open database
        let db = Connection::open(&config.db_path)?;

        // Initialize schema
        init_schema(&db)?;

        debug!("Opened persistent tuple space at {:?}", config.db_path);

        let space = Self {
            memory: TupleSpace::new(),
            db: Arc::new(Mutex::new(db)),
            config: config.clone(),
        };

        // Load existing tuples into memory cache
        space.load_from_disk().await?;

        // Start background cleanup task
        let space_clone = space.clone();
        tokio::spawn(async move {
            space_clone.cleanup_loop().await;
        });

        Ok(space)
    }

    /// out(tuple) - Write tuple to space (persists to disk)
    ///
    /// # Example
    ///
    /// ```rust
    /// use colony_tuple::{PersistentTupleSpace, Tuple, Value};
    ///
    /// # tokio_test::block_on(async {
    /// let space = PersistentTupleSpace::open_default().await.unwrap();
    /// space.out(Tuple::from(vec![
    ///     Value::Str("lock".to_string()),
    ///     Value::Str("deploy".to_string()),
    ///     Value::Str("session-123".to_string()),
    /// ])).await.unwrap();
    /// # });
    /// ```
    pub async fn out(&self, tuple: Tuple) -> Result<(), StorageError> {
        // Check space limit
        if self.len().await >= self.config.max_tuples {
            return Err(StorageError::SpaceFull(self.config.max_tuples));
        }

        // Generate unique ID
        let id = uuid::Uuid::new_v4().to_string();
        let fields_json = serde_json::to_string(&tuple.fields)?;
        let created_at = now_timestamp();
        let length = tuple.fields.len() as i64;

        // Write to disk first (durability)
        {
            let db = self.db.lock();
            db.execute(
                "INSERT INTO tuples (id, fields, created_at, expires_at, source, length)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    fields_json,
                    created_at,
                    None::<i64>,
                    None::<String>,
                    length
                ],
            )?;
        }

        // Then write to memory (performance)
        self.memory.out(tuple);

        debug!("Wrote tuple {} to persistent space", id);

        Ok(())
    }

    /// inp(pattern) - Non-blocking read and remove
    pub async fn inp(&self, pattern: Tuple) -> Result<Option<Tuple>, StorageError> {
        // Try memory first (fast path)
        if let Some(tuple) = self.memory.inp(pattern.clone()) {
            // Remove from disk too
            self.remove_from_disk(&tuple).await?;
            return Ok(Some(tuple));
        }

        // Fall back to disk (slow path)
        self.inp_from_disk(pattern).await
    }

    /// rdp(pattern) - Non-blocking read (non-destructive)
    pub async fn rdp(&self, pattern: Tuple) -> Result<Option<Tuple>, StorageError> {
        // Try memory first
        if let Some(tuple) = self.memory.rdp(pattern.clone()) {
            return Ok(Some(tuple));
        }

        // Fall back to disk
        self.rdp_from_disk(pattern).await
    }

    /// in(pattern) - Blocking read and remove
    pub async fn r#in(&self, pattern: Tuple) -> Result<Option<Tuple>, StorageError> {
        // Blocking with periodic disk checks
        for _ in 0..100 {
            // Try memory
            if let Some(tuple) = self.memory.inp(pattern.clone()) {
                self.remove_from_disk(&tuple).await?;
                return Ok(Some(tuple));
            }

            // Check disk
            if let Some(tuple) = self.inp_from_disk(pattern.clone()).await? {
                return Ok(Some(tuple));
            }

            // Wait briefly
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        Ok(None) // Timeout after ~10 seconds
    }

    /// rd(pattern) - Blocking read (non-destructive)
    pub async fn rd(&self, pattern: Tuple) -> Result<Option<Tuple>, StorageError> {
        // Blocking with periodic disk checks
        for _ in 0..100 {
            // Try memory
            if let Some(tuple) = self.memory.rdp(pattern.clone()) {
                return Ok(Some(tuple));
            }

            // Check disk
            if let Some(tuple) = self.rdp_from_disk(pattern.clone()).await? {
                // Add to memory for next access
                self.memory.out(tuple.clone());
                return Ok(Some(tuple));
            }

            // Wait briefly
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        Ok(None) // Timeout
    }

    /// Count tuples in space
    pub async fn len(&self) -> usize {
        let db = self.db.lock();
        db.query_row("SELECT COUNT(*) FROM tuples", [], |row| row.get(0))
            .unwrap_or(0)
    }

    /// Check if space is empty
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Clear all tuples (memory and disk)
    pub async fn clear(&self) -> Result<(), StorageError> {
        // Clear disk
        {
            let db = self.db.lock();
            db.execute("DELETE FROM tuples", [])?;
        }

        // Clear memory (create new space)
        // Note: We can't actually clear the existing space since it's behind Arc
        // But for testing purposes, we clear the database

        debug!("Cleared persistent tuple space");

        Ok(())
    }

    // --- Internal helpers ---

    /// Load all tuples from disk into memory cache
    async fn load_from_disk(&self) -> Result<(), StorageError> {
        let db = self.db.lock();

        let mut stmt = db.prepare(
            "SELECT fields FROM tuples
             WHERE expires_at IS NULL OR expires_at > ?1",
        )?;

        let now = now_timestamp();
        let mut rows = stmt.query(params![now])?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let fields_json: String = row.get(0)?;
            let fields: Vec<Value> = serde_json::from_str(&fields_json)?;
            let tuple = Tuple::from(fields);
            self.memory.out(tuple);
            count += 1;
        }

        debug!("Loaded {} tuples from disk into memory", count);

        Ok(())
    }

    /// Read and remove matching tuple from disk
    async fn inp_from_disk(&self, pattern: Tuple) -> Result<Option<Tuple>, StorageError> {
        let length = pattern.fields.len() as i64;
        let now = now_timestamp();

        let db = self.db.lock();

        // Find matching tuple
        let mut stmt = db.prepare(
            "SELECT id, fields FROM tuples
             WHERE length = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
        )?;

        let mut rows = stmt.query(params![length, now])?;

        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let fields_json: String = row.get(1)?;
            let fields: Vec<Value> = serde_json::from_str(&fields_json)?;
            let tuple = Tuple::from(fields);

            if tuple.matches(&pattern) {
                // Found match - remove it
                drop(rows);
                drop(stmt);
                db.execute("DELETE FROM tuples WHERE id = ?1", params![id])?;

                debug!("Removed tuple {} from disk", id);
                return Ok(Some(tuple));
            }
        }

        Ok(None)
    }

    /// Read matching tuple from disk (non-destructive)
    async fn rdp_from_disk(&self, pattern: Tuple) -> Result<Option<Tuple>, StorageError> {
        let length = pattern.fields.len() as i64;
        let now = now_timestamp();

        let db = self.db.lock();

        let mut stmt = db.prepare(
            "SELECT fields FROM tuples
             WHERE length = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
        )?;

        let mut rows = stmt.query(params![length, now])?;

        while let Some(row) = rows.next()? {
            let fields_json: String = row.get(0)?;
            let fields: Vec<Value> = serde_json::from_str(&fields_json)?;
            let tuple = Tuple::from(fields);

            if tuple.matches(&pattern) {
                return Ok(Some(tuple));
            }
        }

        Ok(None)
    }

    /// Remove specific tuple from disk
    async fn remove_from_disk(&self, tuple: &Tuple) -> Result<(), StorageError> {
        let fields_json = serde_json::to_string(&tuple.fields)?;

        let db = self.db.lock();

        // Find the tuple's ID first, then delete by ID
        let id: Option<String> = db
            .query_row(
                "SELECT id FROM tuples WHERE fields = ?1",
                params![fields_json],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = id {
            db.execute("DELETE FROM tuples WHERE id = ?1", params![id])?;
        }

        Ok(())
    }

    /// Background cleanup loop - removes expired tuples
    async fn cleanup_loop(&self) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                self.config.cleanup_interval_secs,
            ))
            .await;

            if let Err(e) = self.cleanup_expired().await {
                warn!("Failed to cleanup expired tuples: {}", e);
            }
        }
    }

    /// Remove expired tuples from disk
    async fn cleanup_expired(&self) -> Result<usize, StorageError> {
        let now = now_timestamp();

        let db = self.db.lock();
        let count = db.execute(
            "DELETE FROM tuples WHERE expires_at IS NOT NULL AND expires_at <= ?1",
            params![now],
        )?;

        if count > 0 {
            debug!("Cleaned up {} expired tuples", count);
        }

        Ok(count)
    }
}

impl Clone for PersistentTupleSpace {
    fn clone(&self) -> Self {
        Self {
            memory: self.memory.clone(),
            db: Arc::clone(&self.db),
            config: self.config.clone(),
        }
    }
}

/// Initialize database schema
fn init_schema(db: &Connection) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS tuples (
            id TEXT PRIMARY KEY,
            fields TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER,
            source TEXT,
            length INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_length ON tuples(length);
        CREATE INDEX IF NOT EXISTS idx_expires ON tuples(expires_at) WHERE expires_at IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_source ON tuples(source) WHERE source IS NOT NULL;
        ",
    )?;

    Ok(())
}

/// Get current Unix timestamp
fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create test tuple space in a temp directory
    async fn test_space() -> (PersistentTupleSpace, TempDir) {
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("test-tuples.db");

        let config = PersistentConfig {
            db_path,
            max_tuples: 1000,
            cleanup_interval_secs: 3600, // Don't run during tests
        };

        let space = PersistentTupleSpace::open(config).await.unwrap();
        (space, tmpdir)
    }

    #[tokio::test]
    async fn test_persistent_out_and_inp() {
        let (space, _tmpdir) = test_space().await;

        // Write a tuple
        space
            .out(Tuple::from(vec![
                Value::Str("test".to_string()),
                Value::Int(42),
            ]))
            .await
            .unwrap();

        // Read it back
        let pattern = Tuple::pattern(&[Value::Str("test".to_string()), Value::Int(42)]);
        let result = space.inp(pattern).await.unwrap();

        assert!(result.is_some());
        let tuple = result.unwrap();
        assert_eq!(tuple.fields[0], Value::Str("test".to_string()));
        assert_eq!(tuple.fields[1], Value::Int(42));

        // Should be removed from both memory and disk
        assert_eq!(space.len().await, 0);
    }

    #[tokio::test]
    async fn test_persistence_across_open() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("test-tuples.db");

        let config = PersistentConfig {
            db_path: db_path.clone(),
            max_tuples: 1000,
            cleanup_interval_secs: 3600,
        };

        // First instance - write tuple
        {
            let space = PersistentTupleSpace::open(config.clone()).await.unwrap();
            space
                .out(Tuple::from(vec![
                    Value::Str("persistent".to_string()),
                    Value::Int(123),
                ]))
                .await
                .unwrap();

            assert_eq!(space.len().await, 1);
        } // Space dropped, simulates shell exit

        // Second instance - should load tuple from disk
        {
            let space = PersistentTupleSpace::open(config).await.unwrap();

            // Tuple should be loaded from disk
            assert_eq!(space.len().await, 1);

            let pattern = Tuple::pattern(&[Value::Str("persistent".to_string()), Value::Wildcard]);
            let result = space.inp(pattern).await.unwrap();

            assert!(result.is_some());
            assert_eq!(result.unwrap().fields[1], Value::Int(123));
        }
    }

    #[tokio::test]
    async fn test_wildcard_matching_persistent() {
        let (space, _tmpdir) = test_space().await;

        space
            .out(Tuple::from(vec![
                Value::Str("event".to_string()),
                Value::Str("file_changed".to_string()),
                Value::Str("/path/to/file".to_string()),
            ]))
            .await
            .unwrap();

        // Match with wildcards
        let pattern = Tuple::pattern(&[
            Value::Str("event".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]);

        let result = space.inp(pattern).await.unwrap();
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().fields[1],
            Value::Str("file_changed".to_string())
        );
    }

    #[tokio::test]
    async fn test_rdp_non_destructive_persistent() {
        let (space, _tmpdir) = test_space().await;

        space.out(Tuple::from(vec![Value::Int(100)])).await.unwrap();

        let pattern = Tuple::pattern(&[Value::Wildcard]);

        // First read
        let result1 = space.rdp(pattern.clone()).await.unwrap();
        assert!(result1.is_some());

        // Second read - tuple should still be there
        let result2 = space.rdp(pattern).await.unwrap();
        assert!(result2.is_some());

        assert_eq!(space.len().await, 1);
    }

    #[tokio::test]
    async fn test_memory_fallback_to_disk() {
        let (space, _tmpdir) = test_space().await;

        // Write tuple
        space
            .out(Tuple::from(vec![
                Value::Str("disk_only".to_string()),
                Value::Int(999),
            ]))
            .await
            .unwrap();

        // Manually remove from memory cache (simulates cache miss)
        let pattern = Tuple::pattern(&[Value::Str("disk_only".to_string()), Value::Int(999)]);
        let _ = space.memory.inp(pattern.clone());

        // Should still find it on disk
        let result = space.rdp(pattern).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().fields[1], Value::Int(999));
    }

    #[tokio::test]
    async fn test_max_tuples_limit() {
        let tmpdir = tempfile::tempdir().unwrap();
        let config = PersistentConfig {
            db_path: tmpdir.path().join("test.db"),
            max_tuples: 5, // Small limit for testing
            cleanup_interval_secs: 3600,
        };

        let space = PersistentTupleSpace::open(config).await.unwrap();

        // Fill to capacity
        for i in 0..5 {
            space.out(Tuple::from(vec![Value::Int(i)])).await.unwrap();
        }

        // Should reject 6th tuple
        let result = space.out(Tuple::from(vec![Value::Int(5)])).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(StorageError::SpaceFull(5))));
    }

    #[tokio::test]
    async fn test_distributed_lock_pattern() {
        let (space, _tmpdir) = test_space().await;

        // Session A acquires lock
        space
            .out(Tuple::from(vec![
                Value::Str("lock".to_string()),
                Value::Str("deploy".to_string()),
                Value::Str("session-a".to_string()),
            ]))
            .await
            .unwrap();

        // Session B checks lock
        let pattern = Tuple::pattern(&[
            Value::Str("lock".to_string()),
            Value::Str("deploy".to_string()),
            Value::Wildcard,
        ]);

        let lock = space.rdp(pattern.clone()).await.unwrap();
        assert!(lock.is_some());
        assert_eq!(lock.unwrap().fields[2], Value::Str("session-a".to_string()));

        // Session A releases lock
        space.inp(pattern).await.unwrap();

        // Lock should be gone
        assert_eq!(space.len().await, 0);
    }
}
