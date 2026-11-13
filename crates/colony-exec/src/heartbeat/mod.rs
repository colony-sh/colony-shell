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

/// Go on with a parked agent's run now that every ask in it is answered: the model gets
/// its charter, what it said and the answers, under the same limits as a beat.
/// The daemon calls this from a blocking context.
pub fn resume_agent(name: &str, actor: &str) -> Result<HeartbeatRun, HeartbeatError> {
    let callback = DEFAULT_AGENT_CALLBACK
        .get()
        .cloned()
        .ok_or_else(|| HeartbeatError::NoProvider("no agent callback configured".to_string()))?;
    let path = crate::agents::checklist_path(name);
    let content = std::fs::read_to_string(&path)?;
    let (config, _) = parse_heartbeat(&content);
    let charter = content
        .find("\n---")
        .map_or(content.as_str(), |i| &content[i + 4..]);
    let log = EventLog::open_default().map_err(db_error)?;
    let resumed = agent_beat::resume(&log, actor, name)?;
    let body = resumed.prompt(charter);
    run_beat(
        &log,
        actor,
        &resumed.beat,
        crate::agents::RUN_RESUMED,
        config,
        &path,
        &body,
        &*callback,
    )?
}

/// Run a beat (or a resumed run) of a named agent: in a fresh container if it has an image,
/// then answer it. The outer error is the log's; the inner one is the run's, already answered.
// ponytail: a resumed run gets a fresh container, so what the parked part wrote is gone; keep the
// container while parked if a run needs its files after an answer.
#[allow(clippy::too_many_arguments)]
fn run_beat(
    log: &EventLog,
    actor: &str,
    beat: &agent_beat::Beat,
    kind: &str,
    mut config: HeartbeatConfig,
    path: &Path,
    body: &str,
    callback: &AgentCallback,
) -> Result<Result<HeartbeatRun, HeartbeatError>, HeartbeatError> {
    let (outcome, evidence) = match agent_beat::open_sandbox(log, actor, beat) {
        Err(e) => (Err(e), Vec::new()),
        Ok(sandbox) => {
            config.container = sandbox.as_ref().map(|s| s.container.clone());
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            config.cancel = Some(std::sync::Arc::clone(&cancel));
            let watch = sandbox.as_ref().map(|s| s.watch(cancel));
            let mut outcome = callback(&config, path, body);
            if watch.is_some_and(crate::sandbox::Watch::stop) {
                outcome = Err(HeartbeatError::Agent(
                    "the run's container stopped mid-run; the run was ended".to_string(),
                ));
            }
            let evidence = sandbox
                .map(|s| agent_beat::close_sandbox(log, actor, beat, &s))
                .unwrap_or_default();
            (outcome, evidence)
        }
    };
    agent_beat::finish(log, actor, beat, kind, &outcome, evidence)?;
    Ok(outcome)
}

