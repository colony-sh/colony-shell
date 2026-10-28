//! colony-events: Event router daemon for colony-shell.
//!
//! Polls the persistent tuple space SQLite for new tuples, matches against
//! the listener registry (shared reflection DB), and spawns handler processes
//! with tuple values injected.
//!
//! ## Usage
//!
//! Normally started automatically by `on <pattern> <handler>`:
//!
//! ```text
//! colony-events daemon          # run as background daemon (default)
//! colony-events --foreground    # run in foreground (for debugging)
//! ```
//!
//! ## Architecture
//!
//! - Polls `~/.config/colony/tuple-space.db` every 100ms for new tuples
//! - Reads listener registry from `~/.colony/reflection/observations.db`
//! - Reloads listeners every 5s (or on SIGHUP)
//! - Max 64 concurrent handlers, 30s timeout per handler
//! - Logs executions to `event_handler_runs` table
//! - PID file at `~/.colony/events/router.pid`
//! - Graceful shutdown on SIGTERM

#[allow(dead_code)]
mod dashboard;
mod handler;

use colony_exec::events::{
    daemon_pid_path, evaluate_filter, parse_filter, EventListener, EventRegistry, FilterExpr,
};
use handler::{make_handler_run, spawn_handler};

use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

/// How often to poll for new tuples (milliseconds).
const POLL_INTERVAL_MS: u64 = 100;

/// How often to reload the listener registry (seconds).
const REGISTRY_RELOAD_INTERVAL_SECS: u64 = 5;

/// Maximum concurrent handler processes.
const MAX_CONCURRENT_HANDLERS: usize = 64;

/// How often to prune old runs (seconds).
const PRUNE_INTERVAL_SECS: u64 = 3600;

/// Default retention for handler runs in days.
const RETENTION_DAYS: i64 = 7;

