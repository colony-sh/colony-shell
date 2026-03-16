# Serve Builtin Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a high-performance embedded web server builtin to colony-shell with static file serving, shell function endpoints, webhook support, and a live traffic monitor panel.

**Architecture:** The `serve` builtin embeds an axum HTTP server on a background tokio runtime with its own thread pool inside the colony-shell process. Static files are served via pure async I/O (`tower_http::ServeDir`). Endpoint and webhook handlers clone the shell's `ExpansionContext`, create a per-request `Executor`, inject `$REQ_*` variables and `serve:status`/`serve:header` helper functions, execute the named shell function, and return stdout as the response body with metadata from an in-memory `mpsc` channel. Routes are registered incrementally via subcommands and hot-swapped into the running server via `arc_swap::ArcSwap<Router>`.

**Tech Stack:** Rust, axum, tower-http, hyper, tokio, arc-swap, hmac/sha2, ratatui (panel)

**Spec:** `docs/superpowers/specs/2026-03-16-serve-builtin-design.md`

---

## File Structure

### New module: `crates/colony-exec/src/serve/`

| File | Responsibility |
|------|---------------|
| `mod.rs` | Module root, re-exports `builtin_serve`, `ServeState` |
| `state.rs` | `ServeState`, `ServeConfig`, `Route`, `RequestStats`, `ResponseMeta` structs |
| `server.rs` | Background tokio runtime spawn, axum router builder, `ArcSwap` hot-swap, graceful shutdown |
| `static_handler.rs` | `tower_http::ServeDir` integration for static file routes |
| `endpoint_handler.rs` | Clone `ExpansionContext`, create per-request `Executor`, inject `$REQ_*` vars, inject `serve:status`/`serve:header` functions, capture stdout, parse `ResponseMeta` from mpsc channel |
| `webhook_handler.rs` | Same as endpoint + HMAC-SHA256 signature verification |
| `args.rs` | Subcommand parsing for `serve start/stop/status/static/endpoint/webhook/remove/routes` |

### New crate: `crates/colony-serve-bin/`

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Crate manifest (binary: `serve-monitor`) |
| `src/main.rs` | Panel binary: traffic mode and routes mode, subscribes to `http_request` tuples |

### Modified files

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace root) | Add `colony-serve-bin` to `[workspace.members]`; add `axum`, `tower-http`, `arc-swap`, `hmac` to `[workspace.dependencies]` |
| `crates/colony-exec/Cargo.toml` | Add `axum`, `tower-http`, `arc-swap`, `hmac` dependencies |
| `crates/colony-exec/src/lib.rs` | Add `pub mod serve;` declaration |
| `crates/colony-exec/src/builtins.rs` | Add `"serve"` to `is_builtin_base()` match; add dispatch arm to `execute_builtin()` |

---

## Chunk 1: Dependencies and Module Skeleton

### Task 1: Add workspace dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/colony-exec/Cargo.toml`

- [ ] **Step 1: Add new workspace dependencies to root Cargo.toml**

In `Cargo.toml`, add to the `[workspace.dependencies]` section (after the existing entries around line 105):

```toml
# HTTP server
axum = "0.8"
tower-http = { version = "0.6", features = ["fs", "cors", "limit"] }
tower = "0.5"
arc-swap = "1.7"
hmac = "0.12"
```

Also add `"crates/colony-serve-bin"` to the `[workspace.members]` array (after line 35). This will be created in a later task but registered now.

**Note:** `sha2`, `tokio`, `serde`, and `serde_json` are already workspace dependencies. `hyper` comes transitively through `axum`.

- [ ] **Step 2: Add dependencies to colony-exec Cargo.toml**

In `crates/colony-exec/Cargo.toml`, add after the existing deps (after line 39):

```toml
# serve builtin
axum.workspace = true
tower-http.workspace = true
tower.workspace = true
arc-swap.workspace = true
hmac.workspace = true
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/colony-exec/Cargo.toml Cargo.lock
git commit -m "feat(serve): add axum, tower-http, arc-swap, hmac dependencies"
```

### Task 2: Create serve module skeleton

**Files:**
- Create: `crates/colony-exec/src/serve/mod.rs`
- Create: `crates/colony-exec/src/serve/state.rs`
- Create: `crates/colony-exec/src/serve/server.rs`
- Create: `crates/colony-exec/src/serve/static_handler.rs`
- Create: `crates/colony-exec/src/serve/endpoint_handler.rs`
- Create: `crates/colony-exec/src/serve/webhook_handler.rs`
- Create: `crates/colony-exec/src/serve/args.rs`
- Modify: `crates/colony-exec/src/lib.rs`

- [ ] **Step 1: Create module files with stub contents**

`crates/colony-exec/src/serve/mod.rs`:
```rust
//! `serve` — Embedded HTTP server builtin for colony-shell.
//!
//! Runs an axum server on a background tokio runtime. Supports static file
//! serving, shell function endpoints, and webhook routes with HMAC verification.

mod args;
mod endpoint_handler;
mod server;
mod state;
mod static_handler;
mod webhook_handler;

pub use state::ServeState;

use crate::builtins::BuiltinResult;
use crate::{ExecutionError, Result};
use colony_expand::ExpansionContext;
use colony_tuple::TupleSpace;

/// Entry point for the `serve` builtin. Dispatches subcommands.
pub fn builtin_serve(
    args: &[String],
    context: &ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
    serve_state: &ServeState,
) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("serve: missing subcommand");
        eprintln!("Usage: serve <start|stop|status|static|endpoint|webhook|remove|routes> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    match args[0].as_str() {
        "start" => args::handle_start(&args[1..], context, tuple_space, session_id, serve_state),
        "stop" => args::handle_stop(serve_state),
        "status" => args::handle_status(serve_state),
        "static" => args::handle_static(&args[1..], serve_state),
        "endpoint" => args::handle_endpoint(&args[1..], serve_state),
        "webhook" => args::handle_webhook(&args[1..], serve_state),
        "remove" => args::handle_remove(&args[1..], serve_state),
        "routes" => args::handle_routes(serve_state),
        other => {
            eprintln!("serve: unknown subcommand '{other}'");
            Ok(BuiltinResult::Failure(1))
        }
    }
}
```

Create all other files with placeholder module comments:
- `crates/colony-exec/src/serve/state.rs` — `// state types (Task 3)`
- `crates/colony-exec/src/serve/server.rs` — `// server runtime (Task 5)`
- `crates/colony-exec/src/serve/static_handler.rs` — `// static file handler (Task 6)`
- `crates/colony-exec/src/serve/endpoint_handler.rs` — `// endpoint handler (Task 7)`
- `crates/colony-exec/src/serve/webhook_handler.rs` — `// webhook handler (Task 8)`
- `crates/colony-exec/src/serve/args.rs` — `// subcommand parsing (Task 9)`

- [ ] **Step 2: Add module declaration to lib.rs**

In `crates/colony-exec/src/lib.rs`, add after `pub mod sidebar;` (around line 23):

```rust
pub mod serve;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-exec/src/serve/ crates/colony-exec/src/lib.rs
git commit -m "feat(serve): create serve module skeleton with subcommand dispatch"
```

---

## Chunk 2: State Types and Server Runtime

### Task 3: Implement ServeState, ServeConfig, Route, RequestStats

**Files:**
- Implement: `crates/colony-exec/src/serve/state.rs`

- [ ] **Step 1: Define types**

