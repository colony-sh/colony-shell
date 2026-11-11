//! Heartbeat Scheduler (F080)
//!
//! A scheduled agent execution system driven by user-authored HEARTBEAT.md
//! checklists. The agent processes the checklist on a configurable interval,
//! executes checks using shell tools, reports findings, and stores results
//! in the shared reflection database.
//!
//! ## Architecture
//!
//! - Types, parser, storage, finding extraction, and dashboard rendering
//!   live in this module within `colony-exec`.
//! - Actual agent invocation happens via a callback closure provided at
//!   startup to avoid circular dependencies with `colony-agent`.
//! - The scheduler runs as a background tokio task with a configurable
//!   interval, re-reading HEARTBEAT.md frontmatter each tick for hot-reload.
//! - Which checklists have a heartbeat is state in the machine's event log
//!   ([`HEARTBEAT_SET`], [`HEARTBEAT_PAUSED`]). The `colony-events` daemon replays
//!   it ([`apply`]) and runs a scheduler for each, so heartbeats outlive the shell.
//!   Every tick appends [`HEARTBEAT_FIRED`] and [`HEARTBEAT_ANSWERED`].

pub mod dashboard;
pub mod parser;
pub mod runner;
pub mod storage;
pub mod types;

pub use parser::{body_is_empty, parse_duration, parse_heartbeat};
pub use runner::{build_checklist_prompt, build_system_prompt, derive_status, extract_findings};
pub use storage::HeartbeatStore;
pub use types::*;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use colony_tuple::{Event, EventLog};
use tokio::sync::watch;

/// Event: a checklist was given a heartbeat. Data: `{"checklist": <path>}`.
pub const HEARTBEAT_SET: &str = "HeartbeatSet";
/// Event: a checklist's heartbeat was stopped. Data: `{"checklist": <path>}`.
pub const HEARTBEAT_PAUSED: &str = "HeartbeatPaused";
/// Event: a tick started. Data: `{"checklist": <path>}`.
pub const HEARTBEAT_FIRED: &str = "HeartbeatFired";
/// Event: a tick ended. Cause: its [`HEARTBEAT_FIRED`]. Data: checklist, run, status, findings.
pub const HEARTBEAT_ANSWERED: &str = "HeartbeatAnswered";

/// Fold one event into the set of checklists that have a heartbeat.
pub fn apply(active: &mut BTreeSet<PathBuf>, event: &Event) {
    let Some(path) = event.data.get("checklist").and_then(|v| v.as_str()) else {
        return;
    };
    match event.kind.as_str() {
        HEARTBEAT_SET => {
            active.insert(PathBuf::from(path));
        }
        HEARTBEAT_PAUSED => {
            active.remove(Path::new(path));
        }
        _ => {}
    }
}

/// The checklists that have a heartbeat, by replaying the whole log.
pub fn active_checklists(log: &EventLog) -> Result<BTreeSet<PathBuf>, HeartbeatError> {
    let mut active = BTreeSet::new();
    let mut last = 0;
    loop {
        let page = log.after(last, 1000).map_err(db_error)?;
        let Some(tail) = page.last() else {
            return Ok(active);
        };
        last = tail.seq;
        page.iter().for_each(|ev| apply(&mut active, ev));
    }
}

fn db_error(e: impl std::fmt::Display) -> HeartbeatError {
    HeartbeatError::Database(e.to_string())
}

/// Callback type for agent invocation.
///
/// Takes the heartbeat config, the HEARTBEAT.md path and the checklist body (the file
/// less its frontmatter), returns a `HeartbeatRun` record. The callback is provided by
/// `colony-shell` at startup to bridge the dependency gap between
/// `colony-exec` and `colony-agent`. It is called from a blocking context, never on an
/// async worker, so it may block for the whole run.
pub type AgentCallback =
    dyn Fn(&HeartbeatConfig, &Path, &str) -> Result<HeartbeatRun, HeartbeatError> + Send + Sync;

/// The callback every scheduler uses when none was set on it.
static DEFAULT_AGENT_CALLBACK: OnceLock<Arc<AgentCallback>> = OnceLock::new();

