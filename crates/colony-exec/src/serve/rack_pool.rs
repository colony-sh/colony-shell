//! Rack worker pool -- manages Ruby child processes for Rack app serving.
//!
//! Each worker is a Ruby process running `rack_worker.rb` that communicates
//! via JSON-over-stdio. The pool dispatches requests round-robin and handles
//! worker crashes, timeouts, and graceful restarts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

/// Status of an individual worker.
#[derive(Debug, Clone)]
pub enum WorkerStatus {
    /// Ready to accept a request.
    Idle,
    /// Currently processing a request (tracked start time).
    Busy(Instant),
    /// Process has exited or been killed.
    Dead,
}

/// JSON request sent to worker stdin.
#[derive(Debug, Serialize)]
pub struct RackRequest {
    pub method: String,
    pub path: String,
    pub query: String,
    pub host: String,
    pub port: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// JSON response read from worker stdout.
#[derive(Debug, Deserialize)]
pub struct RackResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// A single Rack worker -- a Ruby child process.
///
/// Each worker has exclusive access to its stdin/stdout via `Mutex`.
/// The mutex is held for the duration of a single request-response cycle.
pub struct RackWorker {
    /// Child process handle (for kill/wait).
    child: Child,
    /// Write requests as JSON lines.
    stdin: ChildStdin,
    /// Read responses as JSON lines.
    stdout: BufReader<ChildStdout>,
    /// Current worker status.
    pub status: WorkerStatus,
    /// Number of requests this worker has handled.
    pub requests_handled: u64,
    /// When this worker was spawned.
    pub spawned_at: Instant,
}

impl RackWorker {
    /// Spawn a new Ruby worker process.
    ///
    /// Runs: `bundle exec ruby <worker_script> config.ru`
    /// with working directory set to `app_dir`.
    pub fn spawn(app_dir: &Path, worker_script: &Path) -> std::io::Result<Self> {
        let mut child = tokio::process::Command::new("bundle")
            .args(["exec", "ruby"])
            .arg(worker_script)
            .arg("config.ru")
            .current_dir(app_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit()) // worker stderr -> colony log
            .spawn()?;

        let stdin = child.stdin.take().expect("child stdin not captured");
        let stdout = child.stdout.take().expect("child stdout not captured");

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            status: WorkerStatus::Idle,
            requests_handled: 0,
            spawned_at: Instant::now(),
        })
    }

    /// Send a request and read the response.
    ///
    /// Returns `None` if the worker has died or produced invalid JSON.
    pub async fn send_request(&mut self, req: &RackRequest) -> Option<RackResponse> {
        self.status = WorkerStatus::Busy(Instant::now());

        // Serialize request to JSON line
        let Ok(mut line) = serde_json::to_string(req) else {
            self.status = WorkerStatus::Idle;
            return None;
        };
        line.push('\n');

        // Write to stdin
        if self.stdin.write_all(line.as_bytes()).await.is_err() {
            self.status = WorkerStatus::Dead;
            return None;
        }
        if self.stdin.flush().await.is_err() {
            self.status = WorkerStatus::Dead;
            return None;
        }

        // Read response line from stdout
        let mut response_line = String::new();
        match self.stdout.read_line(&mut response_line).await {
            Ok(0) => {
                // EOF -- worker exited
                self.status = WorkerStatus::Dead;
                return None;
            }
            Ok(_) => {}
            Err(_) => {
                self.status = WorkerStatus::Dead;
                return None;
            }
        }

        // Parse JSON response
        let Ok(response) = serde_json::from_str::<RackResponse>(response_line.trim()) else {
            self.status = WorkerStatus::Dead;
            return None;
        };

        self.requests_handled += 1;
        self.status = WorkerStatus::Idle;
        Some(response)
    }

    /// Check if this worker's process is still alive.
    #[allow(dead_code)]
    pub fn is_alive(&mut self) -> bool {
        // try_wait returns Ok(Some(status)) if exited, Ok(None) if still running
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Kill the worker process.
    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
        self.status = WorkerStatus::Dead;
    }

    /// PID of the child process.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }
}