```rust
//! Core state types for the serve builtin.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// HTTP method filter for endpoint routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodFilter {
    Any,
    Specific(String), // "GET", "POST", etc.
}

/// A registered route.
#[derive(Debug, Clone)]
pub enum Route {
    Static {
        path: String,
        dir: PathBuf,
        index: Option<String>,
    },
    Endpoint {
        path: String,
        func: String,
        method: MethodFilter,
    },
    Webhook {
        path: String,
        func: String,
        secret: Option<String>,
    },
}

impl Route {
    pub fn path(&self) -> &str {
        match self {
            Route::Static { path, .. }
            | Route::Endpoint { path, .. }
            | Route::Webhook { path, .. } => path,
        }
    }

    pub fn kind(&self) -> &str {
        match self {
            Route::Static { .. } => "static",
            Route::Endpoint { .. } => "endpoint",
            Route::Webhook { .. } => "webhook",
        }
    }
}

/// Server configuration (behind RwLock for route updates).
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub routes: Vec<Route>,
    pub port: u16,
    pub host: String,
    pub thread_pool_size: usize,
    pub max_body_size: usize,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            port: 8080,
            host: "127.0.0.1".to_string(),
            thread_pool_size: 16,
            max_body_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Atomic request statistics (lock-free hot-path increments).
#[derive(Debug, Default)]
pub struct RequestStats {
    pub total_requests: AtomicU64,
    pub total_errors: AtomicU64,
    pub total_duration_us: AtomicU64,
}

impl RequestStats {
    pub fn record_request(&self, duration_us: u64, is_error: bool) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_duration_us.fetch_add(duration_us, Ordering::Relaxed);
        if is_error {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Response metadata sent from handler functions via mpsc channel.
#[derive(Debug, Clone)]
pub enum ResponseMeta {
    Status(u16),
    Header(String, String),
}

/// Top-level server state, stored in the Executor.
///
/// The `config` is behind `RwLock` for route mutation from the shell thread.
/// The `stats` use atomics so handler threads can increment without a lock.
pub struct ServeState {
    pub config: RwLock<ServeConfig>,
    pub stats: Arc<RequestStats>,
    /// Handle to the running server runtime (None if stopped).
    pub(crate) runtime_handle: RwLock<Option<ServeRuntime>>,
}

/// Handle to the background server runtime.
pub(crate) struct ServeRuntime {
    /// Shutdown signal sender.
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
    /// Join handle for the OS thread running the tokio runtime.
    pub thread_handle: Option<std::thread::JoinHandle<()>>,
    /// Router swap handle for hot-reloading routes.
    pub router_swap: Arc<arc_swap::ArcSwap<axum::Router>>,
}

impl ServeState {
    pub fn new() -> Self {
        Self {
            config: RwLock::new(ServeConfig::default()),
            stats: Arc::new(RequestStats::default()),
            runtime_handle: RwLock::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.runtime_handle
            .read()
            .expect("serve runtime lock poisoned")
            .is_some()
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/src/serve/state.rs
git commit -m "feat(serve): implement ServeState, ServeConfig, Route, RequestStats types"
```

### Task 4: Add ServeState to the Executor

**Files:**
- Modify: `crates/colony-exec/src/lib.rs`

- [ ] **Step 1: Add ServeState field to Executor struct**

In the `Executor` struct definition (around line 2262), add a new field:

```rust
/// Embedded HTTP server state (serve builtin)
serve_state: serve::ServeState,
```

- [ ] **Step 2: Initialize in constructors**

In `Executor::new()` (around line 2284) and `Executor::with_history()` (around line 2300), add:

```rust
serve_state: serve::ServeState::new(),
```

- [ ] **Step 3: Add accessor**

Add after the existing accessor methods (around line 2360):

```rust
/// Get reference to serve state (embedded HTTP server)
pub fn serve_state(&self) -> &serve::ServeState {
    &self.serve_state
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 5: Commit**

```bash
git add crates/colony-exec/src/lib.rs
git commit -m "feat(serve): add ServeState to Executor struct"
```

### Task 5: Implement server runtime (start/stop)

**Files:**
- Implement: `crates/colony-exec/src/serve/server.rs`

- [ ] **Step 1: Implement the background server**

```rust
//! Background tokio runtime and axum server management.

use super::state::{RequestStats, Route, ServeConfig, ServeRuntime, ServeState};
use arc_swap::ArcSwap;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;

/// Build an axum Router from the current route configuration.
///
/// Called on initial start and whenever routes change (hot-swap).
pub(crate) fn build_router(
    config: &ServeConfig,
    stats: Arc<RequestStats>,
    context: &colony_expand::ExpansionContext,
    tuple_space: &colony_tuple::TupleSpace,
    session_id: &str,
) -> Router {
    let mut router = Router::new();

    for route in &config.routes {
        match route {
            Route::Static { path, dir, index } => {
                router = super::static_handler::add_static_route(router, path, dir, index.as_deref());
            }
            Route::Endpoint { path, func, method } => {
                router = super::endpoint_handler::add_endpoint_route(
                    router,
                    path,
                    func,
                    method,
                    context.clone(),
                    tuple_space.clone(),
                    session_id.to_string(),
                    stats.clone(),
                );
            }
            Route::Webhook { path, func, secret } => {
                router = super::webhook_handler::add_webhook_route(
                    router,
                    path,
                    func,
                    secret.as_deref(),
                    context.clone(),
                    tuple_space.clone(),
                    session_id.to_string(),
                    stats.clone(),
                );
            }
        }
    }

    router
}

/// Spawn the background HTTP server on a dedicated OS thread with its own tokio runtime.
///
/// Returns the `ServeRuntime` handle for shutdown signaling and router hot-swap.
pub(crate) fn spawn_server(
    config: &ServeConfig,
    stats: Arc<RequestStats>,
    context: &colony_expand::ExpansionContext,
    tuple_space: &colony_tuple::TupleSpace,
    session_id: &str,
) -> Result<ServeRuntime, String> {
    let initial_router = build_router(config, stats.clone(), context, tuple_space, session_id);
    let router_swap = Arc::new(ArcSwap::new(Arc::new(initial_router)));
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e| format!("invalid bind address: {e}"))?;

    let thread_pool_size = config.thread_pool_size;
    let router_swap_clone = router_swap.clone();
    let mut shutdown_rx_clone = shutdown_rx.clone();

    let thread_handle = std::thread::Builder::new()
        .name("serve-runtime".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(thread_pool_size)
                .enable_all()
                .thread_name("serve-worker")
                .build()
                .expect("failed to build serve tokio runtime");

            rt.block_on(async move {
                let make_service = {
                    let router_swap = router_swap_clone;
                    move || {
                        let router = router_swap.load_full();
                        std::future::ready(Ok::<_, std::convert::Infallible>((*router).clone()))
                    }
                };

                // Use axum::serve with the router from ArcSwap
                let listener = match tokio::net::TcpListener::bind(addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!("serve: failed to bind {addr}: {e}");
                        return;
                    }
                };

                let router = router_swap_clone.load_full();
                let server = axum::serve(listener, (*router).clone());

                // Wait for shutdown signal
                let graceful = server.with_graceful_shutdown(async move {
                    let _ = shutdown_rx_clone.changed().await;
                });

                if let Err(e) = graceful.await {
                    eprintln!("serve: server error: {e}");
                }
            });
        })
        .map_err(|e| format!("failed to spawn serve thread: {e}"))?;

    Ok(ServeRuntime {
        shutdown_tx,
        thread_handle: Some(thread_handle),
        router_swap,
    })
}

/// Rebuild the router and hot-swap it into the running server.
///
/// Called after route changes (add/remove). No server restart needed.
pub(crate) fn hot_swap_router(
    serve_state: &ServeState,
    context: &colony_expand::ExpansionContext,
    tuple_space: &colony_tuple::TupleSpace,
    session_id: &str,
) {
    let config = serve_state.config.read().expect("serve config lock poisoned");
    let new_router = build_router(&config, serve_state.stats.clone(), context, tuple_space, session_id);

    let runtime = serve_state.runtime_handle.read().expect("serve runtime lock poisoned");
    if let Some(ref rt) = *runtime {
        rt.router_swap.store(Arc::new(new_router));
    }
}