#[tokio::main]
async fn main() {
    // Parse minimal CLI args
    let args: Vec<String> = std::env::args().collect();
    let foreground = args.iter().any(|a| a == "--foreground" || a == "-f");

    // Initialize logging
    let filter = if foreground { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();

    info!("colony-events daemon starting");

    // Open the reflection DB (listener registry + run logging)
    let registry_path = EventRegistry::default_path();
    let registry = match EventRegistry::open(&registry_path) {
        Ok(r) => r,
        Err(e) => {
            error!(
                "Failed to open registry at {}: {e}",
                registry_path.display()
            );
            std::process::exit(1);
        }
    };

    // Open the persistent tuple space SQLite
    let tuple_db_path = tuple_space_db_path();
    let tuple_db = match open_tuple_db(&tuple_db_path) {
        Ok(db) => db,
        Err(e) => {
            error!(
                "Failed to open tuple space at {}: {e}",
                tuple_db_path.display()
            );
            std::process::exit(1);
        }
    };

    // Write PID file
    if let Err(e) = write_pid_file() {
        error!("Failed to write PID file: {e}");
        std::process::exit(1);
    }

    // Prune old runs on startup
    if let Err(e) = registry.prune_old_runs(RETENTION_DAYS) {
        warn!("Failed to prune old runs: {e}");
    }

    // Set up signal handling
    let shutdown = Arc::new(AtomicBool::new(false));
    let reload = Arc::new(AtomicBool::new(false));

    // Register signal handlers
    {
        let shutdown_clone = Arc::clone(&shutdown);
        let reload_clone = Arc::clone(&reload);

        // SIGTERM -> graceful shutdown
        if let Ok(mut stream) = signal_hook::iterator::Signals::new([signal_hook::consts::SIGTERM])
        {
            let flag = Arc::clone(&shutdown_clone);
            std::thread::spawn(move || {
                if stream.forever().next().is_some() {
                    flag.store(true, Ordering::SeqCst);
                }
            });
        }

        // SIGHUP -> reload listeners (can fire multiple times)
        if let Ok(mut stream) = signal_hook::iterator::Signals::new([signal_hook::consts::SIGHUP]) {
            std::thread::spawn(move || {
                for _ in stream.forever() {
                    reload_clone.store(true, Ordering::SeqCst);
                }
            });
        }

        // SIGINT -> graceful shutdown (for foreground mode)
        if foreground {
            if let Ok(mut stream) =
                signal_hook::iterator::Signals::new([signal_hook::consts::SIGINT])
            {
                let flag = Arc::clone(&shutdown_clone);
                std::thread::spawn(move || {
                    if stream.forever().next().is_some() {
                        flag.store(true, Ordering::SeqCst);
                    }
                });
            }
        }
    }

    info!(
        "Daemon started (pid {}), polling tuple space at {}",
        std::process::id(),
        tuple_db_path.display()
    );

    // Run the event loop
    run_event_loop(&registry, &tuple_db, shutdown, reload).await;

    // Cleanup
    remove_pid_file();
    info!("Daemon stopped");
}

/// Main event loop: poll tuples, match listeners, spawn handlers.
async fn run_event_loop(
    registry: &EventRegistry,
    tuple_db: &Connection,
    shutdown: Arc<AtomicBool>,
    reload: Arc<AtomicBool>,
) {
    let mut listeners = load_listeners(registry);
    let mut compiled_filters: HashMap<String, FilterExpr> = compile_filters(&listeners);
    let mut last_seen_id: i64 = get_max_rowid(tuple_db);
    let mut last_reload = Instant::now();
    let mut last_prune = Instant::now();
    let events_processed = Arc::new(AtomicU64::new(0));

    // Track which listeners have handlers currently running (skip-if-busy)
    let mut busy_listeners: HashSet<String> = HashSet::new();

    let mut handler_set: JoinSet<(String, handler::HandlerResult, EventListener, Vec<String>)> =
        JoinSet::new();

    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

    loop {
        if shutdown.load(Ordering::SeqCst) {
            info!("Shutdown signal received, waiting for in-flight handlers...");
            // Wait up to 5s for handlers
            let deadline = Instant::now() + Duration::from_secs(5);
            while !handler_set.is_empty() && Instant::now() < deadline {
                if let Some(result) = handler_set.join_next().await {
                    handle_completed(result, registry, &mut busy_listeners);
                }
            }
            break;
        }

        // Reload listeners periodically or on SIGHUP
        if reload.load(Ordering::SeqCst)
            || last_reload.elapsed() > Duration::from_secs(REGISTRY_RELOAD_INTERVAL_SECS)
        {
            reload.store(false, Ordering::SeqCst);
            listeners = load_listeners(registry);
            compiled_filters = compile_filters(&listeners);
            last_reload = Instant::now();
            debug!("Reloaded {} listeners", listeners.len());
        }

        // Prune old runs periodically
        if last_prune.elapsed() > Duration::from_secs(PRUNE_INTERVAL_SECS) {
            if let Err(e) = registry.prune_old_runs(RETENTION_DAYS) {
                warn!("Failed to prune old runs: {e}");
            }
            last_prune = Instant::now();
        }

        // Poll for new tuples
        let new_tuples = poll_new_tuples(tuple_db, last_seen_id);
        for (rowid, tuple_values) in &new_tuples {
            if *rowid > last_seen_id {
                last_seen_id = *rowid;
            }

            events_processed.fetch_add(1, Ordering::Relaxed);

            let Some(tuple_key) = tuple_values.first() else {
                continue;
            };

            // Match against listeners
            for listener in &listeners {
                // Pattern matching: exact match on tuple key
                if listener.pattern != *tuple_key {
                    continue;
                }

                // Skip if this listener already has a handler running
                if busy_listeners.contains(&listener.id) {
                    debug!(
                        "Skipping listener {} (busy)",
                        &listener.id[..8.min(listener.id.len())]
                    );
                    continue;
                }

                // Check global concurrency limit
                if handler_set.len() >= MAX_CONCURRENT_HANDLERS {
                    warn!("Max concurrent handlers ({MAX_CONCURRENT_HANDLERS}) reached, skipping");
                    break;
                }

                // Evaluate filter
                if let Some(filter_expr) = compiled_filters.get(&listener.id) {
                    if !evaluate_filter(filter_expr, tuple_values) {
                        debug!(
                            "Filter rejected for listener {}",
                            &listener.id[..8.min(listener.id.len())]
                        );
                        continue;
                    }
                }

                // Spawn handler
                let listener_clone = listener.clone();
                let values_clone = tuple_values.clone();
                busy_listeners.insert(listener.id.clone());

                info!(
                    "Spawning handler for {} -> {} (listener {})",
                    tuple_key,
                    listener.handler,
                    &listener.id[..8.min(listener.id.len())]
                );

                let lid = listener.id.clone();
                handler_set.spawn(async move {
                    let result = spawn_handler(&listener_clone, &values_clone).await;
                    (lid, result, listener_clone, values_clone)
                });
            }
        }

        // Reap completed handlers (non-blocking)
        while let Some(result) = handler_set.try_join_next() {
            handle_completed(result, registry, &mut busy_listeners);
        }

        // Sleep before next poll
        tokio::time::sleep(poll_interval).await;
    }
}

/// Handle a completed handler task.
fn handle_completed(
    result: Result<
        (String, handler::HandlerResult, EventListener, Vec<String>),
        tokio::task::JoinError,
    >,
    registry: &EventRegistry,
    busy_listeners: &mut HashSet<String>,
) {
    let Ok((listener_id, handler_result, listener, tuple_values)) = result else {
        warn!("Handler task panicked");
        return;
    };

    busy_listeners.remove(&listener_id);

    let exit_code = handler_result.exit_code.unwrap_or(-1);
    let short_id = &listener_id[..8.min(listener_id.len())];

    if exit_code == 0 {
        info!(
            "Handler {} completed: exit 0, {}ms",
            short_id, handler_result.duration_ms
        );
    } else {
        warn!(
            "Handler {} completed: exit {}, {}ms",
            short_id, exit_code, handler_result.duration_ms
        );
    }

    // Log the run
    let run = make_handler_run(&listener, &tuple_values, &handler_result);
    if let Err(e) = registry.log_handler_run(&run) {
        warn!("Failed to log handler run: {e}");
    }

    // If once=true, remove the listener
    if listener.once {
        info!("Removing one-shot listener {short_id}");
        if let Err(e) = registry.remove_listener(&listener_id) {
            warn!("Failed to remove one-shot listener: {e}");
        }
    }
}

/// Load enabled listeners from the registry.
fn load_listeners(registry: &EventRegistry) -> Vec<EventListener> {
    match registry.enabled_listeners() {
        Ok(listeners) => listeners,
        Err(e) => {
            warn!("Failed to load listeners: {e}");
            Vec::new()
        }
    }
}

/// Pre-compile filter expressions for all listeners with filters.
fn compile_filters(listeners: &[EventListener]) -> HashMap<String, FilterExpr> {
    let mut filters = HashMap::new();
    for listener in listeners {
        if !listener.filter.is_empty() {
            match parse_filter(&listener.filter) {
                Ok(expr) => {
                    filters.insert(listener.id.clone(), expr);
                }
                Err(e) => {
                    warn!(
                        "Invalid filter for listener {}: {e}",
                        &listener.id[..8.min(listener.id.len())]
                    );
                }
            }
        }
    }
    filters
}

// ---------------------------------------------------------------------------
// Tuple space SQLite polling
// ---------------------------------------------------------------------------

/// Default path to the persistent tuple space database.
fn tuple_space_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::Path::new(&home).join(".config/colony/tuple-space.db")
}

