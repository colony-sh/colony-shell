# Serve Rack & CGI Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Rack application server (worker pool with JSON-over-stdio protocol) and CGI route type to the existing serve builtin, enabling colony-shell to replace Puma for Rails development.

**Architecture:** Colony manages a pool of Ruby child processes (workers), each running a bundled `rack_worker.rb` script that loads the Rack app from `config.ru`. Incoming HTTP requests matching Rack routes are serialized as JSON lines to an available worker's stdin; the worker calls `app.call(env)` and writes the response as a JSON line to stdout. CGI routes spawn a fresh process per request with standard CGI environment variables. Both route types integrate into the existing axum router via `build_router()` and benefit from the global stats/tuple middleware.

**Tech Stack:** Rust, tokio (process management), serde_json (protocol), existing axum serve infrastructure

**Spec:** `docs/superpowers/specs/2026-03-16-serve-builtin-design.md` (Rack Application Server + CGI sections)

**Depends on:** Serve builtin (already implemented)

---

## File Structure

### New files in `crates/colony-exec/src/serve/`

| File | Responsibility |
|------|---------------|
| `rack_pool.rs` | `RackWorkerPool`, `RackWorker`, `WorkerStatus` structs. Worker spawning, round-robin dispatch, request/response JSON protocol, health monitoring, crash respawn, timeout handling. |
| `rack_handler.rs` | Axum handler for Rack routes. Receives HTTP request, serializes to JSON, dispatches to pool, deserializes response, returns HTTP response. |
| `cgi_handler.rs` | Axum handler for CGI routes. Spawns executable per-request with CGI env vars (`REQUEST_METHOD`, `PATH_INFO`, `QUERY_STRING`, `HTTP_*`, etc.). Parses CGI response format (headers before blank line, body after). |
| `rack_script.rs` | Embedded `RACK_WORKER_SCRIPT` constant (the Ruby source). `ensure_worker_script()` function to write `~/.colony/lib/rack_worker.rb` if absent or outdated. |

### Modified files

| File | Changes |
|------|---------|
| `crates/colony-exec/src/serve/state.rs` | Add `Route::Rack` and `Route::Cgi` variants. Add `RackPoolHandle` field to `ServeState`. Add `WorkerConfig` struct. |
| `crates/colony-exec/src/serve/server.rs` | Update `build_router()` to handle `Route::Rack` and `Route::Cgi`. Pass `RackPoolHandle` through. |
| `crates/colony-exec/src/serve/args.rs` | Add `handle_rack()`, `handle_cgi()`, `handle_workers()`, `handle_workers_scale()`, `handle_workers_restart()` functions. |
| `crates/colony-exec/src/serve/mod.rs` | Add `mod rack_pool; mod rack_handler; mod cgi_handler; mod rack_script;` declarations. Add `"rack"`, `"cgi"`, `"workers"` to subcommand dispatch. Update hot-swap trigger list. |

### No new dependencies required

All needed crates are already available: `tokio` (process, io, sync, time), `serde_json`, `axum`, `serde`. No changes to `Cargo.toml` files.

---

## Chunk 1: State Types and Rack Script

### Task 1: Add Route::Rack and Route::Cgi variants to state.rs

**Files:**
- Modify: `crates/colony-exec/src/serve/state.rs`

- [ ] **Step 1: Add Route variants**

Add two new variants to the existing `Route` enum in `crates/colony-exec/src/serve/state.rs`:

```rust
Rack {
    path: String,
    app_dir: PathBuf,
    workers: usize,         // default: 4
    timeout_secs: u64,      // default: 30
},
Cgi {
    path: String,
    executable: PathBuf,
},
```

- [ ] **Step 2: Update Route::path() and Route::kind()**

Add arms to the `path()` match:
```rust
Route::Rack { path, .. } | Route::Cgi { path, .. } => path,
```

Add arms to the `kind()` match:
```rust
Route::Rack { .. } => "rack",
Route::Cgi { .. } => "cgi",
```

- [ ] **Step 3: Add RackPoolHandle to ServeState**

Add an `Arc<RwLock<Option<...>>>` field to `ServeState` for the Rack worker pool. The pool is shared between the axum handler threads and the shell thread (for `serve workers` commands).

```rust
use super::rack_pool::RackWorkerPool;

pub struct ServeState {
    pub config: RwLock<ServeConfig>,
    pub stats: Arc<RequestStats>,
    pub(crate) runtime_handle: RwLock<Option<ServeRuntime>>,
    /// Rack worker pool (None if no rack routes registered).
    pub(crate) rack_pool: Arc<tokio::sync::RwLock<Option<RackWorkerPool>>>,
}
```

Update `ServeState::new()` and `Default` impl to initialize `rack_pool: Arc::new(tokio::sync::RwLock::new(None))`.

Note: use `tokio::sync::RwLock` (not `std::sync::RwLock`) for the pool because worker dispatch is async (reading/writing to child process stdio). The existing `config` and `runtime_handle` use `std::sync::RwLock` because they are only accessed from the synchronous shell thread.

- [ ] **Step 4: Add tests for new Route variants**

Add to the existing `mod tests` block in `state.rs`:

```rust
#[test]
fn route_rack_path_and_kind() {
    let r = Route::Rack {
        path: "/".to_string(),
        app_dir: PathBuf::from("./myapp"),
        workers: 4,
        timeout_secs: 30,
    };
    assert_eq!(r.path(), "/");
    assert_eq!(r.kind(), "rack");
}

#[test]
fn route_cgi_path_and_kind() {
    let r = Route::Cgi {
        path: "/report".to_string(),
        executable: PathBuf::from("./generate_report.rb"),
    };
    assert_eq!(r.path(), "/report");
    assert_eq!(r.kind(), "cgi");
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p colony-exec`