/// Stop the server gracefully: signal shutdown, wait up to 5s for in-flight requests.
pub(crate) fn stop_server(serve_state: &ServeState) -> Result<(), String> {
    let mut runtime = serve_state
        .runtime_handle
        .write()
        .expect("serve runtime lock poisoned");

    let Some(rt) = runtime.take() else {
        return Err("serve: server is not running".to_string());
    };

    // Signal shutdown
    let _ = rt.shutdown_tx.send(true);

    // Wait up to 5 seconds for the thread to finish
    if let Some(handle) = rt.thread_handle {
        match handle.join() {
            Ok(()) => {}
            Err(_) => {
                eprintln!("serve: server thread panicked during shutdown");
            }
        }
    }

    Ok(())
}
```

**Note on `ArcSwap` hot-swap:** The spec mentions using `arc_swap` for lock-free router swapping. The `axum::serve` function takes ownership of the router, so the actual hot-swap implementation may need to use `tower::make::Shared` with a service that loads from the `ArcSwap` on each request, rather than restarting the listener. The implementer should verify the exact axum 0.8 API and adjust. An alternative approach is to use a `tower::Service` wrapper that delegates to the `ArcSwap`-loaded router. The key invariant is: route changes do NOT restart the server or drop connections.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/src/serve/server.rs
git commit -m "feat(serve): implement background server runtime with ArcSwap hot-swap"
```

---

## Chunk 3: Request Handlers

### Task 6: Implement static file handler

**Files:**
- Implement: `crates/colony-exec/src/serve/static_handler.rs`

- [ ] **Step 1: Implement tower-http ServeDir integration**

```rust
//! Static file serving via tower_http::ServeDir.

use axum::Router;
use std::path::Path;
use tower_http::services::ServeDir;

/// Add a static file route to the router.
///
/// Uses `tower_http::ServeDir` for async file serving with:
/// - MIME type detection via file extension
/// - ETag and Last-Modified caching headers (automatic)
/// - Optional index file (e.g., `index.html`)
pub fn add_static_route(
    router: Router,
    url_path: &str,
    dir: &Path,
    index: Option<&str>,
) -> Router {
    let mut service = ServeDir::new(dir);
    if let Some(idx) = index {
        // append_index_html_on_directories handles serving index files
        service = service.append_index_html_on_directories(true);
        // For a custom index file name, we need to use fallback
        // tower_http::ServeDir uses "index.html" by default when
        // append_index_html_on_directories is true. If the user wants
        // a different index file, handle via a fallback.
        let _ = idx; // default "index.html" behavior covers most cases
    }

    // Nest the service under the URL path
    let clean_path = url_path.trim_end_matches('/');
    router.nest_service(clean_path, service)
}
```

**Implementation note:** `tower_http::ServeDir` automatically handles ETag, Last-Modified, and conditional requests (304 Not Modified). MIME types are detected from file extensions. The implementer should verify the exact API for custom index file names in tower-http 0.6.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/src/serve/static_handler.rs
git commit -m "feat(serve): implement static file handler with ServeDir"
```

### Task 7: Implement endpoint handler

**Files:**
- Implement: `crates/colony-exec/src/serve/endpoint_handler.rs`

This is the most complex handler. It must:
1. Clone `ExpansionContext` for request isolation
2. Create a per-request `Executor`
3. Inject `$REQ_*` variables
4. Inject `serve:status` and `serve:header` shell functions
5. Execute the named function, capturing stdout
6. Collect `ResponseMeta` from mpsc channel
7. Build the HTTP response

- [ ] **Step 1: Implement the handler**

```rust
//! Endpoint handler: executes shell functions as HTTP request handlers.
//!
//! Each request gets a cloned ExpansionContext (subshell-style isolation),
//! a lightweight per-request Executor, and injected $REQ_* variables.

use super::state::{MethodFilter, RequestStats, ResponseMeta};
use crate::builtins::BuiltinResult;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, any};
use axum::Router;
use colony_expand::ExpansionContext;
use colony_tuple::TupleSpace;
use std::sync::Arc;
use std::time::Instant;

/// Shared state passed to each endpoint handler via axum State.
#[derive(Clone)]
struct EndpointState {
    func_name: String,
    context: ExpansionContext,
    tuple_space: TupleSpace,
    session_id: String,
    stats: Arc<RequestStats>,
}

/// Add an endpoint route to the router.
pub fn add_endpoint_route(
    router: Router,
    url_path: &str,
    func_name: &str,
    method: &MethodFilter,
    context: ExpansionContext,
    tuple_space: TupleSpace,
    session_id: String,
    stats: Arc<RequestStats>,
) -> Router {
    let state = EndpointState {
        func_name: func_name.to_string(),
        context,
        tuple_space,
        session_id,
        stats,
    };

    let handler = |
        axum::extract::State(state): axum::extract::State<EndpointState>,
        req: Request,
    | async move {
        handle_endpoint_request(state, req).await
    };

    let route = match method {
        MethodFilter::Any => axum::routing::any(handler),
        MethodFilter::Specific(m) => match m.to_uppercase().as_str() {
            "GET" => axum::routing::get(handler),
            "POST" => axum::routing::post(handler),
            "PUT" => axum::routing::put(handler),
            "DELETE" => axum::routing::delete(handler),
            "PATCH" => axum::routing::patch(handler),
            _ => axum::routing::any(handler),
        },
    };

    router.route(url_path, route.with_state(state))
}

/// Handle an individual endpoint request.
///
/// 1. Extract request data (method, path, query, headers, body)
/// 2. Clone ExpansionContext for isolation
/// 3. Inject $REQ_* variables
/// 4. Create mpsc channel for ResponseMeta
/// 5. Inject serve:status and serve:header helper functions
/// 6. Spawn blocking: create per-request Executor, execute function, capture stdout
/// 7. Collect ResponseMeta, build HTTP response
async fn handle_endpoint_request(
    state: EndpointState,
    req: Request,
) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    let headers = req.headers().clone();
    let remote_addr = String::new(); // extracted from ConnectInfo if configured

    // Read body
    let body_bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            state.stats.record_request(start.elapsed().as_micros() as u64, true);
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
    };
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    // Clone context for isolation (same semantics as subshells)
    let mut ctx = state.context.clone();

    // Inject $REQ_* variables
    inject_request_vars(&mut ctx, &method, &path, &query, &headers, &body, &remote_addr);

    // Create mpsc channel for ResponseMeta (serve:status, serve:header)
    let (meta_tx, mut meta_rx) = tokio::sync::mpsc::unbounded_channel::<ResponseMeta>();

    // Inject serve:status and serve:header helper functions into the context.
    // These are pre-parsed FunctionDef ASTs that, when called, write metadata
    // to the mpsc channel. The implementation stores the channel sender handle
    // in a thread-local or context-attached state that the helper functions read.
    //
    // Implementation detail: The helper functions are shell builtins (namespaced
    // as serve:status and serve:header) that write to a per-request mpsc sender.
    // The sender is stored in the context as a special variable or via a
    // thread-local, accessible to the builtin dispatch.

    let func_name = state.func_name.clone();
    let tuple_space = state.tuple_space.clone();
    let session_id = state.session_id.clone();

    // Execute handler on a blocking thread (shell execution is synchronous)
    let result = tokio::task::spawn_blocking(move || {
        // Create per-request Executor with:
        // - Cloned context (isolated)
        // - No-op JobTable (handlers don't manage jobs)
        // - Shared TupleSpace (for http_request tuple emission)
        // - Dedicated ChannelManager
        let mut executor = crate::Executor::new(ctx);

        // Look up the function
        let Some(func_def) = executor.context().get_function(&func_name).cloned() else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                HeaderMap::new(),
                format!("handler function not found: {func_name}"),
            );
        };

        // Execute function, capturing stdout
        // Use execute_and_capture or redirect stdout to a buffer
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create handler tokio runtime");

        let output = rt.block_on(async {
            // Redirect stdout capture
            executor.execute_function(&func_def, vec![]).await
        });

        match output {
            Ok(exit_status) => {
                if exit_status.code() != 0 {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        HeaderMap::new(),
                        format!("handler exited with code {}", exit_status.code()),
                    )
                } else {
                    // stdout was captured — return as body
                    // (stdout capture mechanism to be integrated with Executor)
                    (StatusCode::OK, HeaderMap::new(), String::new())
                }
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                HeaderMap::new(),
                format!("handler error: {e}"),
            ),
        }
    })
    .await;

    let (mut status, mut response_headers, body_str) = match result {
        Ok(r) => r,
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            HeaderMap::new(),
            format!("handler panicked: {e}"),
        ),
    };

    // Collect ResponseMeta from the channel
    while let Ok(meta) = meta_rx.try_recv() {
        match meta {
            ResponseMeta::Status(code) => {
                status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            }
            ResponseMeta::Header(name, value) => {
                if let (Ok(n), Ok(v)) = (
                    axum::http::header::HeaderName::from_bytes(name.as_bytes()),
                    HeaderValue::from_str(&value),
                ) {
                    response_headers.insert(n, v);
                }
            }
        }
    }

    // Auto-detect Content-Type if not set
    if !response_headers.contains_key("content-type") {
        let content_type = if serde_json::from_str::<serde_json::Value>(&body_str).is_ok() {
            "application/json"
        } else {
            "text/plain; charset=utf-8"
        };
        response_headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static(content_type),
        );
    }

    // Record stats
    let duration_us = start.elapsed().as_micros() as u64;
    let is_error = status.is_client_error() || status.is_server_error();
    state.stats.record_request(duration_us, is_error);

    // Emit http_request tuple
    let duration_ms = start.elapsed().as_millis() as u64;
    state.tuple_space.out(colony_tuple::Tuple::new(vec![
        colony_tuple::Value::Str("http_request".to_string()),
        colony_tuple::Value::Str(state.session_id.clone()),
        colony_tuple::Value::Str(method),
        colony_tuple::Value::Str(path),
        colony_tuple::Value::Int(status.as_u16() as i64),
        colony_tuple::Value::Int(duration_ms as i64),
    ]));

    // Build response
    let mut response = Response::builder().status(status);
    for (name, value) in &response_headers {
        response = response.header(name, value);
    }
    response.body(Body::from(body_str)).unwrap_or_else(|_| {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })
}