/// Error from dispatch.
#[derive(Debug)]
pub enum DispatchError {
    /// No workers in the pool.
    NoWorkers,
    /// Worker crashed or produced invalid JSON.
    WorkerCrashed,
    /// Request exceeded the timeout.
    Timeout,
    /// Failed to spawn a replacement worker.
    SpawnFailed,
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWorkers => write!(f, "no workers available"),
            Self::WorkerCrashed => write!(f, "worker crashed"),
            Self::Timeout => write!(f, "request timed out"),
            Self::SpawnFailed => write!(f, "failed to spawn worker"),
        }
    }
}

/// Summary info for a single worker (for `serve workers` display).
pub struct WorkerInfo {
    pub index: usize,
    pub pid: Option<u32>,
    pub status: String,
    pub requests_handled: u64,
    pub uptime_secs: u64,
}

/// Pool of Rack workers with round-robin dispatch.
pub struct RackWorkerPool {
    /// Workers behind individual mutexes for per-worker locking.
    workers: Vec<Arc<Mutex<RackWorker>>>,
    /// Application directory (working dir for workers).
    app_dir: PathBuf,
    /// Path to the worker script.
    worker_script: PathBuf,
    /// Round-robin index.
    next_worker: AtomicUsize,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl RackWorkerPool {
    /// Create a new pool and spawn `count` workers.
    pub fn new(
        app_dir: PathBuf,
        worker_script: PathBuf,
        count: usize,
        timeout_secs: u64,
    ) -> std::io::Result<Self> {
        let mut workers = Vec::with_capacity(count);

        for _ in 0..count {
            let worker = RackWorker::spawn(&app_dir, &worker_script)?;
            workers.push(Arc::new(Mutex::new(worker)));
        }

        Ok(Self {
            workers,
            app_dir,
            worker_script,
            next_worker: AtomicUsize::new(0),
            timeout_secs,
        })
    }

    /// Dispatch a request to the next available worker (round-robin).
    ///
    /// Applies a timeout. Returns a `RackResponse` or an error.
    /// On worker failure (crash, invalid JSON), returns 502 and respawns.
    /// On timeout, kills the worker, respawns, and returns 504.
    pub async fn dispatch(&self, req: &RackRequest) -> Result<RackResponse, DispatchError> {
        let worker_count = self.workers.len();
        if worker_count == 0 {
            return Err(DispatchError::NoWorkers);
        }

        // Round-robin: try each worker starting from next_worker
        let start_idx = self.next_worker.fetch_add(1, Ordering::Relaxed) % worker_count;

        for attempt in 0..worker_count {
            let idx = (start_idx + attempt) % worker_count;
            let worker_arc = &self.workers[idx];

            // Try to acquire the lock (non-blocking for busy workers)
            let Ok(mut worker) = worker_arc.try_lock() else {
                continue; // Worker is busy, try next
            };

            // Skip dead workers
            if matches!(worker.status, WorkerStatus::Dead) {
                continue;
            }

            // Send request with timeout
            let timeout = tokio::time::Duration::from_secs(self.timeout_secs);
            match tokio::time::timeout(timeout, worker.send_request(req)).await {
                Ok(Some(response)) => return Ok(response),
                Ok(None) => {
                    // Worker crashed or invalid JSON -- mark dead, respawn
                    worker.status = WorkerStatus::Dead;
                    drop(worker);
                    self.respawn_worker(idx).await;
                    return Err(DispatchError::WorkerCrashed);
                }
                Err(_) => {
                    // Timeout -- kill worker, respawn
                    worker.kill().await;
                    drop(worker);
                    self.respawn_worker(idx).await;
                    return Err(DispatchError::Timeout);
                }
            }
        }

        // All workers are busy -- block on the next one in rotation
        let idx = start_idx % worker_count;
        let worker_arc = &self.workers[idx];
        let mut worker = worker_arc.lock().await;

        if matches!(worker.status, WorkerStatus::Dead) {
            // Respawn inline
            match RackWorker::spawn(&self.app_dir, &self.worker_script) {
                Ok(new_worker) => *worker = new_worker,
                Err(_) => return Err(DispatchError::SpawnFailed),
            }
        }

        let timeout = tokio::time::Duration::from_secs(self.timeout_secs);
        match tokio::time::timeout(timeout, worker.send_request(req)).await {
            Ok(Some(response)) => Ok(response),
            Ok(None) => {
                worker.status = WorkerStatus::Dead;
                Err(DispatchError::WorkerCrashed)
            }
            Err(_) => {
                worker.kill().await;
                Err(DispatchError::Timeout)
            }
        }
    }

    /// Respawn a dead worker at the given index.
    async fn respawn_worker(&self, idx: usize) {
        let worker_arc = &self.workers[idx];
        let mut worker = worker_arc.lock().await;

        if !matches!(worker.status, WorkerStatus::Dead) {
            return; // Already respawned by another task
        }

        match RackWorker::spawn(&self.app_dir, &self.worker_script) {
            Ok(new_worker) => {
                eprintln!("serve: rack worker {idx} respawned");
                *worker = new_worker;
            }
            Err(e) => {
                eprintln!("serve: failed to respawn rack worker {idx}: {e}");
            }
        }
    }

    /// Number of workers in the pool.
    #[allow(dead_code)]
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Graceful restart: spawn new workers, drain old (up to 30s), kill old.
    ///
    /// This enables zero-downtime deployment (like Unicorn's USR2):
    /// 1. Spawn N new workers (they load the app fresh -- picks up code changes)
    /// 2. Wait for in-flight requests on old workers to complete (up to 30s)
    /// 3. Kill old workers that haven't finished
    pub async fn restart(&mut self) -> std::io::Result<()> {
        let count = self.workers.len();
        let mut new_workers = Vec::with_capacity(count);

        // 1. Spawn new workers
        for _ in 0..count {
            let worker = RackWorker::spawn(&self.app_dir, &self.worker_script)?;
            new_workers.push(Arc::new(Mutex::new(worker)));
        }

        // 2. Swap: old workers are still behind Arc<Mutex>, in-flight requests hold locks
        let old_workers = std::mem::replace(&mut self.workers, new_workers);

        // 3. Drain old workers: wait for locks (in-flight requests) up to 30s
        let drain_timeout = tokio::time::Duration::from_secs(30);
        for (i, old) in old_workers.iter().enumerate() {
            if let Ok(mut worker) = tokio::time::timeout(drain_timeout, old.lock()).await {
                worker.kill().await;
            } else {
                // Force-kill if drain times out
                if let Ok(mut worker) = old.try_lock() {
                    worker.kill().await;
                }
                eprintln!("serve: rack worker {i} drain timed out, force-killed");
            }
        }

        eprintln!("serve: rack workers restarted ({count} workers)");
        Ok(())
    }

    /// Scale the pool to `target` workers.
    ///
    /// If target > current: spawn additional workers.
    /// If target < current: drain and kill excess workers (newest first).
    pub async fn scale(&mut self, target: usize) -> std::io::Result<()> {
        let current = self.workers.len();

        if target > current {
            // Spawn additional workers
            for _ in current..target {
                let worker = RackWorker::spawn(&self.app_dir, &self.worker_script)?;
                self.workers.push(Arc::new(Mutex::new(worker)));
            }
        } else if target < current {
            // Remove excess workers (newest first)
            let excess = self.workers.split_off(target);
            for worker_arc in &excess {
                let mut worker = worker_arc.lock().await;
                worker.kill().await;
            }
        }

        Ok(())
    }

    /// Shutdown all workers.
    pub async fn shutdown(&self) {
        for worker_arc in &self.workers {
            let mut worker = worker_arc.lock().await;
            worker.kill().await;
        }
    }

    /// Get worker info for `serve workers` display.
    pub async fn worker_info(&self) -> Vec<WorkerInfo> {
        let mut info = Vec::with_capacity(self.workers.len());
        for (i, worker_arc) in self.workers.iter().enumerate() {
            let worker = worker_arc.lock().await;
            let status_str = match &worker.status {
                WorkerStatus::Idle => "idle".to_string(),
                WorkerStatus::Busy(since) => format!("busy ({}ms)", since.elapsed().as_millis()),
                WorkerStatus::Dead => "dead".to_string(),
            };
            info.push(WorkerInfo {
                index: i,
                pid: worker.pid(),
                status: status_str,
                requests_handled: worker.requests_handled,
                uptime_secs: worker.spawned_at.elapsed().as_secs(),
            });
        }
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rack_request_serializes_to_json() {
        let req = RackRequest {
            method: "GET".to_string(),
            path: "/users".to_string(),
            query: "page=1".to_string(),
            host: "localhost".to_string(),
            port: "8080".to_string(),
            headers: [("Accept".to_string(), "text/html".to_string())]
                .into_iter()
                .collect(),
            body: String::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""method":"GET"#));
        assert!(json.contains(r#""path":"/users"#));
    }

    #[test]
    fn rack_response_deserializes_from_json() {
        let json =
            r#"{"status":200,"headers":{"Content-Type":"text/html"},"body":"<html></html>"}"#;
        let resp: RackResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.headers.get("Content-Type").unwrap(), "text/html");
        assert_eq!(resp.body, "<html></html>");
    }

    #[test]
    fn rack_response_handles_500() {
        let json = r#"{"status":500,"headers":{"Content-Type":"text/plain"},"body":"Internal Server Error: boom"}"#;
        let resp: RackResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, 500);
        assert!(resp.body.contains("boom"));
    }

    #[test]
    fn rack_request_with_body() {
        let req = RackRequest {
            method: "POST".to_string(),
            path: "/api/users".to_string(),
            query: String::new(),
            host: "localhost".to_string(),
            port: "3000".to_string(),
            headers: [("Content-Type".to_string(), "application/json".to_string())]
                .into_iter()
                .collect(),
            body: r#"{"name":"Alice"}"#.to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["body"], r#"{"name":"Alice"}"#);
    }

    #[test]
    fn worker_info_default_values() {
        let info = WorkerInfo {
            index: 0,
            pid: Some(12345),
            status: "idle".to_string(),
            requests_handled: 42,
            uptime_secs: 300,
        };
        assert_eq!(info.index, 0);
        assert_eq!(info.pid, Some(12345));
        assert_eq!(info.requests_handled, 42);
    }

    #[test]
    fn rack_response_with_empty_body() {
        let json = r#"{"status":204,"headers":{},"body":""}"#;
        let resp: RackResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, 204);
        assert!(resp.body.is_empty());
    }

    #[test]
    fn rack_response_with_many_headers() {
        let json = r#"{"status":200,"headers":{"Content-Type":"text/html","X-Request-Id":"abc123","Set-Cookie":"session=xyz"},"body":"ok"}"#;
        let resp: RackResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.headers.len(), 3);
    }

    #[test]
    fn rack_request_empty_headers() {
        let req = RackRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            query: String::new(),
            host: "localhost".to_string(),
            port: "8080".to_string(),
            headers: HashMap::new(),
            body: String::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""headers":{}"#));
    }

    #[test]
    fn dispatch_error_display() {
        assert_eq!(DispatchError::NoWorkers.to_string(), "no workers available");
        assert_eq!(DispatchError::Timeout.to_string(), "request timed out");
        assert_eq!(DispatchError::WorkerCrashed.to_string(), "worker crashed");
        assert_eq!(
            DispatchError::SpawnFailed.to_string(),
            "failed to spawn worker"
        );
    }
}