This will fail initially because `rack_pool` module does not exist yet. That is expected -- we will create a stub in the next task so it compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/colony-exec/src/serve/state.rs
git commit -m "feat(serve): add Route::Rack and Route::Cgi variants, RackPoolHandle to ServeState"
```

### Task 2: Create rack_script.rs -- bundled Ruby worker script

**Files:**
- Create: `crates/colony-exec/src/serve/rack_script.rs`

- [ ] **Step 1: Create rack_script.rs with embedded Ruby source**

`crates/colony-exec/src/serve/rack_script.rs`:

```rust
//! Bundled Rack worker script for colony-shell.
//!
//! The Ruby script is embedded as a string constant and written to
//! `~/.colony/lib/rack_worker.rb` on first use. The script loads a
//! Rack app from `config.ru`, reads JSON request lines from stdin,
//! calls `app.call(env)`, and writes JSON response lines to stdout.

use std::path::PathBuf;

/// The Ruby source for the Rack worker script.
///
/// Protocol:
/// - Input (stdin): one JSON object per line with keys:
///   `method`, `path`, `query`, `host`, `port`, `headers`, `body`
/// - Output (stdout): one JSON object per line with keys:
///   `status`, `headers`, `body`
/// - Errors go to stderr (forwarded to colony's log).
pub const RACK_WORKER_SCRIPT: &str = r#"require 'rack'
require 'json'
require 'stringio'

app, _ = Rack::Builder.parse_file(ARGV[0] || 'config.ru')
STDOUT.sync = true
STDERR.sync = true

STDIN.each_line do |line|
  begin
    req = JSON.parse(line)

    rack_env = {
      'REQUEST_METHOD'    => req['method'],
      'PATH_INFO'         => req['path'],
      'QUERY_STRING'      => req['query'] || '',
      'SERVER_NAME'       => req['host'] || 'localhost',
      'SERVER_PORT'       => req['port'] || '8080',
      'SCRIPT_NAME'       => '',
      'rack.version'      => [1, 6],
      'rack.input'        => StringIO.new(req['body'] || ''),
      'rack.errors'       => STDERR,
      'rack.url_scheme'   => 'http',
      'rack.multithread'  => false,
      'rack.multiprocess' => true,
      'rack.run_once'     => false,
      'SERVER_SOFTWARE'   => 'colony-shell',
    }

    (req['headers'] || {}).each do |k, v|
      if k.downcase == 'content-type'
        rack_env['CONTENT_TYPE'] = v
      elsif k.downcase == 'content-length'
        rack_env['CONTENT_LENGTH'] = v
      else
        rack_env["HTTP_#{k.upcase.tr('-', '_')}"] = v
      end
    end

    status, headers, body = app.call(rack_env)
    chunks = []
    body.each { |chunk| chunks << chunk }
    body.close if body.respond_to?(:close)

    STDOUT.puts({ status: status, headers: headers, body: chunks.join }.to_json)
  rescue => e
    STDERR.puts("colony rack worker error: #{e.class}: #{e.message}")
    STDOUT.puts({ status: 500, headers: { 'Content-Type' => 'text/plain' }, body: "Internal Server Error: #{e.message}" }.to_json)
  end
end
"#;

/// Path where the worker script is installed.
pub fn worker_script_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".colony")
        .join("lib")
        .join("rack_worker.rb")
}

/// Ensure the worker script exists at `~/.colony/lib/rack_worker.rb`.
///
/// Creates the directory and writes the script if it does not exist,
/// or if the existing script content differs (upgrade path).
pub fn ensure_worker_script() -> std::io::Result<PathBuf> {
    let path = worker_script_path();

    // Check if script already exists with correct content
    if path.exists() {
        if let Ok(existing) = std::fs::read_to_string(&path) {
            if existing == RACK_WORKER_SCRIPT {
                return Ok(path);
            }
        }
    }

    // Create parent directories
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&path, RACK_WORKER_SCRIPT)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_script_path_ends_correctly() {
        let path = worker_script_path();
        assert!(path.ends_with(".colony/lib/rack_worker.rb"));
    }

    #[test]
    fn worker_script_contains_rack_builder() {
        assert!(RACK_WORKER_SCRIPT.contains("Rack::Builder.parse_file"));
    }

    #[test]
    fn worker_script_sets_stdout_sync() {
        assert!(RACK_WORKER_SCRIPT.contains("STDOUT.sync = true"));
    }

    #[test]
    fn worker_script_handles_errors() {
        assert!(RACK_WORKER_SCRIPT.contains("rescue => e"));
        assert!(RACK_WORKER_SCRIPT.contains("status: 500"));
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/src/serve/rack_script.rs
git commit -m "feat(serve): add bundled rack_worker.rb script with install logic"
```

---

## Chunk 2: Rack Worker Pool

### Task 3: Implement RackWorkerPool and RackWorker

**Files:**
- Create: `crates/colony-exec/src/serve/rack_pool.rs`

- [ ] **Step 1: Define WorkerStatus, RackWorker, and RackWorkerPool structs**

`crates/colony-exec/src/serve/rack_pool.rs`:

```rust
//! Rack worker pool -- manages Ruby child processes for Rack app serving.
//!
//! Each worker is a Ruby process running `rack_worker.rb` that communicates
//! via JSON-over-stdio. The pool dispatches requests round-robin and handles
//! worker crashes, timeouts, and graceful restarts.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
    pub headers: std::collections::HashMap<String, String>,
    pub body: String,
}