/// Inject $REQ_* environment variables into the handler's context.
fn inject_request_vars(
    ctx: &mut ExpansionContext,
    method: &str,
    path: &str,
    query: &str,
    headers: &HeaderMap,
    body: &str,
    remote_addr: &str,
) {
    use colony_expand::Variable;

    ctx.set_variable("REQ_METHOD", Variable::String(method.to_string()));
    ctx.set_variable("REQ_PATH", Variable::String(path.to_string()));
    ctx.set_variable("REQ_QUERY", Variable::String(query.to_string()));
    ctx.set_variable("REQ_BODY", Variable::String(body.to_string()));
    ctx.set_variable("REQ_REMOTE_ADDR", Variable::String(remote_addr.to_string()));

    // Parse individual query params: $REQ_QUERY_<name>
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            let var_name = format!("REQ_QUERY_{}", key.to_uppercase());
            ctx.set_variable(&var_name, Variable::String(value.to_string()));
        }
    }

    // Headers: $REQ_HEADER_<NAME> (uppercased, hyphens to underscores)
    for (name, value) in headers {
        let var_name = format!(
            "REQ_HEADER_{}",
            name.as_str().to_uppercase().replace('-', "_")
        );
        if let Ok(v) = value.to_str() {
            ctx.set_variable(&var_name, Variable::String(v.to_string()));
        }
    }
}
```

**Key implementation notes for the implementer:**

1. **stdout capture:** The per-request `Executor` needs to redirect stdout to an in-memory buffer. Look at `crate::execute_and_capture` for the existing pattern. The handler should use the same mechanism but adapted for function execution. Alternatively, use `os_pipe::pipe()` or redirect at the fd level.

2. **serve:status and serve:header functions:** These should be implemented as namespaced builtins (`serve:status` and `serve:header`) rather than injected `FunctionDef` ASTs. They are already handled by the `split_namespaced_builtin()` pattern in `builtins.rs`. The `mpsc::Sender<ResponseMeta>` should be stored in a thread-local or as an `Arc` in the `ExpansionContext`'s special variables, so the builtin dispatch can find it during execution.

3. **Async/sync bridge:** Shell function execution is synchronous (the Executor uses `.await` internally but runs on a blocking thread). The handler uses `spawn_blocking` to move execution off the async runtime, then creates a single-threaded tokio runtime for the `execute_function` call.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/src/serve/endpoint_handler.rs
git commit -m "feat(serve): implement endpoint handler with context cloning and REQ_* injection"
```

### Task 8: Implement webhook handler

**Files:**
- Implement: `crates/colony-exec/src/serve/webhook_handler.rs`

- [ ] **Step 1: Implement webhook handler with HMAC-SHA256 verification**

The webhook handler reuses the endpoint handler logic and adds:
- HMAC-SHA256 signature verification when `--secret` is provided
- `$WEBHOOK_SIGNATURE` variable injection
- Body always available as `$REQ_BODY` regardless of Content-Type

```rust
//! Webhook handler: endpoint handler + HMAC-SHA256 signature verification.

use super::endpoint_handler;
use super::state::RequestStats;
use axum::Router;
use colony_expand::ExpansionContext;
use colony_tuple::TupleSpace;
use std::sync::Arc;

/// Add a webhook route to the router.
///
/// Same as endpoint but with optional HMAC-SHA256 signature verification.
/// If `secret` is provided, the handler verifies the `X-Hub-Signature-256`
/// header (GitHub-style) against `HMAC-SHA256(secret, body)` and returns
/// 401 on mismatch.
pub fn add_webhook_route(
    router: Router,
    url_path: &str,
    func_name: &str,
    secret: Option<&str>,
    context: ExpansionContext,
    tuple_space: TupleSpace,
    session_id: String,
    stats: Arc<RequestStats>,
) -> Router {
    // Implementation delegates to endpoint handler with webhook-specific
    // middleware for signature verification.
    //
    // The handler:
    // 1. Reads the raw body
    // 2. If secret is set: computes HMAC-SHA256(secret, body), compares
    //    with X-Hub-Signature-256 header, returns 401 on mismatch
    // 3. Injects $WEBHOOK_SIGNATURE variable
    // 4. Delegates to endpoint handler logic

    todo!("implement webhook handler")
}

/// Verify HMAC-SHA256 signature.
///
/// Compares `sha256=<hex>` format (GitHub-style webhook signatures).
fn verify_signature(secret: &str, body: &[u8], signature_header: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let Some(hex_sig) = signature_header.strip_prefix("sha256=") else {
        return false;
    };

    let Ok(expected) = hex::decode(hex_sig) else {
        return false;
    };

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(body);

    mac.verify_slice(&expected).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_valid_signature() {
        let secret = "mysecret";
        let body = b"hello world";

        // Compute expected signature
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let result = mac.finalize();
        let hex_sig = hex::encode(result.into_bytes());

        let header = format!("sha256={hex_sig}");
        assert!(verify_signature(secret, body, &header));
    }

    #[test]
    fn test_verify_invalid_signature() {
        assert!(!verify_signature("secret", b"body", "sha256=0000"));
    }

    #[test]
    fn test_verify_missing_prefix() {
        assert!(!verify_signature("secret", b"body", "bad_format"));
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/src/serve/webhook_handler.rs
git commit -m "feat(serve): implement webhook handler with HMAC-SHA256 verification"
```

---

## Chunk 4: Builtin Registration and Subcommand Dispatch

### Task 9: Implement subcommand argument parsing

**Files:**
- Implement: `crates/colony-exec/src/serve/args.rs`

- [ ] **Step 1: Implement all subcommand handlers**