/// Open the tuple space SQLite database (read-only).
fn open_tuple_db(path: &PathBuf) -> Result<Connection, rusqlite::Error> {
    if !path.exists() {
        // Create the database if it does not exist yet
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
    }

    let db = Connection::open(path)?;
    // Ensure the tuples table exists (in case the tuple space has not been used yet)
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS tuples (
            id TEXT PRIMARY KEY,
            fields TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER,
            source TEXT,
            length INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_length ON tuples(length);",
    )?;
    Ok(db)
}

/// Get the maximum rowid from the tuples table (for tracking "last seen").
fn get_max_rowid(db: &Connection) -> i64 {
    db.query_row("SELECT COALESCE(MAX(rowid), 0) FROM tuples", [], |row| {
        row.get(0)
    })
    .unwrap_or(0)
}

/// Poll for tuples with rowid greater than `last_seen`.
///
/// Returns a list of (rowid, tuple_values) pairs where tuple_values
/// is the parsed JSON fields array converted to strings.
fn poll_new_tuples(db: &Connection, last_seen: i64) -> Vec<(i64, Vec<String>)> {
    let mut stmt =
        match db.prepare("SELECT rowid, fields FROM tuples WHERE rowid > ?1 ORDER BY rowid") {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to prepare tuple poll query: {e}");
                return Vec::new();
            }
        };

    let mut results = Vec::new();
    let rows = match stmt.query_map(rusqlite::params![last_seen], |row| {
        let rowid: i64 = row.get(0)?;
        let fields_json: String = row.get(1)?;
        Ok((rowid, fields_json))
    }) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to poll tuples: {e}");
            return Vec::new();
        }
    };

    for row in rows {
        let Ok((rowid, fields_json)) = row else {
            continue;
        };

        // Parse JSON fields array into string values
        let values = parse_tuple_fields(&fields_json);
        if !values.is_empty() {
            results.push((rowid, values));
        }
    }

    results
}