/// Set the process-wide agent callback, once, at startup. Later calls are ignored.
///
/// Executors are created in many places (subshells, scripts, the REPL), so the shell sets
/// the callback here once instead of on each scheduler.
pub fn set_default_agent_callback(callback: Arc<AgentCallback>) {
    let _ = DEFAULT_AGENT_CALLBACK.set(callback);
}

/// Shared state for the heartbeat scheduler, accessible from builtins.
#[derive(Clone)]
pub struct HeartbeatScheduler {
    inner: Arc<Mutex<SchedulerState>>,
}

struct SchedulerState {
    /// Path to the discovered HEARTBEAT.md file.
    checklist_path: Option<PathBuf>,
    /// Current configuration (from frontmatter + overrides).
    config: HeartbeatConfig,
    /// Override interval set by `heartbeat interval <duration>`.
    interval_override: Option<u64>,
    /// Whether the scheduler background task is running.
    running: bool,
    /// Channel to signal the background task to stop.
    stop_tx: Option<watch::Sender<bool>>,
    /// The store for persisting runs.
    store: Option<Arc<HeartbeatStore>>,
    /// Agent invocation callback.
    agent_callback: Option<Arc<AgentCallback>>,
    /// Last completed run (cached for status display).
    last_run: Option<HeartbeatRun>,
    /// Start time of the scheduler (for "next run" calculation).
    started_at: Option<std::time::Instant>,
    /// Tuple space reference for emitting tuples.
    tuple_space: Option<colony_tuple::TupleSpace>,
    /// Session ID for tuple scoping.
    session_id: String,
}