```rust
//! Subcommand argument parsing and dispatch for the serve builtin.
//!
//! All subcommands parse args manually (no clap — builtins parse args manually
//! following the flux/sidebar pattern).

use super::server;
use super::state::{MethodFilter, Route, ServeState};
use crate::builtins::BuiltinResult;
use crate::Result;
use colony_expand::ExpansionContext;
use colony_tuple::TupleSpace;
use std::path::PathBuf;

/// `serve start [port] [--threads N] [--host ADDR]`
pub fn handle_start(
    args: &[String],
    context: &ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
    serve_state: &ServeState,
) -> Result<BuiltinResult> {
    if serve_state.is_running() {
        eprintln!("serve: server is already running");
        return Ok(BuiltinResult::Failure(1));
    }

    let mut port: Option<u16> = None;
    let mut threads: Option<usize> = None;
    let mut host: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--threads" => {
                i += 1;
                threads = Some(
                    args.get(i)
                        .ok_or_else(|| crate::ExecutionError::BuiltinError(
                            "serve start: --threads requires a value".to_string(),
                        ))?
                        .parse()
                        .map_err(|_| crate::ExecutionError::BuiltinError(
                            "serve start: --threads must be a number".to_string(),
                        ))?,
                );
            }
            "--host" => {
                i += 1;
                host = Some(
                    args.get(i)
                        .ok_or_else(|| crate::ExecutionError::BuiltinError(
                            "serve start: --host requires a value".to_string(),
                        ))?
                        .clone(),
                );
            }
            arg if !arg.starts_with('-') && port.is_none() => {
                port = Some(arg.parse().map_err(|_| {
                    crate::ExecutionError::BuiltinError(format!(
                        "serve start: invalid port '{arg}'"
                    ))
                })?);
            }
            other => {
                eprintln!("serve start: unknown option '{other}'");
                return Ok(BuiltinResult::Failure(1));
            }
        }
        i += 1;
    }

    // Apply parsed options to config
    {
        let mut config = serve_state.config.write().expect("serve config lock poisoned");
        if let Some(p) = port {
            config.port = p;
        }
        if let Some(t) = threads {
            config.thread_pool_size = t;
        }
        if let Some(h) = host {
            config.host = h;
        }
    }

    // Spawn the server
    let config = serve_state.config.read().expect("serve config lock poisoned");
    match server::spawn_server(&config, serve_state.stats.clone(), context, tuple_space, session_id) {
        Ok(runtime) => {
            let port = config.port;
            let host = config.host.clone();
            drop(config);
            *serve_state.runtime_handle.write().expect("serve runtime lock poisoned") = Some(runtime);
            eprintln!("serve: listening on {host}:{port}");
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("serve: {e}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// `serve stop`
pub fn handle_stop(serve_state: &ServeState) -> Result<BuiltinResult> {
    match server::stop_server(serve_state) {
        Ok(()) => {
            eprintln!("serve: server stopped");
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("serve: {e}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// `serve status`
pub fn handle_status(serve_state: &ServeState) -> Result<BuiltinResult> {
    let config = serve_state.config.read().expect("serve config lock poisoned");
    let running = serve_state.is_running();
    let total = serve_state.stats.total_requests.load(std::sync::atomic::Ordering::Relaxed);
    let errors = serve_state.stats.total_errors.load(std::sync::atomic::Ordering::Relaxed);
    let duration_us = serve_state.stats.total_duration_us.load(std::sync::atomic::Ordering::Relaxed);

    let state_str = if running { "running" } else { "stopped" };
    let avg_ms = if total > 0 { duration_us / total / 1000 } else { 0 };

    println!("State:    {state_str}");
    println!("Address:  {}:{}", config.host, config.port);
    println!("Routes:   {}", config.routes.len());
    println!("Threads:  {}", config.thread_pool_size);
    println!("Requests: {total}");
    println!("Errors:   {errors}");
    println!("Avg:      {avg_ms}ms");

    Ok(BuiltinResult::Success(0))
}

/// `serve static <path> <dir> [--index FILE]`
pub fn handle_static(args: &[String], serve_state: &ServeState) -> Result<BuiltinResult> {
    if args.len() < 2 {
        eprintln!("Usage: serve static <path> <dir> [--index FILE]");
        return Ok(BuiltinResult::Failure(1));
    }

    let url_path = args[0].clone();
    let dir = PathBuf::from(&args[1]);
    let mut index = None;

    let mut i = 2;
    while i < args.len() {
        if args[i] == "--index" {
            i += 1;
            index = args.get(i).cloned();
        }
        i += 1;
    }

    if !dir.exists() {
        eprintln!("serve static: directory '{}' does not exist", dir.display());
        return Ok(BuiltinResult::Failure(1));
    }

    let mut config = serve_state.config.write().expect("serve config lock poisoned");
    config.routes.push(Route::Static {
        path: url_path.clone(),
        dir,
        index,
    });
    drop(config);

    eprintln!("serve: static route added: {url_path}");
    // Hot-swap will happen on next request cycle (caller should trigger if server is running)
    Ok(BuiltinResult::Success(0))
}

/// `serve endpoint <path> <func> [--method METHOD]`
pub fn handle_endpoint(args: &[String], serve_state: &ServeState) -> Result<BuiltinResult> {
    if args.len() < 2 {
        eprintln!("Usage: serve endpoint <path> <func> [--method METHOD]");
        return Ok(BuiltinResult::Failure(1));
    }

    let url_path = args[0].clone();
    let func = args[1].clone();
    let mut method = MethodFilter::Any;

    let mut i = 2;
    while i < args.len() {
        if args[i] == "--method" {
            i += 1;
            if let Some(m) = args.get(i) {
                method = MethodFilter::Specific(m.to_uppercase());
            }
        }
        i += 1;
    }

    let mut config = serve_state.config.write().expect("serve config lock poisoned");
    config.routes.push(Route::Endpoint {
        path: url_path.clone(),
        func: func.clone(),
        method,
    });
    drop(config);

    eprintln!("serve: endpoint added: {url_path} -> {func}()");
    Ok(BuiltinResult::Success(0))
}

/// `serve webhook <path> <func> [--secret SECRET]`
pub fn handle_webhook(args: &[String], serve_state: &ServeState) -> Result<BuiltinResult> {
    if args.len() < 2 {
        eprintln!("Usage: serve webhook <path> <func> [--secret SECRET]");
        return Ok(BuiltinResult::Failure(1));
    }

    let url_path = args[0].clone();
    let func = args[1].clone();
    let mut secret = None;

    let mut i = 2;
    while i < args.len() {
        if args[i] == "--secret" {
            i += 1;
            secret = args.get(i).cloned();
        }
        i += 1;
    }

    let mut config = serve_state.config.write().expect("serve config lock poisoned");
    config.routes.push(Route::Webhook {
        path: url_path.clone(),
        func: func.clone(),
        secret,
    });
    drop(config);

    eprintln!("serve: webhook added: {url_path} -> {func}()");
    Ok(BuiltinResult::Success(0))
}

/// `serve remove <path>`
pub fn handle_remove(args: &[String], serve_state: &ServeState) -> Result<BuiltinResult> {
    let Some(url_path) = args.first() else {
        eprintln!("Usage: serve remove <path>");
        return Ok(BuiltinResult::Failure(1));
    };

    let mut config = serve_state.config.write().expect("serve config lock poisoned");
    let before = config.routes.len();
    config.routes.retain(|r| r.path() != url_path);
    let removed = before - config.routes.len();
    drop(config);

    if removed > 0 {
        eprintln!("serve: removed {removed} route(s) at {url_path}");
        Ok(BuiltinResult::Success(0))
    } else {
        eprintln!("serve: no route found at '{url_path}'");
        Ok(BuiltinResult::Failure(1))
    }
}

/// `serve routes`
pub fn handle_routes(serve_state: &ServeState) -> Result<BuiltinResult> {
    let config = serve_state.config.read().expect("serve config lock poisoned");

    if config.routes.is_empty() {
        println!("No routes registered.");
        return Ok(BuiltinResult::Success(0));
    }

    for route in &config.routes {
        match route {
            Route::Static { path, dir, index } => {
                let idx = index.as_deref().unwrap_or("(none)");
                println!("  static   {path:<20} {:<30} index={idx}", dir.display());
            }
            Route::Endpoint { path, func, method } => {
                let m = match method {
                    MethodFilter::Any => "ANY",
                    MethodFilter::Specific(m) => m,
                };
                println!("  endpoint {path:<20} {func:<30} method={m}");
            }
            Route::Webhook { path, func, secret } => {
                let sig = if secret.is_some() { "yes" } else { "no" };
                println!("  webhook  {path:<20} {func:<30} secret={sig}");
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/src/serve/args.rs
git commit -m "feat(serve): implement subcommand argument parsing and dispatch"
```

### Task 10: Register serve builtin and wire up dispatch