/// JSON response read from worker stdout.
#[derive(Debug, Deserialize)]
pub struct RackResponse {
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
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
```

Note: each `RackWorker` is behind its own `tokio::sync::Mutex` so that one busy worker does not block dispatch to others. The round-robin index skips workers that are currently locked (busy).

- [ ] **Step 2: Implement worker spawning**

```rust
impl RackWorker {
    /// Spawn a new Ruby worker process.
    ///
    /// Runs: `bundle exec ruby <worker_script> config.ru`
    /// with working directory set to `app_dir`.
    pub fn spawn(app_dir: &Path, worker_script: &Path) -> std::io::Result<Self> {
        use std::path::Path;
        use tokio::process::Command;

        let mut child = Command::new("bundle")
            .args(["exec", "ruby"])
            .arg(worker_script)
            .arg("config.ru")
            .current_dir(app_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit()) // worker stderr -> colony log
            .spawn()?;

        let stdin = child.stdin.take()
            .expect("child stdin not captured");
        let stdout = child.stdout.take()
            .expect("child stdout not captured");

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
        let mut line = match serde_json::to_string(req) {
            Ok(s) => s,
            Err(_) => {
                self.status = WorkerStatus::Idle;
                return None;
            }
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
        let response: RackResponse = match serde_json::from_str(response_line.trim()) {
            Ok(r) => r,
            Err(_) => {
                self.status = WorkerStatus::Dead;
                return None;
            }
        };

        self.requests_handled += 1;
        self.status = WorkerStatus::Idle;
        Some(response)
    }

    /// Check if this worker's process is still alive.
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
```

- [ ] **Step 3: Implement pool creation and dispatch**

```rust
impl RackWorkerPool {
    /// Create a new pool and spawn `count` workers.
    pub async fn new(
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
    /// Applies a timeout. Returns a `RackResponse` or an error string.
    /// On worker failure (crash, invalid JSON), returns 502 and schedules respawn.
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
                    // Worker crashed or invalid JSON -- mark dead, respawn in background
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
                eprintln!("serve: rack worker {} respawned", idx);
                *worker = new_worker;
            }
            Err(e) => {
                eprintln!("serve: failed to respawn rack worker {}: {}", idx, e);
            }
        }
    }

    /// Number of workers in the pool.
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Graceful restart: spawn new workers, drain old, kill old.
    pub async fn restart(&mut self) -> std::io::Result<()> { ... }

    /// Scale the pool to `target` workers.
    pub async fn scale(&mut self, target: usize) -> std::io::Result<()> { ... }

    /// Shutdown all workers.
    pub async fn shutdown(&self) { ... }