/// Parse the JSON fields array into a flat list of string values.
///
/// Tuple fields are stored as a JSON array of `colony_tuple::Value` variants.
/// We extract the string representation of each field.
fn parse_tuple_fields(fields_json: &str) -> Vec<String> {
    let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(fields_json) else {
        return Vec::new();
    };

    arr.iter()
        .map(|v| {
            // colony_tuple::Value serializes as {"Str": "value"} etc.
            if let Some(obj) = v.as_object() {
                if let Some(s) = obj.get("Str").and_then(|v| v.as_str()) {
                    return s.to_string();
                }
                if let Some(n) = obj.get("Int").and_then(|v| v.as_i64()) {
                    return n.to_string();
                }
                if let Some(n) = obj.get("Float").and_then(|v| v.as_f64()) {
                    return n.to_string();
                }
                if let Some(b) = obj.get("Bool").and_then(|v| v.as_bool()) {
                    return b.to_string();
                }
                if obj.contains_key("Null") {
                    return String::new();
                }
            }
            // Fallback: use raw JSON representation
            v.to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// PID file management
// ---------------------------------------------------------------------------

/// Write the current process PID to the PID file.
fn write_pid_file() -> Result<(), std::io::Error> {
    let path = daemon_pid_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, std::process::id().to_string())
}

/// Remove the PID file.
fn remove_pid_file() {
    let _ = std::fs::remove_file(daemon_pid_path());
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use colony_exec::events::HandlerType;

    #[test]
    fn test_parse_tuple_fields_str() {
        let json = r#"[{"Str":"http_request"},{"Str":"GET"},{"Str":"/api"},{"Str":"200"}]"#;
        let values = parse_tuple_fields(json);
        assert_eq!(values, vec!["http_request", "GET", "/api", "200"]);
    }

    #[test]
    fn test_parse_tuple_fields_mixed() {
        let json = r#"[{"Str":"event"},{"Int":42},{"Float":3.14},{"Bool":true},{"Null":null}]"#;
        let values = parse_tuple_fields(json);
        assert_eq!(values, vec!["event", "42", "3.14", "true", ""]);
    }

    #[test]
    fn test_parse_tuple_fields_empty() {
        let values = parse_tuple_fields("[]");
        assert!(values.is_empty());
    }

    #[test]
    fn test_parse_tuple_fields_invalid() {
        let values = parse_tuple_fields("not json");
        assert!(values.is_empty());
    }

    #[test]
    fn test_tuple_space_db_path() {
        let path = tuple_space_db_path();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("colony/tuple-space.db"));
    }

    #[test]
    fn test_compile_filters_empty() {
        let filters = compile_filters(&[]);
        assert!(filters.is_empty());
    }

    #[test]
    fn test_compile_filters_with_filter() {
        let listener = EventListener {
            id: "test-id".to_string(),
            pattern: "http_request".to_string(),
            handler: "echo".to_string(),
            handler_type: HandlerType::CommandEval,
            filter: "$4 >= 500".to_string(),
            once: false,
            enabled: true,
            description: String::new(),
            created_at: 0,
        };
        let filters = compile_filters(&[listener]);
        assert_eq!(filters.len(), 1);
        assert!(filters.contains_key("test-id"));
    }
}