**Files:**
- Modify: `crates/colony-exec/src/builtins.rs`
- Modify: `crates/colony-exec/src/lib.rs` (if needed for executor plumbing)

- [ ] **Step 1: Add "serve" to is_builtin_base()**

In `crates/colony-exec/src/builtins.rs`, in the `is_builtin_base()` function (around line 128), add `"serve"` to the match:

```rust
            | "sidebar"
            | "serve"
    )
```

- [ ] **Step 2: Add dispatch arm to execute_builtin()**

In the main `match name { ... }` dispatch block (around line 285), add after the `"sidebar"` arm:

```rust
        "serve" => crate::serve::builtin_serve(
            &args,
            context,
            tuple_space,
            session_id,
            &crate::serve::ServeState::new(), // placeholder — see step 3
        ),
```

**Important:** The above uses a temporary `ServeState::new()` which is wrong because it creates a new state on every call. The real state must live in the `Executor`. Since `execute_builtin` does not have access to the `Executor`, we need to either:

(a) Pass `ServeState` through `execute_builtin` — this requires adding a parameter to the function signature, or
(b) Store `ServeState` in a `thread_local!` or `static` `LazyLock`, or
(c) Store `ServeState` as an `Arc` in the `ExpansionContext`.

**Recommended approach:** Use `Arc<ServeState>` stored as a field on the `Executor`. Since `execute_builtin` is called from the `Executor`'s `execute_simple_command` method, modify the call site in `lib.rs` to pass `&self.serve_state` to `builtin_serve` directly, bypassing the generic `execute_builtin` function. Follow the same pattern as the `"sidebar"` arm which calls `crate::sidebar::parse_sidebar_args` and uses `tuple_space` directly.

Alternative: Add `serve_state: Option<&ServeState>` as a parameter to `execute_builtin()`.

- [ ] **Step 3: Wire up Executor -> serve builtin dispatch**

In `crates/colony-exec/src/lib.rs`, find where `execute_builtin` is called (in the `execute_simple_command` method, around line 2996). Add a special case before the generic builtin dispatch:

```rust
// Handle serve builtin with executor-owned state
if command_name == "serve" {
    return match crate::serve::builtin_serve(
        &args,
        &self.context,
        &self.tuple_space,
        &self.session_id,
        &self.serve_state,
    ) {
        Ok(result) => {
            // If server is running and routes changed, hot-swap
            if self.serve_state.is_running() && matches!(args.first().map(|s| s.as_str()), Some("static" | "endpoint" | "webhook" | "remove")) {
                crate::serve::server::hot_swap_router(
                    &self.serve_state,
                    &self.context,
                    &self.tuple_space,
                    &self.session_id,
                );
            }
            Ok(result.exit_code())
        }
        Err(e) => Err(e),
    };
}
```

This ensures the serve builtin has access to the `Executor`'s `ServeState` and can trigger hot-swap after route changes.

- [ ] **Step 4: Add serve:status and serve:header as namespaced builtins**

In `is_builtin_base()`, `"serve"` is already registered, and the namespaced pattern (`serve:status`, `serve:header`) is handled by `split_namespaced_builtin()`. Add handling in the `"serve"` dispatch arm or in the `serve` module:

The `serve:status` and `serve:header` builtins need access to the per-request `mpsc::Sender<ResponseMeta>`. Since these run inside a handler's cloned context on a spawned thread, use a `thread_local!` to store the sender:

```rust
// In crates/colony-exec/src/serve/endpoint_handler.rs
thread_local! {
    static RESPONSE_META_TX: std::cell::RefCell<Option<tokio::sync::mpsc::UnboundedSender<ResponseMeta>>> = const { std::cell::RefCell::new(None) };
}

/// Set the per-request response metadata sender (called before handler execution).
pub fn set_response_meta_sender(tx: tokio::sync::mpsc::UnboundedSender<ResponseMeta>) {
    RESPONSE_META_TX.with(|cell| {
        *cell.borrow_mut() = Some(tx);
    });
}

/// Send a response metadata message (called by serve:status and serve:header builtins).
pub fn send_response_meta(meta: ResponseMeta) -> bool {
    RESPONSE_META_TX.with(|cell| {
        if let Some(ref tx) = *cell.borrow() {
            tx.send(meta).is_ok()
        } else {
            false
        }
    })
}
```

Then in the serve builtin dispatch, handle `"status"` and `"header"` as namespaced subcommands that call `send_response_meta()`.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 6: Commit**

```bash
git add crates/colony-exec/src/builtins.rs crates/colony-exec/src/lib.rs crates/colony-exec/src/serve/
git commit -m "feat(serve): register serve builtin and wire up dispatch with Executor"
```

---

## Chunk 5: Tuple Emission and Hot-Swap

### Task 11: Implement http_request tuple emission

**Files:**
- Modify: `crates/colony-exec/src/serve/endpoint_handler.rs`
- Modify: `crates/colony-exec/src/serve/static_handler.rs`

- [ ] **Step 1: Ensure every request emits an http_request tuple**

The tuple format from the spec:
```
("http_request", session_id, method, path, status_code, duration_ms)
```

This is already shown in the endpoint handler code (Task 7). Ensure the static file handler also emits tuples by wrapping `ServeDir` in a middleware layer that records requests.

For static routes, add a `tower` middleware layer:

```rust
// In static_handler.rs, wrap ServeDir with a logging layer
use tower::ServiceBuilder;

// Add a response-tracking layer that emits tuples for static requests
```

Alternatively, add an axum middleware at the router level (applied to all routes) in `server.rs`:

```rust
// In build_router(), add a global middleware layer:
use axum::middleware;

let stats_clone = stats.clone();
let tuple_space_clone = tuple_space.clone();
let session_id_clone = session_id.to_string();

let router = router.layer(middleware::from_fn(move |req, next| {
    let stats = stats_clone.clone();
    let ts = tuple_space_clone.clone();
    let sid = session_id_clone.clone();
    async move {
        let method = req.method().to_string();
        let path = req.uri().path().to_string();
        let start = std::time::Instant::now();
        let response = next.run(req).await;
        let status = response.status().as_u16();
        let duration_ms = start.elapsed().as_millis() as u64;
        let duration_us = start.elapsed().as_micros() as u64;

        stats.record_request(duration_us, status >= 400);

        ts.out(colony_tuple::Tuple::new(vec![
            colony_tuple::Value::Str("http_request".to_string()),
            colony_tuple::Value::Str(sid),
            colony_tuple::Value::Str(method),
            colony_tuple::Value::Str(path),
            colony_tuple::Value::Int(status as i64),
            colony_tuple::Value::Int(duration_ms as i64),
        ]));

        response
    }
}));
```

This global middleware approach is cleaner — it avoids duplicating tuple emission in each handler and ensures static routes also emit tuples.

If using the global middleware, remove the per-handler tuple emission from `endpoint_handler.rs` to avoid double-counting.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/src/serve/
git commit -m "feat(serve): add global middleware for http_request tuple emission"
```

### Task 12: Implement router hot-swap on route changes

**Files:**
- Modify: `crates/colony-exec/src/serve/server.rs` (if not already done)
- Modify: `crates/colony-exec/src/serve/args.rs`

- [ ] **Step 1: Ensure route-changing subcommands trigger hot-swap**

After `handle_static`, `handle_endpoint`, `handle_webhook`, and `handle_remove` modify the config, they should trigger a router rebuild if the server is running.

The approach from Task 10 handles this in `lib.rs` at the call site. Alternatively, pass `context`, `tuple_space`, and `session_id` to these handlers so they can call `hot_swap_router` directly:

```rust
// In handle_static, after modifying config:
if serve_state.is_running() {
    server::hot_swap_router(serve_state, context, tuple_space, session_id);
}
```

This requires updating the function signatures to accept `context`, `tuple_space`, and `session_id`. Update `mod.rs` dispatch accordingly.

- [ ] **Step 2: Update serve module dispatch signatures**

Update `builtin_serve` to pass context/tuple_space/session_id to route-modifying handlers:

```rust
"static" => {
    let result = args::handle_static(&args[1..], serve_state)?;
    if result.is_success() && serve_state.is_running() {
        server::hot_swap_router(serve_state, context, tuple_space, session_id);
    }
    Ok(result)
}
```

Repeat for `endpoint`, `webhook`, and `remove`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-exec/src/serve/
git commit -m "feat(serve): trigger router hot-swap on route changes"
```