/// Answer every run that died with the daemon. Call it when the daemon starts, before any
/// beat: an agent the log says is running has no run in this process, so its run ended
/// when the last daemon stopped. Each is answered missed (with `RunResumed` if it was a
/// resumed run), posts a blocked report, has its container removed, and goes back to
/// sleep. Returns the agents recovered.
pub fn recover(log: &EventLog, actor: &str) -> Result<Vec<String>, HeartbeatError> {
    let all = crate::agents::Agents::from_log(log).map_err(HeartbeatError::Agent)?;
    let dead: Vec<_> = all
        .iter()
        .filter(|a| a.state == crate::agents::AgentState::Running)
        .collect();
    for agent in &dead {
        agent_beat::recover(log, actor, agent)?;
    }
    Ok(dead.iter().map(|a| a.name.clone()).collect())
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
    /// The named agent this scheduler beats for, if any.
    agent: Option<String>,
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
                agent: None,
            })),
        }
    }

    /// Set the agent invocation callback.
    pub fn set_agent_callback(&self, callback: Arc<AgentCallback>) {
        let mut state = self.inner.lock().expect("scheduler mutex poisoned");
        state.agent_callback = Some(callback);
    }

    /// Beat for the named agent `name`: each tick is one of its runs. A tick fires only
    /// while the log says the agent is sleeping, and records its state changes, its run,
    /// the run's output and its answer as events about the agent.
    pub fn set_agent(&self, name: &str) {
        let mut state = self.inner.lock().expect("scheduler mutex poisoned");
        state.agent = Some(name.to_string());
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

    /// Start the background scheduler task. The first tick comes one interval from now.
    pub fn start(&self) -> Result<(), HeartbeatError> {
        let interval_secs = {
            let state = self.inner.lock().expect("scheduler mutex poisoned");
            state
                .interval_override
                .unwrap_or(state.config.interval_secs)
        };
        self.start_after(std::time::Duration::from_secs(interval_secs))
    }

    /// Start the background scheduler task with its first tick after `first`, then one
    /// every interval. The daemon uses this to keep a beat's time across restarts.
    pub fn start_after(&self, first: std::time::Duration) -> Result<(), HeartbeatError> {
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
                .background_loop(path, interval_secs, first, stop_rx)
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
        let agent = state.agent.clone();

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

        if let Some(name) = &agent {
            // An agent's state is the log: no log, no beat.
            let log = EventLog::open_default().map_err(db_error)?;
            let beat = agent_beat::start(&log, &actor, name)?;
            let outcome = run_beat(
                &log,
                &actor,
                &beat,
                HEARTBEAT_ANSWERED,
                config.clone(),
                &path,
                body,
                &*callback,
            )?;
            let mut run = outcome?;
            run.checklist_path = path.display().to_string();
            run.interval_secs = config.interval_secs;
            if let Some(ref store) = store {
                store.insert_run(&run)?;
            }
            let mut state = self.inner.lock().expect("scheduler mutex poisoned");
            state.last_run = Some(run.clone());
            state.config = config;
            return Ok(run);
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

    /// The background loop: the first tick after `first`, then one every interval.
    async fn background_loop(
        &self,
        path: PathBuf,
        initial_interval_secs: u64,
        first: std::time::Duration,
        mut stop_rx: watch::Receiver<bool>,
    ) {
        let mut interval_secs = initial_interval_secs;
        let mut wait = first;

        loop {
            tokio::select! {
                () = tokio::time::sleep(wait) => {
                    // Check for interval changes (hot-reload)
                    if let Some(new_interval) = self.check_interval_change(&path, interval_secs) {
                        interval_secs = new_interval;
                    }
                    wait = std::time::Duration::from_secs(interval_secs);

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

/// The events one beat of a named agent writes.
mod agent_beat {
    use colony_tuple::{Address, EventLog, Scope};
    use serde_json::json;

    use super::{db_error, HeartbeatError, HeartbeatRun, HeartbeatStatus};
    use super::{HEARTBEAT_ANSWERED, HEARTBEAT_FIRED};
    use crate::agents::{
        self, AgentSpec, AgentState, Agents, Report, AGENT_STATE_CHANGED, CONTAINER_REMOVED,
        CONTAINER_STARTED, DIFF_RECORDED, NETWORK_BLOCKED, OUTPUT_RECORDED, REPORT_POSTED,
    };
    use crate::sandbox::Sandbox;

    /// A beat, or a resumed run, in progress.
    pub(super) struct Beat {
        name: String,
        run: u64,
        spec: AgentSpec,
        /// The event it follows from: the beat's fire, or the last answer.
        fired: i64,
        /// The number its first output takes.
        first_output: u64,
    }

    /// A parked run going on.
    pub(super) struct Resumed {
        pub(super) beat: Beat,
        report: Report,
        answers: Vec<String>,
    }

    impl Resumed {
        /// What the model is told: its charter, what it said, and the answers.
        pub(super) fn prompt(&self, charter: &str) -> String {
            let mut out = format!(
                "{}\n\n## Your run so far\n\nYou wrote:\n\n{}\n\nYou asked, and were answered:\n",
                charter.trim(),
                self.report.body
            );
            for (ask, answer) in self.report.asks.iter().zip(&self.answers) {
                out.push_str(&format!("\nASKED: {}\nANSWER: {answer}\n", ask.question));
            }
            out.push_str("\nGo on with the run using these answers. Report as before.\n");
            out
        }
    }

    /// What a beat's status means for the agent's answer.
    fn outcome(status: HeartbeatStatus) -> &'static str {
        match status {
            HeartbeatStatus::Pass => "idle",
            HeartbeatStatus::Warn | HeartbeatStatus::Alert | HeartbeatStatus::Unstructured => {
                "acted"
            }
            HeartbeatStatus::Timeout | HeartbeatStatus::Error => "missed",
        }
    }

    fn agent_err(msg: String) -> HeartbeatError {
        HeartbeatError::Agent(msg)
    }

    /// Fire a beat: the agent goes from sleeping to running, and its next run starts.
    /// A beat past the daily budget is fired and answered missed at once.
    pub(super) fn start(log: &EventLog, actor: &str, name: &str) -> Result<Beat, HeartbeatError> {
        let all = Agents::from_log(log).map_err(agent_err)?;
        let agent = all
            .get(name)
            .ok_or_else(|| agent_err(format!("no agent named {name}")))?;
        if agent.state != AgentState::Sleeping {
            return Err(agent_err(format!(
                "{name} is {}, beat skipped",
                agent.state
            )));
        }
        let run = agent.runs + 1;
        let run_addr = agents::run_address(name, run);
        let spent = agents::tokens_today(log, name).map_err(agent_err)?;
        if spent >= agent.spec.daily_tokens {
            let fired = log
                .append(
                    actor,
                    HEARTBEAT_FIRED,
                    Some(&run_addr),
                    None,
                    json!({ "beat": run }),
                )
                .map_err(db_error)?;
            let why = format!(
                "daily token budget spent ({spent} of {})",
                agent.spec.daily_tokens
            );
            let answered = log
                .append(
                    actor,
                    HEARTBEAT_ANSWERED,
                    Some(&run_addr),
                    Some(fired),
                    json!({ "outcome": "missed", "reason": why, "tokens": 0 }),
                )
                .map_err(db_error)?;
            let evidence = vec![format!("colony://local/event/{answered}")];
            post(
                log,
                actor,
                &run_addr,
                answered,
                Report::from_output(&why, true, evidence),
            )?;
            return Err(agent_err(why));
        }
        log.append(
            actor,
            AGENT_STATE_CHANGED,
            Some(&agent.address()),
            None,
            json!({ "from": AgentState::Sleeping, "to": AgentState::Running, "run": run }),
        )
        .map_err(db_error)?;
        let fired = log
            .append(
                actor,
                HEARTBEAT_FIRED,
                Some(&run_addr),
                None,
                json!({ "beat": run }),
            )
            .map_err(db_error)?;
        Ok(Beat {
            name: name.to_string(),
            run,
            spec: agent.spec.clone(),
            fired,
            first_output: 1,
        })
    }

    /// Wake a parked agent whose asks are all answered: parked to running, on the same run.
    pub(super) fn resume(
        log: &EventLog,
        actor: &str,
        name: &str,
    ) -> Result<Resumed, HeartbeatError> {
        let all = Agents::from_log(log).map_err(agent_err)?;
        let agent = all
            .get(name)
            .ok_or_else(|| agent_err(format!("no agent named {name}")))?;
        if !agent.ready_to_resume() {
            return Err(agent_err(format!("{name} is not waiting to resume")));
        }
        let report = agent
            .pending
            .clone()
            .expect("ready_to_resume checks pending");
        let answers: Vec<_> = report
            .asks
            .iter()
            .filter_map(|a| agent.answers.get(&a.address))
            .collect();
        let last_answer = answers
            .iter()
            .max_by_key(|a| a.at_ms)
            .map(|a| a.at_ms)
            .unwrap_or_default();
        let run_addr = agents::run_address(name, agent.runs);
        let outputs = log
            .about(&run_addr)
            .map_err(db_error)?
            .iter()
            .filter(|ev| ev.kind == OUTPUT_RECORDED)
            .count() as u64;
        let changed = log
            .append(
                actor,
                AGENT_STATE_CHANGED,
                Some(&agent.address()),
                None,
                json!({ "from": AgentState::Parked, "to": AgentState::Running, "run": agent.runs, "answered_at": last_answer }),
            )
            .map_err(db_error)?;
        Ok(Resumed {
            beat: Beat {
                name: name.to_string(),
                run: agent.runs,
                spec: agent.spec.clone(),
                fired: changed,
                first_output: outputs + 1,
            },
            answers: answers.iter().map(|a| a.text.clone()).collect(),
            report,
        })
    }

    /// Start the beat's container if its agent has an image. `None`: it runs without one.
    /// A container that does not start fails the run: it never falls back to this machine.
    pub(super) fn open_sandbox(
        log: &EventLog,
        actor: &str,
        beat: &Beat,
    ) -> Result<Option<Sandbox>, HeartbeatError> {
        let Some(image) = &beat.spec.image else {
            return Ok(None);
        };
        let sandbox = Sandbox::start(
            &beat.name,
            beat.run,
            image,
            &beat.spec.workdir,
            &beat.spec.grants,
        )
        .map_err(|e| agent_err(format!("the run's container did not start: {e}")))?;
        let run_addr = agents::run_address(&beat.name, beat.run);
        let data = json!({ "container": sandbox.container, "image": image, "grants": beat.spec.grants, "skipped": sandbox.skipped });
        if let Err(e) = log.append(
            actor,
            CONTAINER_STARTED,
            Some(&run_addr),
            Some(beat.fired),
            data,
        ) {
            sandbox.remove();
            return Err(db_error(e));
        }
        Ok(Some(sandbox))
    }

    /// Record what the run's container saw and changed, then remove it. Returns the
    /// evidence: the diff, and each blocked call. Never fails: a log error is a warning.
    pub(super) fn close_sandbox(
        log: &EventLog,
        actor: &str,
        beat: &Beat,
        sandbox: &Sandbox,
    ) -> Vec<String> {
        let verdicts = sandbox.verdicts();
        let diff = sandbox.diff();
        sandbox.remove();
        let run_addr = agents::run_address(&beat.name, beat.run);
        let mut evidence = Vec::new();
        let append = |kind: &str, subject: &Address, data: serde_json::Value| match log.append(
            actor,
            kind,
            Some(subject),
            Some(beat.fired),
            data,
        ) {
            Ok(seq) => Some(seq),
            Err(e) => {
                tracing::warn!("{}: could not record {kind}: {e}", beat.name);
                None
            }
        };
        for v in verdicts.iter().filter(|v| !v.allowed) {
            if let Some(seq) = append(
                NETWORK_BLOCKED,
                &run_addr,
                json!({ "host": v.host, "port": v.port }),
            ) {
                evidence.push(format!("colony://local/event/{seq}"));
            }
        }
        match diff {
            Ok(text) if !text.is_empty() => {
                let addr = Address::local(Scope::Diff {
                    agent: beat.name.clone(),
                    run: beat.run,
                });
                if append(DIFF_RECORDED, &addr, json!({ "content": text })).is_some() {
                    evidence.push(addr.to_string());
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("{}: no diff for run {}: {e}", beat.name, beat.run),
        }
        append(
            CONTAINER_REMOVED,
            &run_addr,
            json!({ "container": sandbox.container }),
        );
        evidence
    }

    /// Answer one agent's dead run. See [`super::recover`].
    pub(super) fn recover(
        log: &EventLog,
        actor: &str,
        agent: &agents::AgentRecord,
    ) -> Result<(), HeartbeatError> {
        let run_addr = agents::run_address(&agent.name, agent.runs);
        let events = log.about(&run_addr).map_err(db_error)?;
        let count = |kind: &str| events.iter().filter(|e| e.kind == kind).count();
        let fired = events
            .iter()
            .rev()
            .find(|e| e.kind == HEARTBEAT_FIRED)
            .map(|e| e.seq);
        if agent.spec.image.is_some() {
            let sandbox = Sandbox::named(&agent.name, agent.runs);
            sandbox.remove();
            if count(CONTAINER_STARTED) > count(CONTAINER_REMOVED) {
                log.append(
                    actor,
                    CONTAINER_REMOVED,
                    Some(&run_addr),
                    fired,
                    json!({ "container": sandbox.container, "reason": "the daemon stopped mid-run" }),
                )
                .map_err(db_error)?;
            }
        }
        // A run already answered once was parked and resumed: this is the resumed part.
        let kind = if count(HEARTBEAT_ANSWERED) > 0 {
            agents::RUN_RESUMED
        } else {
            HEARTBEAT_ANSWERED
        };
        let why = "the run died: the daemon stopped while it was running";
        let answered = log
            .append(
                actor,
                kind,
                Some(&run_addr),
                fired,
                json!({ "outcome": "missed", "status": "error", "error": why, "tokens": 0 }),
            )
            .map_err(db_error)?;
        let evidence = vec![format!("colony://local/event/{answered}")];
        post(
            log,
            actor,
            &run_addr,
            answered,
            Report::from_output(why, true, evidence),
        )?;
        log.append(
            actor,
            AGENT_STATE_CHANGED,
            Some(&agent.address()),
            None,
            json!({ "from": AgentState::Running, "to": AgentState::Sleeping, "run": agent.runs }),
        )
        .map_err(db_error)?;
        tracing::warn!(
            "{}: run {} died with the daemon; answered missed",
            agent.name,
            agent.runs
        );
        Ok(())
    }

    fn post(
        log: &EventLog,
        actor: &str,
        run_addr: &Address,
        cause: i64,
        mut report: Report,
    ) -> Result<(), HeartbeatError> {
        for (n, ask) in (1..).zip(&mut report.asks) {
            ask.address = format!("{run_addr}/ask/{n}");
        }
        let data = serde_json::to_value(report).map_err(|e| agent_err(e.to_string()))?;
        log.append(actor, REPORT_POSTED, Some(run_addr), Some(cause), data)
            .map_err(db_error)?;
        Ok(())
    }

    /// Answer a beat (`kind` is `HeartbeatAnswered`) or a resumed run (`RunResumed`):
    /// record its outputs, its answer and, unless it was idle, its report. Then the agent
    /// parks if the report asks something, else sleeps; unless something else (a pause)
    /// changed its state meanwhile.
    pub(super) fn finish(
        log: &EventLog,
        actor: &str,
        beat: &Beat,
        kind: &str,
        outcome_of_run: &Result<HeartbeatRun, HeartbeatError>,
        sandbox_evidence: Vec<String>,
    ) -> Result<(), HeartbeatError> {
        let run_addr = agents::run_address(&beat.name, beat.run);
        let mut evidence = Vec::new();
        let mut record = |n: u64, data: serde_json::Value| -> Result<(), HeartbeatError> {
            let addr = Address::local(Scope::Output {
                agent: beat.name.clone(),
                run: beat.run,
                n,
            });
            log.append(actor, OUTPUT_RECORDED, Some(&addr), Some(beat.fired), data)
                .map_err(db_error)?;
            evidence.push(addr.to_string());
            Ok(())
        };
        let (answer, text, missed) = match outcome_of_run {
            Ok(run) => {
                let first = beat.first_output;
                if !run.raw_output.is_empty() {
                    record(first, json!({ "content": run.raw_output }))?;
                }
                for (n, call) in (first + 1..).zip(&run.tool_calls) {
                    record(
                        n,
                        json!({ "content": call.output, "tool": call.tool, "input": call.input }),
                    )?;
                }
                let answer = json!({
                    "outcome": outcome(run.status),
                    "status": run.status.to_string(),
                    "run": run.id,
                    "findings": run.findings.len(),
                    "tokens": run.tokens_used,
                });
                (
                    answer,
                    run.raw_output.clone(),
                    outcome(run.status) == "missed",
                )
            }
            Err(e) => (
                json!({ "outcome": "missed", "status": "error", "error": e.to_string(), "tokens": 0 }),
                e.to_string(),
                true,
            ),
        };
        let mut answer = answer;
        evidence.extend(sandbox_evidence);
        let mut report = Report::from_output(&text, missed, evidence);
        // An idle run that asks something still needs you.
        if !report.asks.is_empty() {
            answer["outcome"] = json!("needs_you");
        }
        let idle = answer["outcome"] == "idle";
        let parks = !report.asks.is_empty();
        let answered = log
            .append(actor, kind, Some(&run_addr), Some(beat.fired), answer)
            .map_err(db_error)?;
        if !idle {
            if report.evidence.is_empty() {
                report
                    .evidence
                    .push(format!("colony://local/event/{answered}"));
            }
            post(log, actor, &run_addr, answered, report)?;
        }

        let all = Agents::from_log(log).map_err(agent_err)?;
        if let Some(agent) = all
            .get(&beat.name)
            .filter(|a| a.state == AgentState::Running)
        {
            let to = if parks {
                AgentState::Parked
            } else {
                AgentState::Sleeping
            };
            log.append(
                actor,
                AGENT_STATE_CHANGED,
                Some(&agent.address()),
                None,
                json!({ "from": AgentState::Running, "to": to, "run": beat.run }),
            )
            .map_err(db_error)?;
        }
        Ok(())
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

    mod recovery {
        use super::super::recover;
        use crate::agents::*;
        use crate::heartbeat::{HEARTBEAT_ANSWERED, HEARTBEAT_FIRED};
        use colony_tuple::EventLog;
        use serde_json::json;

        fn running_agent(log: &EventLog) {
            let spec = AgentSpec {
                charter: "c".into(),
                every_secs: 60,
                mode: crate::heartbeat::SecurityMode::Notify,
                workdir: "/w".into(),
                timeout_secs: 30,
                max_tool_calls: 5,
                token_budget: 100,
                daily_tokens: 1000,
                notes: vec![],
                image: None,
                grants: vec![],
            };
            let agent = agent_address("a");
            log.append(
                "t",
                AGENT_CREATED,
                Some(&agent),
                None,
                serde_json::to_value(spec).unwrap(),
            )
            .unwrap();
            log.append(
                "d",
                AGENT_STATE_CHANGED,
                Some(&agent),
                None,
                json!({"from": "sleeping", "to": "running"}),
            )
            .unwrap();
            log.append(
                "d",
                HEARTBEAT_FIRED,
                Some(&run_address("a", 1)),
                None,
                json!({}),
            )
            .unwrap();
        }

        fn kinds(log: &EventLog) -> Vec<String> {
            log.about(&run_address("a", 1))
                .unwrap()
                .into_iter()
                .map(|e| e.kind)
                .collect()
        }

        #[test]
        fn a_run_that_died_with_the_daemon_is_answered_missed_and_the_agent_sleeps() {
            let dir = tempfile::tempdir().unwrap();
            let log = EventLog::open(&dir.path().join("m.db")).unwrap();
            running_agent(&log);
            assert_eq!(recover(&log, "daemon").unwrap(), ["a"]);
            assert_eq!(
                kinds(&log),
                [HEARTBEAT_FIRED, HEARTBEAT_ANSWERED, REPORT_POSTED]
            );
            let answer = log.of_kind(HEARTBEAT_ANSWERED, 1).unwrap().remove(0);
            assert_eq!(answer.data["outcome"], "missed");
            let report = log.of_kind(REPORT_POSTED, 1).unwrap().remove(0);
            assert_eq!(report.data["status"], "blocked");
            let agents = Agents::from_log(&log).unwrap();
            assert_eq!(agents.get("a").unwrap().state, AgentState::Sleeping);
            assert!(recover(&log, "daemon").unwrap().is_empty(), "once only");
        }

        #[test]
        fn a_resumed_run_that_died_is_answered_as_a_resume() {
            let dir = tempfile::tempdir().unwrap();
            let log = EventLog::open(&dir.path().join("m.db")).unwrap();
            running_agent(&log);
            log.append(
                "d",
                HEARTBEAT_ANSWERED,
                Some(&run_address("a", 1)),
                None,
                json!({"outcome": "needs_you"}),
            )
            .unwrap();
            recover(&log, "daemon").unwrap();
            assert_eq!(kinds(&log).last().map(String::as_str), Some(REPORT_POSTED));
            assert_eq!(log.of_kind(RUN_RESUMED, 5).unwrap().len(), 1);
        }
    }

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