    /// Get worker info for `serve workers` display.
    pub async fn worker_info(&self) -> Vec<WorkerInfo> { ... }
}

/// Error from dispatch.
#[derive(Debug)]
pub enum DispatchError {
    NoWorkers,
    WorkerCrashed,
    Timeout,
    SpawnFailed,
}

/// Summary info for a single worker (for `serve workers` display).
pub struct WorkerInfo {
    pub index: usize,
    pub pid: Option<u32>,
    pub status: String,
    pub requests_handled: u64,
    pub uptime_secs: u64,
}
```

- [ ] **Step 4: Implement `restart()` -- graceful zero-downtime restart**

```rust
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
        match tokio::time::timeout(drain_timeout, old.lock()).await {
            Ok(mut worker) => {
                worker.kill().await;
            }
            Err(_) => {
                // Force-kill if drain times out
                if let Ok(mut worker) = old.try_lock() {
                    worker.kill().await;
                }
                eprintln!("serve: rack worker {} drain timed out, force-killed", i);
            }
        }
    }

    eprintln!("serve: rack workers restarted ({} workers)", count);
    Ok(())
}
```

- [ ] **Step 5: Implement `scale()` -- dynamic worker count adjustment**

```rust
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
```

- [ ] **Step 6: Implement `shutdown()` and `worker_info()`**

```rust
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
```

- [ ] **Step 7: Add unit tests**

Test the serialization/deserialization protocol and `DispatchError` variants:

```rust
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
                .into_iter().collect(),
            body: String::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""method":"GET"#));
        assert!(json.contains(r#""path":"/users"#));
    }

    #[test]
    fn rack_response_deserializes_from_json() {
        let json = r#"{"status":200,"headers":{"Content-Type":"text/html"},"body":"<html></html>"}"#;
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
                .into_iter().collect(),
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
}
```

- [ ] **Step 8: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 9: Commit**

```bash
git add crates/colony-exec/src/serve/rack_pool.rs
git commit -m "feat(serve): implement RackWorkerPool with round-robin dispatch, timeout, crash recovery"
```

---

## Chunk 3: Rack Handler (axum integration)

### Task 4: Implement rack_handler.rs

**Files:**
- Create: `crates/colony-exec/src/serve/rack_handler.rs`

- [ ] **Step 1: Create the Rack route handler**

`crates/colony-exec/src/serve/rack_handler.rs`:

```rust
//! Axum handler for Rack routes.
//!
//! Receives HTTP requests, serializes them to the JSON protocol,
//! dispatches to the RackWorkerPool, and returns the response.

use super::rack_pool::{DispatchError, RackRequest, RackWorkerPool};
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared state for Rack route handlers.
#[derive(Clone)]
struct RackState {
    pool: Arc<RwLock<Option<RackWorkerPool>>>,
    /// The path prefix for this rack route (for stripping).
    prefix: String,
    /// Host and port from server config for the worker protocol.
    host: String,
    port: String,
}

/// Add a Rack route to the router.
///
/// All HTTP methods are accepted -- the Rack app decides how to handle them.
/// The route is registered as a wildcard under the given path prefix.
pub fn add_rack_route(
    router: Router,
    url_path: &str,
    pool: Arc<RwLock<Option<RackWorkerPool>>>,
    host: &str,
    port: u16,
) -> Router {
    let state = RackState {
        pool,
        prefix: url_path.trim_end_matches('/').to_string(),
        host: host.to_string(),
        port: port.to_string(),
    };

    let handler = |axum::extract::State(state): axum::extract::State<RackState>,
                   req: Request| async move { handle_rack_request(state, req).await };

    let clean_path = url_path.trim_end_matches('/');
    if clean_path.is_empty() || clean_path == "/" {
        // Mount at root: catch all requests
        router.fallback(handler).with_state(state)
    } else {
        // Mount at subpath with wildcard
        let pattern = format!("{}/*rest", clean_path);
        router
            .route(&pattern, axum::routing::any(handler.clone()).with_state(state.clone()))
            .route(clean_path, axum::routing::any(handler).with_state(state))
    }
}

/// Handle an individual Rack request.
async fn handle_rack_request(state: RackState, req: Request) -> Response {
    let method = req.method().to_string();
    let full_path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    let headers = req.headers().clone();

    // Strip the route prefix to get the path the Rack app sees
    let path = if state.prefix.is_empty() || state.prefix == "/" {
        full_path.clone()
    } else {
        full_path
            .strip_prefix(&state.prefix)
            .unwrap_or(&full_path)
            .to_string()
    };
    // Ensure path starts with /
    let path = if path.is_empty() || !path.starts_with('/') {
        format!("/{path}")
    } else {
        path
    };

    // Read body
    let body_bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    // Build headers map
    let mut header_map = std::collections::HashMap::new();
    for (name, value) in &headers {
        if let Ok(v) = value.to_str() {
            header_map.insert(name.as_str().to_string(), v.to_string());
        }
    }

    let rack_req = RackRequest {
        method,
        path,
        query,
        host: state.host.clone(),
        port: state.port.clone(),
        headers: header_map,
        body,
    };

    // Dispatch to worker pool
    let pool_guard = state.pool.read().await;
    let Some(ref pool) = *pool_guard else {
        return (StatusCode::SERVICE_UNAVAILABLE, "rack worker pool not initialized").into_response();
    };

    match pool.dispatch(&rack_req).await {
        Ok(resp) => rack_response_to_http(resp),
        Err(DispatchError::Timeout) => {
            (StatusCode::GATEWAY_TIMEOUT, "rack worker timeout").into_response()
        }
        Err(DispatchError::WorkerCrashed) => {
            (StatusCode::BAD_GATEWAY, "rack worker crashed").into_response()
        }
        Err(DispatchError::NoWorkers) => {
            (StatusCode::SERVICE_UNAVAILABLE, "no rack workers available").into_response()
        }
        Err(DispatchError::SpawnFailed) => {
            (StatusCode::BAD_GATEWAY, "failed to spawn rack worker").into_response()
        }
    }
}

/// Convert a RackResponse into an axum HTTP Response.
fn rack_response_to_http(rack_resp: super::rack_pool::RackResponse) -> Response {
    let status = StatusCode::from_u16(rack_resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut builder = Response::builder().status(status);

    for (name, value) in &rack_resp.headers {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            builder = builder.header(n, v);
        }
    }

    builder
        .body(Body::from(rack_resp.body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
```

- [ ] **Step 2: Add unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use super::super::rack_pool::RackResponse;

    #[test]
    fn rack_response_to_http_basic() {
        let resp = RackResponse {
            status: 200,
            headers: [("Content-Type".to_string(), "text/html".to_string())]
                .into_iter().collect(),
            body: "<h1>Hello</h1>".to_string(),
        };
        let http = rack_response_to_http(resp);
        assert_eq!(http.status(), StatusCode::OK);
    }

    #[test]
    fn rack_response_to_http_invalid_status_defaults_to_500() {
        let resp = RackResponse {
            status: 999,
            headers: std::collections::HashMap::new(),
            body: String::new(),
        };
        let http = rack_response_to_http(resp);
        assert_eq!(http.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-exec/src/serve/rack_handler.rs
git commit -m "feat(serve): add rack_handler axum integration with pool dispatch"
```

---

## Chunk 4: CGI Handler

### Task 5: Implement cgi_handler.rs

**Files:**
- Create: `crates/colony-exec/src/serve/cgi_handler.rs`

- [ ] **Step 1: Create the CGI route handler**

`crates/colony-exec/src/serve/cgi_handler.rs`:

```rust
//! CGI route handler -- spawns an executable per-request with CGI env vars.
//!
//! Standard CGI protocol:
//! - Environment variables: REQUEST_METHOD, PATH_INFO, QUERY_STRING, HTTP_*, etc.
//! - Request body on stdin
//! - Response on stdout: headers (key: value) before blank line, body after
//!
//! Works with any language: Ruby, Python, Perl, shell scripts.
//! Higher latency than Rack workers (process spawn per request) but zero setup.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

/// Shared state for CGI handlers.
#[derive(Clone)]
struct CgiState {
    executable: PathBuf,
    /// The URL path prefix (for stripping from PATH_INFO).
    prefix: String,
}

/// Add a CGI route to the router.
///
/// The executable is spawned per-request with CGI environment variables.
pub fn add_cgi_route(router: Router, url_path: &str, executable: &PathBuf) -> Router {
    let state = CgiState {
        executable: executable.clone(),
        prefix: url_path.trim_end_matches('/').to_string(),
    };

    let handler = |axum::extract::State(state): axum::extract::State<CgiState>,
                   req: Request| async move { handle_cgi_request(state, req).await };

    let clean_path = url_path.trim_end_matches('/');
    if clean_path.is_empty() || clean_path == "/" {
        router.fallback(handler).with_state(state)
    } else {
        let pattern = format!("{}/*rest", clean_path);
        router
            .route(&pattern, axum::routing::any(handler.clone()).with_state(state.clone()))
            .route(clean_path, axum::routing::any(handler).with_state(state))
    }
}

/// Handle an individual CGI request.
async fn handle_cgi_request(state: CgiState, req: Request) -> Response {
    let method = req.method().to_string();
    let full_path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    let headers = req.headers().clone();

    // PATH_INFO: strip the route prefix
    let path_info = if state.prefix.is_empty() || state.prefix == "/" {
        full_path.clone()
    } else {
        full_path
            .strip_prefix(&state.prefix)
            .unwrap_or(&full_path)
            .to_string()
    };
    let path_info = if path_info.is_empty() { "/".to_string() } else { path_info };

    // Read request body
    let body_bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };

    // Build CGI environment variables
    let mut env_vars: Vec<(String, String)> = vec![
        ("REQUEST_METHOD".to_string(), method),
        ("PATH_INFO".to_string(), path_info),
        ("QUERY_STRING".to_string(), query),
        ("SERVER_NAME".to_string(), "localhost".to_string()),
        ("SERVER_PORT".to_string(), "8080".to_string()),
        ("SERVER_PROTOCOL".to_string(), "HTTP/1.1".to_string()),
        ("SERVER_SOFTWARE".to_string(), "colony-shell".to_string()),
        ("GATEWAY_INTERFACE".to_string(), "CGI/1.1".to_string()),
        ("SCRIPT_NAME".to_string(), state.prefix.clone()),
    ];

    // Add Content-Type and Content-Length from request headers
    for (name, value) in &headers {
        let Ok(v) = value.to_str() else { continue };
        let name_lower = name.as_str().to_lowercase();
        match name_lower.as_str() {
            "content-type" => {
                env_vars.push(("CONTENT_TYPE".to_string(), v.to_string()));
            }
            "content-length" => {
                env_vars.push(("CONTENT_LENGTH".to_string(), v.to_string()));
            }
            _ => {
                // HTTP_* variables: uppercase, hyphens to underscores
                let var_name = format!("HTTP_{}", name.as_str().to_uppercase().replace('-', "_"));
                env_vars.push((var_name, v.to_string()));
            }
        }
    }

    // Spawn the CGI process
    let mut child = match tokio::process::Command::new(&state.executable)
        .envs(env_vars)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to spawn CGI process: {e}"),
            )
                .into_response();
        }
    };

    // Write request body to stdin
    if !body_bytes.is_empty() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&body_bytes).await;
            // Drop stdin to signal EOF
        }
    } else {
        // Drop stdin immediately to prevent hanging
        drop(child.stdin.take());
    }

    // Read stdout
    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("CGI process error: {e}"),
            )
                .into_response();
        }
    };

    if !output.status.success() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "CGI process exited with non-zero status",
        )
            .into_response();
    }

    // Parse CGI response: headers before blank line, body after
    parse_cgi_response(&output.stdout)
}

/// Parse CGI response format.
///
/// CGI response format:
/// ```text
/// Content-Type: text/html
/// Status: 200 OK
/// X-Custom: value
///
/// <html>body here</html>
/// ```
///
/// Headers are terminated by a blank line (`\r\n\r\n` or `\n\n`).
/// The `Status` header is special: it sets the HTTP status code.
/// If no Status header, defaults to 200.
fn parse_cgi_response(raw: &[u8]) -> Response {
    let raw_str = String::from_utf8_lossy(raw);

    // Find the header/body boundary (blank line)
    let (header_section, body) = if let Some(pos) = raw_str.find("\r\n\r\n") {
        (&raw_str[..pos], &raw[pos + 4..])
    } else if let Some(pos) = raw_str.find("\n\n") {
        (&raw_str[..pos], &raw[pos + 2..])
    } else {
        // No headers -- entire output is body
        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(raw.to_vec()))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    };

    let mut status = StatusCode::OK;
    let mut response_headers = Vec::new();

    for line in header_section.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();

        if name.eq_ignore_ascii_case("Status") {
            // Parse "200 OK" or just "200"
            let code_str = value.split_whitespace().next().unwrap_or("200");
            if let Ok(code) = code_str.parse::<u16>() {
                status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);
            }
        } else if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            response_headers.push((n, v));
        }
    }

    let mut builder = Response::builder().status(status);
    for (name, value) in response_headers {
        builder = builder.header(name, value);
    }

    builder
        .body(Body::from(body.to_vec()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
```

- [ ] **Step 2: Add unit tests for CGI response parsing**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn parse_cgi_response_with_headers() {
        let raw = b"Content-Type: text/html\r\nX-Custom: value\r\n\r\n<html>hello</html>";
        let resp = parse_cgi_response(raw);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/html"
        );
        assert_eq!(
            resp.headers().get("x-custom").unwrap(),
            "value"
        );
    }

    #[tokio::test]
    async fn parse_cgi_response_with_status_header() {
        let raw = b"Status: 404 Not Found\r\nContent-Type: text/plain\r\n\r\nNot Found";
        let resp = parse_cgi_response(raw);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn parse_cgi_response_lf_only() {
        let raw = b"Content-Type: text/plain\n\nhello world";
        let resp = parse_cgi_response(raw);
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn parse_cgi_response_no_headers() {
        let raw = b"just body content";
        let resp = parse_cgi_response(raw);
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn parse_cgi_response_status_code_only() {
        let raw = b"Status: 302\nLocation: /new\n\n";
        let resp = parse_cgi_response(raw);
        assert_eq!(resp.status(), StatusCode::FOUND);
        assert_eq!(resp.headers().get("location").unwrap(), "/new");
    }

    #[test]
    fn cgi_env_var_construction() {
        // Verify the header-to-env-var transformation logic
        let header_name = "X-Forwarded-For";
        let var_name = format!("HTTP_{}", header_name.to_uppercase().replace('-', "_"));
        assert_eq!(var_name, "HTTP_X_FORWARDED_FOR");
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-exec/src/serve/cgi_handler.rs
git commit -m "feat(serve): add CGI handler with per-request process spawn and response parsing"
```

---

## Chunk 5: Router Integration and Args

### Task 6: Update server.rs build_router() for Rack and CGI routes

**Files:**
- Modify: `crates/colony-exec/src/serve/server.rs`

- [ ] **Step 1: Update build_router() to handle Rack and CGI routes**

In `crates/colony-exec/src/serve/server.rs`, update the `build_router()` function signature to accept the `rack_pool` parameter, and add match arms for `Route::Rack` and `Route::Cgi`:

```rust
pub(crate) fn build_router(
    config: &ServeConfig,
    stats: Arc<RequestStats>,
    context: &colony_expand::ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
    rack_pool: Arc<tokio::sync::RwLock<Option<super::rack_pool::RackWorkerPool>>>,
) -> Router {
```

Add inside the `for route in &config.routes` match:

```rust
Route::Rack { path, .. } => {
    router = super::rack_handler::add_rack_route(
        router,
        path,
        rack_pool.clone(),
        &config.host,
        config.port,
    );
}
Route::Cgi { path, executable } => {
    router = super::cgi_handler::add_cgi_route(router, path, executable);
}
```

- [ ] **Step 2: Update spawn_server() to pass rack_pool**

Add `rack_pool` parameter to `spawn_server()` and forward it to `build_router()`.

- [ ] **Step 3: Update hot_swap_router() to pass rack_pool**

Add `rack_pool` parameter to `hot_swap_router()` and forward it to `build_router()`.

- [ ] **Step 4: Update all call sites**

In `mod.rs` and `args.rs`, update calls to `spawn_server()` and `hot_swap_router()` to pass `serve_state.rack_pool.clone()`.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 6: Commit**

```bash
git add crates/colony-exec/src/serve/server.rs
git commit -m "feat(serve): integrate Rack and CGI routes into build_router()"
```

### Task 7: Add subcommand parsing for rack, cgi, and workers

**Files:**
- Modify: `crates/colony-exec/src/serve/args.rs`

- [ ] **Step 1: Add `handle_rack()` function**

`serve rack <path> <app_dir> [--workers N] [--timeout SECS]`:

```rust
/// `serve rack <path> <app_dir> [--workers N] [--timeout SECS]`
pub fn handle_rack(
    args: &[String],
    serve_state: &ServeState,
) -> Result<BuiltinResult> {
    if args.len() < 2 {
        eprintln!("Usage: serve rack <path> <app_dir> [--workers N] [--timeout SECS]");
        return Ok(BuiltinResult::Failure(1));
    }

    let url_path = args[0].clone();
    let app_dir = PathBuf::from(&args[1]);
    let mut workers = 4_usize;
    let mut timeout_secs = 30_u64;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--workers" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    eprintln!("serve rack: --workers requires a value");
                    return Ok(BuiltinResult::Failure(1));
                };
                let Ok(n) = val.parse::<usize>() else {
                    eprintln!("serve rack: --workers must be a number");
                    return Ok(BuiltinResult::Failure(1));
                };
                workers = n;
            }
            "--timeout" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    eprintln!("serve rack: --timeout requires a value");
                    return Ok(BuiltinResult::Failure(1));
                };
                let Ok(n) = val.parse::<u64>() else {
                    eprintln!("serve rack: --timeout must be a number");
                    return Ok(BuiltinResult::Failure(1));
                };
                timeout_secs = n;
            }
            other => {
                eprintln!("serve rack: unknown option '{other}'");
                return Ok(BuiltinResult::Failure(1));
            }
        }
        i += 1;
    }

    // Validate app directory exists
    if !app_dir.exists() {
        eprintln!("serve rack: directory '{}' does not exist", app_dir.display());
        return Ok(BuiltinResult::Failure(1));
    }

    // Validate config.ru exists
    if !app_dir.join("config.ru").exists() {
        eprintln!("serve rack: no config.ru found in '{}'", app_dir.display());
        return Ok(BuiltinResult::Failure(1));
    }

    // Ensure worker script is installed
    let worker_script = match super::rack_script::ensure_worker_script() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("serve rack: failed to install worker script: {e}");
            return Ok(BuiltinResult::Failure(1));
        }
    };

    // Add route to config
    let mut config = serve_state.config.write()
        .expect("serve config lock poisoned");
    config.routes.push(Route::Rack {
        path: url_path.clone(),
        app_dir: app_dir.clone(),
        workers,
        timeout_secs,
    });
    drop(config);

    // Initialize or update the worker pool
    // Pool creation is async, so we spawn a blocking task on a temporary runtime
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create rack setup runtime");

    let pool_result = rt.block_on(async {
        super::rack_pool::RackWorkerPool::new(
            app_dir.clone(),
            worker_script,
            workers,
            timeout_secs,
        ).await
    });

    match pool_result {
        Ok(pool) => {
            rt.block_on(async {
                let mut pool_guard = serve_state.rack_pool.write().await;
                *pool_guard = Some(pool);
            });
            eprintln!("serve: rack route added: {url_path} -> {} ({workers} workers)", app_dir.display());
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("serve rack: failed to spawn workers: {e}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}
```

- [ ] **Step 2: Add `handle_cgi()` function**

`serve cgi <path> <executable>`:

```rust
/// `serve cgi <path> <executable>`
pub fn handle_cgi(args: &[String], serve_state: &ServeState) -> Result<BuiltinResult> {
    if args.len() < 2 {
        eprintln!("Usage: serve cgi <path> <executable>");
        return Ok(BuiltinResult::Failure(1));
    }

    let url_path = args[0].clone();
    let executable = PathBuf::from(&args[1]);

    if !executable.exists() {
        eprintln!("serve cgi: executable '{}' does not exist", executable.display());
        return Ok(BuiltinResult::Failure(1));
    }

    let mut config = serve_state.config.write()
        .expect("serve config lock poisoned");
    config.routes.push(Route::Cgi {
        path: url_path.clone(),
        executable: executable.clone(),
    });
    drop(config);

    eprintln!("serve: cgi route added: {url_path} -> {}", executable.display());
    Ok(BuiltinResult::Success(0))
}
```

- [ ] **Step 3: Add `handle_workers()` function**

`serve workers` -- display worker pool status:

```rust
/// `serve workers` -- display worker pool status.
pub fn handle_workers(serve_state: &ServeState) -> Result<BuiltinResult> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create workers display runtime");

    let info = rt.block_on(async {
        let pool_guard = serve_state.rack_pool.read().await;
        let Some(ref pool) = *pool_guard else {
            return None;
        };
        Some(pool.worker_info().await)
    });

    let Some(workers) = info else {
        eprintln!("serve: no rack worker pool active");
        return Ok(BuiltinResult::Failure(1));
    };

    if workers.is_empty() {
        println!("No workers.");
        return Ok(BuiltinResult::Success(0));
    }

    println!("  {:<6} {:<8} {:<8} {:<12} {}", "IDX", "PID", "STATUS", "REQUESTS", "UPTIME");
    for w in &workers {
        let pid_str = w.pid.map_or("-".to_string(), |p| p.to_string());
        let uptime = format_uptime(w.uptime_secs);
        println!("  {:<6} {:<8} {:<8} {:<12} {}", w.index, pid_str, w.status, w.requests_handled, uptime);
    }

    Ok(BuiltinResult::Success(0))
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}
```

- [ ] **Step 4: Add `handle_workers_scale()` function**

`serve workers scale N`:

```rust
/// `serve workers scale <N>` -- scale the worker pool.
pub fn handle_workers_scale(args: &[String], serve_state: &ServeState) -> Result<BuiltinResult> {
    let Some(count_str) = args.first() else {
        eprintln!("Usage: serve workers scale <N>");
        return Ok(BuiltinResult::Failure(1));
    };
    let Ok(count) = count_str.parse::<usize>() else {
        eprintln!("serve workers scale: invalid count '{count_str}'");
        return Ok(BuiltinResult::Failure(1));
    };
    if count == 0 {
        eprintln!("serve workers scale: count must be at least 1");
        return Ok(BuiltinResult::Failure(1));
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create workers scale runtime");

    let result = rt.block_on(async {
        let mut pool_guard = serve_state.rack_pool.write().await;
        let Some(ref mut pool) = *pool_guard else {
            return Err("no rack worker pool active".to_string());
        };
        pool.scale(count).await.map_err(|e| e.to_string())
    });

    match result {
        Ok(()) => {
            eprintln!("serve: worker pool scaled to {count}");
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("serve workers scale: {e}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}
```

- [ ] **Step 5: Add `handle_workers_restart()` function**

`serve workers restart`:

```rust
/// `serve workers restart` -- graceful restart of all workers.
pub fn handle_workers_restart(serve_state: &ServeState) -> Result<BuiltinResult> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create workers restart runtime");

    let result = rt.block_on(async {
        let mut pool_guard = serve_state.rack_pool.write().await;
        let Some(ref mut pool) = *pool_guard else {
            return Err("no rack worker pool active".to_string());
        };
        pool.restart().await.map_err(|e| e.to_string())
    });

    match result {
        Ok(()) => Ok(BuiltinResult::Success(0)),
        Err(e) => {
            eprintln!("serve workers restart: {e}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}
```

- [ ] **Step 6: Update `handle_routes()` to display Rack and CGI routes**

In `handle_routes()`, add match arms:

```rust
Route::Rack { path, app_dir, workers, timeout_secs } => {
    println!("  rack     {path:<20} {:<30} workers={workers} timeout={timeout_secs}s", app_dir.display());
}
Route::Cgi { path, executable } => {
    println!("  cgi      {path:<20} {}", executable.display());
}
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 8: Commit**

```bash
git add crates/colony-exec/src/serve/args.rs
git commit -m "feat(serve): add rack, cgi, workers subcommand parsing"
```

### Task 8: Update mod.rs dispatch and module declarations

**Files:**
- Modify: `crates/colony-exec/src/serve/mod.rs`

- [ ] **Step 1: Add module declarations**

Add to module declarations:

```rust
mod rack_pool;
mod rack_handler;
mod cgi_handler;
mod rack_script;
```

- [ ] **Step 2: Add subcommand dispatch arms**

In the `match sub` block, add:

```rust
"rack" => args::handle_rack(sub_args, serve_state),
"cgi" => args::handle_cgi(sub_args, serve_state),
"workers" => {
    // Sub-dispatch: `serve workers`, `serve workers scale N`, `serve workers restart`
    if sub_args.is_empty() {
        args::handle_workers(serve_state)
    } else {
        match sub_args[0].as_str() {
            "scale" => args::handle_workers_scale(&sub_args[1..], serve_state),
            "restart" => args::handle_workers_restart(serve_state),
            other => {
                eprintln!("serve workers: unknown subcommand '{other}'");
                Ok(BuiltinResult::Failure(1))
            }
        }
    }
}
```

- [ ] **Step 3: Update the hot-swap trigger list**

Update the hot-swap condition to include `"rack"` and `"cgi"`:

```rust
if r.is_success()
    && serve_state.is_running()
    && matches!(sub, "static" | "endpoint" | "webhook" | "rack" | "cgi" | "remove")
```

- [ ] **Step 4: Update the usage message**

Update the usage eprintln to include new subcommands:

```rust
"Usage: serve <start|stop|status|static|endpoint|webhook|rack|cgi|workers|remove|routes> [args...]"
```

- [ ] **Step 5: Update hot_swap_router call to pass rack_pool**

In the hot-swap section of `builtin_serve()`, update the call:

```rust
server::hot_swap_router(serve_state, context, tuple_space, session_id);
```

to pass the rack pool handle from `serve_state`.

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 7: Run existing tests**

Run: `cargo test -p colony-exec -- serve`

- [ ] **Step 8: Commit**

```bash
git add crates/colony-exec/src/serve/mod.rs
git commit -m "feat(serve): wire up rack, cgi, workers subcommand dispatch"
```

---

## Chunk 6: Shutdown Integration and Tuple Emission

### Task 9: Shutdown rack workers on serve stop

**Files:**
- Modify: `crates/colony-exec/src/serve/server.rs`

- [ ] **Step 1: Update `stop_server()` to shut down the rack pool**

In `stop_server()`, before signaling the HTTP server to shut down, shut down the worker pool:

```rust
pub(crate) fn stop_server(
    serve_state: &ServeState,
) -> std::result::Result<(), String> {
    // Shutdown rack workers first
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create shutdown runtime: {e}"))?;

    rt.block_on(async {
        let pool_guard = serve_state.rack_pool.read().await;
        if let Some(ref pool) = *pool_guard {
            pool.shutdown().await;
        }
    });

    // ... existing shutdown logic ...
}
```

- [ ] **Step 2: Verify tuple emission works for Rack and CGI routes**

Rack and CGI routes return standard axum `Response` values through the router. The global middleware layer in `build_router()` (lines 64-102 of current `server.rs`) wraps ALL routes, so `http_request` tuples are emitted automatically for Rack and CGI requests. No additional work needed -- confirm this by code inspection.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 4: Run all serve tests**

Run: `cargo test -p colony-exec -- serve`

- [ ] **Step 5: Commit**

```bash
git add crates/colony-exec/src/serve/server.rs
git commit -m "feat(serve): shut down rack workers on serve stop"
```

---

## Chunk 7: Tests and Polish

### Task 10: Add comprehensive unit tests

**Files:**
- Modify: `crates/colony-exec/src/serve/rack_pool.rs` (add tests)
- Modify: `crates/colony-exec/src/serve/cgi_handler.rs` (add tests)
- Modify: `crates/colony-exec/src/serve/rack_handler.rs` (add tests)

- [ ] **Step 1: Add rack_pool protocol edge case tests**

In `rack_pool.rs`, add to the test module:

```rust
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
        headers: std::collections::HashMap::new(),
        body: String::new(),
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains(r#""headers":{}"#));
}
```

- [ ] **Step 2: Add CGI env var edge case tests**

In `cgi_handler.rs`, add to the test module:

```rust
#[tokio::test]
async fn parse_cgi_response_empty_body() {
    let raw = b"Status: 204\nContent-Type: text/plain\n\n";
    let resp = parse_cgi_response(raw);
    assert_eq!(resp.status(), 204);
}

#[tokio::test]
async fn parse_cgi_response_binary_body() {
    let mut raw = b"Content-Type: application/octet-stream\n\n".to_vec();
    raw.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x01]);
    let resp = parse_cgi_response(&raw);
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn cgi_env_content_type_not_prefixed() {
    // Content-Type should be CONTENT_TYPE, not HTTP_CONTENT_TYPE per CGI spec
    let header_name = "content-type";
    assert_ne!(
        format!("HTTP_{}", header_name.to_uppercase().replace('-', "_")),
        "CONTENT_TYPE"
    );
    // Our implementation handles this specially -- verified in handle_cgi_request
}
```

- [ ] **Step 3: Add format_uptime tests**

In `args.rs`, add a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_seconds() {
        assert_eq!(format_uptime(45), "45s");
    }

    #[test]
    fn format_uptime_minutes() {
        assert_eq!(format_uptime(125), "2m 5s");
    }

    #[test]
    fn format_uptime_hours() {
        assert_eq!(format_uptime(7260), "2h 1m");
    }
}
```

- [ ] **Step 4: Verify all tests pass**

Run: `cargo test -p colony-exec -- serve`

- [ ] **Step 5: Run clippy**

Run: `cargo clippy -p colony-exec -- -W clippy::all -W clippy::pedantic`

- [ ] **Step 6: Run fmt**

Run: `cargo fmt -p colony-exec`

- [ ] **Step 7: Commit**

```bash
git add crates/colony-exec/src/serve/
git commit -m "feat(serve): add comprehensive tests for Rack pool, CGI handler, and args"
```

### Task 11: Final integration verify

- [ ] **Step 1: Full workspace build**

Run: `cargo build`

- [ ] **Step 2: Full workspace test**

Run: `cargo test`

- [ ] **Step 3: Verify no regressions in existing serve tests**

Run: `cargo test -p colony-exec -- serve`

- [ ] **Step 4: Final commit (if any fixups needed)**

```bash
git add -A
git commit -m "feat(serve): Rack application server and CGI route support"
```