---

## Chunk 6: Panel Binary

### Task 13: Create colony-serve-bin crate

**Files:**
- Create: `crates/colony-serve-bin/Cargo.toml`
- Create: `crates/colony-serve-bin/src/main.rs`
- Modify: `Cargo.toml` (workspace root, already added in Task 1)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "colony-serve-bin"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "serve-monitor"
path = "src/main.rs"

[dependencies]
colony-panel = { path = "../colony-panel" }
serde_json = { workspace = true }
serde = { workspace = true }
```

- [ ] **Step 2: Implement main.rs**

Follow the exact panel protocol pattern from `colony-session-bin` and `colony-history-bin`. The panel:

1. Emits `PanelConfig` with `subscriptions: ["http_request"]`
2. Event loop reads `InputEvent` from stdin
3. On `InputEvent::Tuple` with key `"http_request"`: parse the tuple data (session_id, method, path, status, duration_ms), add to recent requests list
4. On `InputEvent::Key` with `Tab`: toggle between traffic and routes mode
5. On `InputEvent::Key` with `j`/`k`: scroll
6. On `InputEvent::Key` with `c`: clear history
7. Emit frames to stdout via `SENTINEL_FRAME` protocol

```rust
//! serve-monitor: panel binary for live HTTP traffic monitoring.
//!
//! Subscribes to `http_request` tuples and displays recent requests,
//! server stats, and route information.

use colony_panel::protocol::{
    FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME,
};
use serde::Deserialize;
use std::io::{self, BufRead, Write};
use std::time::Instant;

const MAX_RECENT_REQUESTS: usize = 100;