impl HeartbeatScheduler {
    /// Create a new scheduler (not yet started).
    pub fn new(session_id: &str) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SchedulerState {
                checklist_path: None,
                config: HeartbeatConfig::default(),
                interval_override: None,
                running: false,
                stop_tx: None,
                store: None,
                agent_callback: None,
                last_run: None,
                started_at: None,
                tuple_space: None,
                session_id: session_id.to_string(),
            })),
        }
    }

    /// Set the agent invocation callback.
    pub fn set_agent_callback(&self, callback: Arc<AgentCallback>) {
        let mut state = self.inner.lock().expect("scheduler mutex poisoned");
        state.agent_callback = Some(callback);
    }

    /// Set the tuple space for emitting tuples.
    pub fn set_tuple_space(&self, ts: colony_tuple::TupleSpace) {
        let mut state = self.inner.lock().expect("scheduler mutex poisoned");
        state.tuple_space = Some(ts);
    }

    /// Discover HEARTBEAT.md and validate the environment.
    ///
    /// Returns the path to the discovered HEARTBEAT.md, or an error.
    pub fn discover(&self, start_dir: &std::path::Path) -> Result<PathBuf, HeartbeatError> {
        let path = find_heartbeat_md(start_dir)?;

        let content = std::fs::read_to_string(&path)?;
        let (config, _items) = parse_heartbeat(&content);

        let mut state = self.inner.lock().expect("scheduler mutex poisoned");
        state.checklist_path = Some(path.clone());
        state.config = config;

        // Open the store
        let store = HeartbeatStore::open(&HeartbeatStore::default_path())
            .map_err(|e| HeartbeatError::Database(e.to_string()))?;
        state.store = Some(Arc::new(store));

        Ok(path)
    }

    /// Start the background scheduler task.
    pub fn start(&self) -> Result<(), HeartbeatError> {
        let mut state = self.inner.lock().expect("scheduler mutex poisoned");

        if state.running {
            return Err(HeartbeatError::AlreadyRunning);
        }

        let Some(path) = state.checklist_path.clone() else {
            return Err(HeartbeatError::NotFound);
        };

        let (stop_tx, stop_rx) = watch::channel(false);
        state.stop_tx = Some(stop_tx);
        state.running = true;
        state.started_at = Some(std::time::Instant::now());

        let interval_secs = state
            .interval_override
            .unwrap_or(state.config.interval_secs);
        let scheduler = self.clone();

        tokio::spawn(async move {
            scheduler
                .background_loop(path, interval_secs, stop_rx)
                .await;
        });

        Ok(())
    }

    /// Stop the background scheduler task.
    pub fn stop(&self) -> Result<(), HeartbeatError> {
        let mut state = self.inner.lock().expect("scheduler mutex poisoned");

        if !state.running {
            return Err(HeartbeatError::NotRunning);
        }

        if let Some(tx) = state.stop_tx.take() {
            let _ = tx.send(true);
        }
        state.running = false;
        state.started_at = None;

        Ok(())
    }

    /// Execute a single heartbeat tick immediately, discovering HEARTBEAT.md from
    /// `start_dir` first if the scheduler has not found one yet.
    pub fn run_once_from(&self, start_dir: &Path) -> Result<HeartbeatRun, HeartbeatError> {
        if self.checklist_path().is_none() {
            self.discover(start_dir)?;
        }
        self.run_once()
    }

    /// Execute a single heartbeat tick immediately.
    pub fn run_once(&self) -> Result<HeartbeatRun, HeartbeatError> {
        let state = self.inner.lock().expect("scheduler mutex poisoned");

        let Some(path) = state.checklist_path.clone() else {
            return Err(HeartbeatError::NotFound);
        };

        let callback = state
            .agent_callback
            .clone()
            .or_else(|| DEFAULT_AGENT_CALLBACK.get().cloned())
            .ok_or_else(|| {
                HeartbeatError::NoProvider("no agent callback configured".to_string())
            })?;

        let store = state.store.clone();
        let tuple_space = state.tuple_space.clone();
        // Who the log says fired the beat: the shell's session or "daemon".
        let actor = state.session_id.clone();

        // Drop the lock before doing I/O
        drop(state);

        let content = std::fs::read_to_string(&path)?;
        let (config, items) = parse_heartbeat(&content);

        // Check for empty body
        let body = content
            .find("\n---")
            .map(|i| &content[i + 4..])
            .unwrap_or(&content);
        if body_is_empty(body) && items.is_empty() {
            return Err(HeartbeatError::Agent(
                "checklist is empty, skipping".to_string(),
            ));
        }

        let log = EventLog::open_default()
            .map_err(|e| tracing::warn!("heartbeat: event log unavailable: {e}"))
            .ok();
        let checklist = serde_json::json!({ "checklist": path.display().to_string() });
        let fired = log.as_ref().and_then(|log| {
            log.append(&actor, HEARTBEAT_FIRED, None, None, checklist.clone())
                .map_err(|e| tracing::warn!("heartbeat: could not log the beat: {e}"))
                .ok()
        });

        let outcome = callback(&config, &path, body);
        if let Some(log) = &log {
            let data = match &outcome {
                Ok(run) => serde_json::json!({
                    "checklist": checklist["checklist"],
                    "run": run.id,
                    "status": run.status.to_string(),
                    "findings": run.findings.len(),
                }),
                Err(e) => serde_json::json!({
                    "checklist": checklist["checklist"],
                    "status": "error",
                    "error": e.to_string(),
                }),
            };
            if let Err(e) = log.append(&actor, HEARTBEAT_ANSWERED, None, fired, data) {
                tracing::warn!("heartbeat: could not log the answer: {e}");
            }
        }
        let mut run = outcome?;
        // The scheduler owns these facts, not the agent.
        run.checklist_path = path.display().to_string();
        run.interval_secs = config.interval_secs;

        // Store the run
        if let Some(ref store) = store {
            store.insert_run(&run)?;
        }

        // Emit tuples
        if let Some(ref ts) = tuple_space {
            use colony_tuple::{Tuple, Value};
            ts.out(Tuple::from(vec![
                Value::Str("heartbeat_complete".to_string()),
                Value::Str(path.display().to_string()),
                Value::Str(run.id.clone()),
                Value::Str(run.status.to_string()),
                Value::Int(run.findings.len() as i64),
                Value::Int(run.duration_ms as i64),
            ]));

            if config.notify {
                for finding in &run.findings {
                    ts.out(Tuple::from(vec![
                        Value::Str("heartbeat_finding".to_string()),
                        Value::Str(path.display().to_string()),
                        Value::Str(run.id.clone()),
                        Value::Str(finding.severity.to_string()),
                        Value::Str(finding.message.clone()),
                    ]));
                }
            }
        }

        // Cache the last run
        let mut state = self.inner.lock().expect("scheduler mutex poisoned");
        state.last_run = Some(run.clone());
        state.config = config;

        Ok(run)
    }

    /// Get the current scheduler status for display.
    pub fn status(&self) -> SchedulerStatus {
        let state = self.inner.lock().expect("scheduler mutex poisoned");
        let interval_secs = state
            .interval_override
            .unwrap_or(state.config.interval_secs);

        SchedulerStatus {
            running: state.running,
            checklist_path: state.checklist_path.clone(),
            interval_secs,
            mode: state.config.mode,
            last_run: state.last_run.clone(),
            active_items: 0, // populated by caller if needed
        }
    }

    /// Override the scheduler interval.
    pub fn set_interval(&self, secs: u64) {
        let mut state = self.inner.lock().expect("scheduler mutex poisoned");
        state.interval_override = Some(secs);
    }

    /// Get a reference to the store (for log/render commands).
    pub fn store(&self) -> Option<Arc<HeartbeatStore>> {
        let state = self.inner.lock().expect("scheduler mutex poisoned");
        state.store.clone()
    }

    /// Get the current config.
    pub fn config(&self) -> HeartbeatConfig {
        let state = self.inner.lock().expect("scheduler mutex poisoned");
        state.config.clone()
    }

    /// Get the checklist path.
    pub fn checklist_path(&self) -> Option<PathBuf> {
        let state = self.inner.lock().expect("scheduler mutex poisoned");
        state.checklist_path.clone()
    }

    /// Check if the scheduler is running.
    pub fn is_running(&self) -> bool {
        let state = self.inner.lock().expect("scheduler mutex poisoned");
        state.running
    }

    /// Check if the interval needs updating (hot-reload from frontmatter).
    ///
    /// Returns the new effective interval if it changed, or `None` if unchanged.
    fn check_interval_change(&self, path: &std::path::Path, current_interval: u64) -> Option<u64> {
        let content = std::fs::read_to_string(path).ok()?;
        let (new_config, _) = parse_heartbeat(&content);
        let effective = {
            let state = self.inner.lock().expect("scheduler mutex poisoned");
            state.interval_override.unwrap_or(new_config.interval_secs)
        };
        if effective != current_interval {
            Some(effective)
        } else {
            None
        }
    }

    /// The background loop that runs on each interval tick.
    async fn background_loop(
        &self,
        path: PathBuf,
        initial_interval_secs: u64,
        mut stop_rx: watch::Receiver<bool>,
    ) {
        let mut interval_secs = initial_interval_secs;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));

        // Skip the first immediate tick (the scheduler just started)
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Check for interval changes (hot-reload)
                    if let Some(new_interval) = self.check_interval_change(&path, interval_secs) {
                        interval_secs = new_interval;
                        interval = tokio::time::interval(
                            std::time::Duration::from_secs(interval_secs),
                        );
                        interval.tick().await; // consume immediate tick
                    }

                    // Execute tick off the async worker: a run blocks for its whole length.
                    let scheduler = self.clone();
                    let outcome = tokio::task::spawn_blocking(move || scheduler.run_once())
                        .await
                        .unwrap_or_else(|e| Err(HeartbeatError::Agent(format!("tick panicked: {e}"))));
                    match outcome {
                        Ok(run) => {
                            tracing::info!(
                                "Heartbeat tick complete: status={}, findings={}, duration={}ms",
                                run.status,
                                run.findings.len(),
                                run.duration_ms,
                            );
                        }
                        Err(HeartbeatError::Agent(ref msg)) if msg.contains("empty") => {
                            tracing::debug!("Heartbeat skipped: {msg}");
                        }
                        Err(e) => {
                            tracing::warn!("Heartbeat tick failed: {e}");
                        }
                    }
                }
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() {
                        tracing::info!("Heartbeat scheduler stopped");
                        break;
                    }
                }
            }
        }

        // Mark as stopped
        let mut state = self.inner.lock().expect("scheduler mutex poisoned");
        state.running = false;
    }
}