fn main() {
    let mut state = MonitorState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["http_request".to_string()],
            help: vec![
                "Tab: toggle traffic/routes".to_string(),
                "j/k: scroll".to_string(),
                "c: clear".to_string(),
            ],
        };
        writeln!(out, "{}", serde_json::to_string(&config).expect("config must serialize")).ok();
        out.flush().ok();
    }

    emit_frame(&stdout, &state);

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(event) = serde_json::from_str::<InputEvent>(&line) else {
            continue;
        };

        match event {
            InputEvent::Tuple { ref data } => {
                let Some(key) = data.get("key").and_then(|k| k.as_str()) else {
                    continue;
                };
                if key == "http_request" {
                    if let Some(value) = data.get("value") {
                        state.add_request(value);
                    }
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Key { ref key, .. } => {
                let changed = handle_key(&mut state, key);
                if changed {
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Resize { .. } => emit_frame(&stdout, &state),
            InputEvent::Shutdown => break,
            InputEvent::Focus { .. } => {}
        }
    }
}

fn handle_key(state: &mut MonitorState, key: &str) -> bool {
    match key {
        "Tab" => {
            state.toggle_mode();
            true
        }
        "j" | "Down" => state.scroll_down(),
        "k" | "Up" => state.scroll_up(),
        "c" => {
            state.clear();
            true
        }
        _ => false,
    }
}

fn emit_frame(stdout: &io::Stdout, state: &MonitorState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in state.render_lines() {
        writeln!(out, "{}", serde_json::to_string(&line).expect("frame line must serialize")).ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

// -----------------------------------------------------------------------
// State
// -----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Traffic,
    Routes,
}

#[derive(Debug, Clone)]
struct RequestEntry {
    method: String,
    path: String,
    status: u16,
    duration_ms: u64,
    timestamp: String,
}

struct MonitorState {
    mode: Mode,
    requests: Vec<RequestEntry>,
    total_requests: u64,
    total_errors: u64,
    cursor: usize,
    start: Instant,
}

impl MonitorState {
    fn new() -> Self {
        Self {
            mode: Mode::Traffic,
            requests: Vec::new(),
            total_requests: 0,
            total_errors: 0,
            cursor: 0,
            start: Instant::now(),
        }
    }

    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Traffic => Mode::Routes,
            Mode::Routes => Mode::Traffic,
        };
    }

    fn add_request(&mut self, value: &serde_json::Value) {
        // Parse tuple values from the JSON
        // The tuple format: (method, path, status_code, duration_ms)
        // Exact JSON shape depends on how TupleSpace serializes tuples
        // for the panel protocol.

        let method = value.get("method")
            .or_else(|| value.get(0))
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let path = value.get("path")
            .or_else(|| value.get(1))
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let status = value.get("status")
            .or_else(|| value.get(2))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u16;
        let duration_ms = value.get("duration_ms")
            .or_else(|| value.get(3))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let now = chrono::Local::now();
        let timestamp = now.format("%H:%M:%S").to_string();

        self.total_requests += 1;
        if status >= 400 {
            self.total_errors += 1;
        }

        self.requests.insert(0, RequestEntry {
            method,
            path,
            status,
            duration_ms,
            timestamp,
        });

        // Cap history
        if self.requests.len() > MAX_RECENT_REQUESTS {
            self.requests.truncate(MAX_RECENT_REQUESTS);
        }
    }

    fn scroll_down(&mut self) -> bool {
        if self.cursor + 1 < self.requests.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn scroll_up(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    fn clear(&mut self) {
        self.requests.clear();
        self.cursor = 0;
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        match self.mode {
            Mode::Traffic => self.render_traffic(),
            Mode::Routes => self.render_routes(),
        }
    }

    fn render_traffic(&self) -> Vec<FrameLine> {
        let mut lines = Vec::new();

        // Status line
        let err_rate = if self.total_requests > 0 {
            (self.total_errors as f64 / self.total_requests as f64) * 100.0
        } else {
            0.0
        };

        lines.push(FrameLine {
            text: format!(
                "{} req  {} err  {:.1}% err rate",
                self.total_requests, self.total_errors, err_rate
            ),
            icon: None,
            style: Some("bold,cyan".to_string()),
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        });

        lines.push(FrameLine {
            text: String::new(),
            icon: None,
            style: None,
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        });

        // Recent requests
        for (i, req) in self.requests.iter().enumerate() {
            let status_style = if req.status >= 500 {
                "red"
            } else if req.status >= 400 {
                "yellow"
            } else {
                "green"
            };

            let selected = i == self.cursor;
            lines.push(FrameLine {
                text: format!(
                    "{:<4} {:<30} {}  {:>4}ms  {}",
                    req.method, req.path, req.status, req.duration_ms, req.timestamp
                ),
                icon: None,
                style: Some(if selected {
                    format!("bold,{status_style}")
                } else {
                    status_style.to_string()
                }),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: Some(selected),
            });
        }

        lines
    }

    fn render_routes(&self) -> Vec<FrameLine> {
        // Routes mode shows a placeholder since the panel doesn't have
        // direct access to serve config. A future enhancement could have
        // the serve builtin emit route_added/route_removed tuples.
        vec![FrameLine {
            text: "Routes mode: subscribe to route tuples (future)".to_string(),
            icon: None,
            style: Some("dim".to_string()),
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        }]
    }
}
```

**Note:** Add `chrono = { workspace = true }` to the panel bin's `Cargo.toml` if timestamp formatting is needed.

- [ ] **Step 3: Add chrono dependency if needed**

```toml
# In crates/colony-serve-bin/Cargo.toml [dependencies]
chrono = { workspace = true }
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p colony-serve-bin`

- [ ] **Step 5: Commit**

```bash
git add crates/colony-serve-bin/
git commit -m "feat(serve): add serve-monitor panel binary with traffic and routes modes"
```

---

## Chunk 7: Graceful Shutdown and Testing

### Task 14: Implement graceful shutdown

**Files:**
- Modify: `crates/colony-exec/src/serve/server.rs`

- [ ] **Step 1: Verify shutdown behavior**

The `stop_server` function in Task 5 already signals shutdown and joins the thread. Ensure:

1. `serve stop` sends the shutdown signal via `watch::Sender`
2. `axum::serve::with_graceful_shutdown` stops accepting new connections
3. The thread join has a timeout (5 seconds, per spec)
4. If the thread doesn't finish in 5 seconds, log a warning and return

Update `stop_server` to use a timed join:

```rust
// Wait up to 5 seconds for graceful shutdown
if let Some(handle) = rt.thread_handle {
    let timeout = std::time::Duration::from_secs(5);
    // std::thread::JoinHandle doesn't have a timed join, so
    // spawn a helper thread to detect timeout
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let join_thread = std::thread::spawn(move || {
        let result = handle.join();
        let _ = done_tx.send(result);
    });

    match done_rx.recv_timeout(timeout) {
        Ok(Ok(())) => {}
        Ok(Err(_)) => eprintln!("serve: server thread panicked"),
        Err(_) => eprintln!("serve: shutdown timed out after 5s, forcing stop"),
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-exec/src/serve/server.rs
git commit -m "feat(serve): implement graceful shutdown with 5s timeout"
```

### Task 15: Add tests

**Files:**
- Modify: `crates/colony-exec/src/serve/state.rs` (unit tests)
- Modify: `crates/colony-exec/src/serve/webhook_handler.rs` (HMAC tests already in Task 8)
- Modify: `crates/colony-exec/src/serve/args.rs` (argument parsing tests)

- [ ] **Step 1: Add state type tests**

In `crates/colony-exec/src/serve/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_config_default_values() {
        let config = ServeConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.thread_pool_size, 16);
        assert_eq!(config.max_body_size, 10 * 1024 * 1024);
        assert!(config.routes.is_empty());
    }

    #[test]
    fn request_stats_record() {
        let stats = RequestStats::default();
        stats.record_request(1000, false);
        stats.record_request(2000, true);
        assert_eq!(stats.total_requests.load(Ordering::Relaxed), 2);
        assert_eq!(stats.total_errors.load(Ordering::Relaxed), 1);
        assert_eq!(stats.total_duration_us.load(Ordering::Relaxed), 3000);
    }

    #[test]
    fn serve_state_initially_not_running() {
        let state = ServeState::new();
        assert!(!state.is_running());
    }

    #[test]
    fn route_path_and_kind() {
        let r = Route::Static {
            path: "/app".to_string(),
            dir: PathBuf::from("./dist"),
            index: None,
        };
        assert_eq!(r.path(), "/app");
        assert_eq!(r.kind(), "static");

        let r = Route::Endpoint {
            path: "/api".to_string(),
            func: "handler".to_string(),
            method: MethodFilter::Any,
        };
        assert_eq!(r.kind(), "endpoint");
    }
}
```

- [ ] **Step 2: Add endpoint handler inject_request_vars test**

In `crates/colony-exec/src/serve/endpoint_handler.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_request_vars() {
        let mut ctx = ExpansionContext::new();
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert("X-Custom-Header", HeaderValue::from_static("value123"));

        inject_request_vars(
            &mut ctx,
            "POST",
            "/api/users",
            "page=1&limit=10",
            &headers,
            r#"{"name":"test"}"#,
            "127.0.0.1",
        );

        assert_eq!(ctx.get_variable("REQ_METHOD").unwrap().as_str(), "POST");
        assert_eq!(ctx.get_variable("REQ_PATH").unwrap().as_str(), "/api/users");
        assert_eq!(ctx.get_variable("REQ_QUERY").unwrap().as_str(), "page=1&limit=10");
        assert_eq!(ctx.get_variable("REQ_QUERY_PAGE").unwrap().as_str(), "1");
        assert_eq!(ctx.get_variable("REQ_QUERY_LIMIT").unwrap().as_str(), "10");
        assert_eq!(ctx.get_variable("REQ_HEADER_CONTENT_TYPE").unwrap().as_str(), "application/json");
        assert_eq!(ctx.get_variable("REQ_HEADER_X_CUSTOM_HEADER").unwrap().as_str(), "value123");
    }
}
```

**Note:** These tests depend on `ExpansionContext::new()` and `Variable` accessors. Adjust the accessor calls (`get_variable`, `as_str`) to match the actual API in `colony-expand`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p colony-exec -- serve`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-exec/src/serve/
git commit -m "test(serve): add unit tests for state types, HMAC verification, and request var injection"
```

---

## Chunk 8: Integration and Verification

### Task 16: Full build verification

- [ ] **Step 1: Run workspace check**

```bash
cargo check
```

- [ ] **Step 2: Run clippy on colony-exec**

```bash
cargo clippy -p colony-exec -- -W clippy::all -W clippy::pedantic
```

Fix any warnings following the RUST_STYLE_GUIDE.md conventions (no nested `if let`, use `let-else`, etc.).

- [ ] **Step 3: Run clippy on colony-serve-bin**

```bash
cargo clippy -p colony-serve-bin -- -W clippy::all -W clippy::pedantic
```

- [ ] **Step 4: Run all tests**

```bash
cargo test -p colony-exec
cargo test -p colony-serve-bin
```

- [ ] **Step 5: Commit any clippy fixes**

```bash
git add -A
git commit -m "chore(serve): fix clippy warnings"
```

### Task 17: Manual smoke test

- [ ] **Step 1: Build and run**

```bash
cargo build -p colony-exec
```

- [ ] **Step 2: Test in colony-shell (if runnable)**

```bash
# In a colony-shell session:
serve start 9090
serve static /app ./dist
serve endpoint /api/hello hello_handler --method GET
serve routes
serve status
# Open http://localhost:9090/api/hello in browser
serve stop
```

- [ ] **Step 3: Commit final state**

```bash
git add -A
git commit -m "feat(serve): complete serve builtin implementation"
```

---

## Summary of all commits

1. `feat(serve): add axum, tower-http, arc-swap, hmac dependencies`
2. `feat(serve): create serve module skeleton with subcommand dispatch`
3. `feat(serve): implement ServeState, ServeConfig, Route, RequestStats types`
4. `feat(serve): add ServeState to Executor struct`
5. `feat(serve): implement background server runtime with ArcSwap hot-swap`
6. `feat(serve): implement static file handler with ServeDir`
7. `feat(serve): implement endpoint handler with context cloning and REQ_* injection`
8. `feat(serve): implement webhook handler with HMAC-SHA256 verification`
9. `feat(serve): implement subcommand argument parsing and dispatch`
10. `feat(serve): register serve builtin and wire up dispatch with Executor`
11. `feat(serve): add global middleware for http_request tuple emission`
12. `feat(serve): trigger router hot-swap on route changes`
13. `feat(serve): add serve-monitor panel binary with traffic and routes modes`
14. `feat(serve): implement graceful shutdown with 5s timeout`
15. `test(serve): add unit tests for state types, HMAC verification, and request var injection`
16. `chore(serve): fix clippy warnings`
17. `feat(serve): complete serve builtin implementation`

---

## Key Implementation Decisions

1. **ServeState lives on Executor, not as a global.** This matches the pattern where session-scoped state is owned by the Executor. The serve builtin is dispatched specially from the Executor (bypassing the generic `execute_builtin` path) to access `self.serve_state`.

2. **Thread-local for ResponseMeta sender.** The `serve:status` and `serve:header` namespaced builtins communicate metadata back to the HTTP response builder via a thread-local `mpsc::Sender`. This avoids polluting the `ExpansionContext` with HTTP-specific state and is safe because each handler runs on its own thread.

3. **Global middleware for tuple emission.** Rather than duplicating tuple emission in each handler type, a single axum middleware layer records method, path, status, and duration for every request and emits the `http_request` tuple. This ensures static routes are also tracked.

4. **Router hot-swap via ArcSwap.** Route changes rebuild the full router and swap it atomically. The server never restarts. The exact integration with `axum::serve` may require a `tower::Service` wrapper that loads from `ArcSwap` on each request -- the implementer should verify the axum 0.8 API.

5. **No new crate for serve logic.** The serve module lives inside `colony-exec` (matching `flux`, `sidebar`, `mirror`, etc.). Only the panel binary is a separate crate (`colony-serve-bin`).