/// Status information for display by the `heartbeat status` subcommand.
#[derive(Debug)]
pub struct SchedulerStatus {
    pub running: bool,
    pub checklist_path: Option<PathBuf>,
    pub interval_secs: u64,
    pub mode: SecurityMode,
    pub last_run: Option<HeartbeatRun>,
    pub active_items: usize,
}

/// Walk the directory hierarchy to find HEARTBEAT.md.
///
/// Searches: current dir -> parent dirs -> `~/.colony/`.
fn find_heartbeat_md(start_dir: &std::path::Path) -> Result<PathBuf, HeartbeatError> {
    let mut current = start_dir.to_path_buf();
    loop {
        let candidate = current.join("HEARTBEAT.md");
        if candidate.is_file() {
            return Ok(candidate);
        }
        let Some(parent) = current.parent() else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent.to_path_buf();
    }

    // Check ~/.colony/
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".colony/HEARTBEAT.md");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(HeartbeatError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_heartbeat_md_in_current_dir() {
        let dir = tempfile::tempdir().unwrap();
        let heartbeat = dir.path().join("HEARTBEAT.md");
        std::fs::write(&heartbeat, "- [ ] Check tests\n").unwrap();

        let found = find_heartbeat_md(dir.path()).unwrap();
        assert_eq!(found, heartbeat);
    }

    #[test]
    fn test_find_heartbeat_md_in_parent() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("child");
        std::fs::create_dir(&child).unwrap();
        let heartbeat = root.path().join("HEARTBEAT.md");
        std::fs::write(&heartbeat, "- [ ] Check tests\n").unwrap();

        let found = find_heartbeat_md(&child).unwrap();
        assert_eq!(found, heartbeat);
    }

    #[test]
    fn test_find_heartbeat_md_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = find_heartbeat_md(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_scheduler_new() {
        let scheduler = HeartbeatScheduler::new("test-session");
        assert!(!scheduler.is_running());
        assert!(scheduler.checklist_path().is_none());
    }

    #[test]
    fn test_scheduler_discover() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("HEARTBEAT.md"),
            "---\ninterval: 10m\nmode: active\n---\n\n- [ ] Check tests\n",
        )
        .unwrap();

        let scheduler = HeartbeatScheduler::new("test");
        let path = scheduler.discover(dir.path()).unwrap();
        assert!(path.ends_with("HEARTBEAT.md"));

        let config = scheduler.config();
        assert_eq!(config.interval_secs, 600);
        assert_eq!(config.mode, SecurityMode::Active);
    }

    #[test]
    fn test_scheduler_set_interval() {
        let scheduler = HeartbeatScheduler::new("test");
        scheduler.set_interval(300);

        let status = scheduler.status();
        assert_eq!(status.interval_secs, 300);
    }

    #[test]
    fn test_active_checklists_follow_set_and_paused() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(&dir.path().join("m.db")).unwrap();
        let set = |p: &str| serde_json::json!({ "checklist": p });
        log.append("a", HEARTBEAT_SET, None, None, set("/x/HEARTBEAT.md"))
            .unwrap();
        log.append("a", HEARTBEAT_SET, None, None, set("/y/HEARTBEAT.md"))
            .unwrap();
        log.append("a", HEARTBEAT_FIRED, None, None, set("/z/HEARTBEAT.md"))
            .unwrap();
        log.append("a", HEARTBEAT_PAUSED, None, None, set("/x/HEARTBEAT.md"))
            .unwrap();
        let active: Vec<_> = active_checklists(&log).unwrap().into_iter().collect();
        assert_eq!(active, [PathBuf::from("/y/HEARTBEAT.md")]);

        log.append("a", HEARTBEAT_SET, None, None, set("/x/HEARTBEAT.md"))
            .unwrap();
        assert_eq!(active_checklists(&log).unwrap().len(), 2);
    }

    #[test]
    fn test_scheduler_stop_not_running() {
        let scheduler = HeartbeatScheduler::new("test");
        let result = scheduler.stop();
        assert!(result.is_err());
    }
}
