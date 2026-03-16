# serve: Embedded Web Server Builtin

**Date:** 2026-03-16
**Status:** Draft
**Crate:** `colony-exec` (builtin) + `colony-serve-bin` (panel)
**Repo:** colony-shell

## Overview

A shell builtin that embeds a high-performance HTTP server (axum on tokio) inside the colony-shell process. Serves static files, exposes shell functions as HTTP endpoints, and receives webhooks — all composable via incremental subcommands. Endpoint handlers execute in parallel on a thread pool with cloned shell contexts, making this faster than anyone expects from a shell builtin.

## Design Decisions

1. **In-process with thread pool** — Server runs on a background tokio runtime with its own thread pool. Static file serving is pure async I/O (no shell involvement). Endpoint handlers each get a cloned `ExpansionContext` on their own thread — parallel request handling with subshell-style isolation. No variable mutation leaks between requests or back to the parent shell.

2. **Incremental route registration** — Routes added/removed dynamically via subcommands (`serve static`, `serve endpoint`, `serve webhook`). Most shell-native approach — composable from scripts, `.colony/init`, or interactively. Follows the `sidebar` builtin pattern.

3. **Functions as handlers** — Endpoint handlers reference shell functions by name. Request context injected as `$REQ_*` variables. Response is stdout (body) + fd 4 (metadata via `serve:status`/`serve:header` helpers). Functions are testable independently in the terminal.

4. **Tuple emission for observability** — Every request emits an `http_request` tuple. The serve monitor panel, mirror agent, and event workers can subscribe.

## Builtin Interface

### Lifecycle

```bash
serve start [port]                        # Start server (default: 8080)
serve start 3000 --threads 32             # Custom thread pool size (default: 16)
serve start 8080 --host 0.0.0.0           # Bind address (default: 127.0.0.1)
serve stop                                # Stop server
serve status                              # Running state, port, route count, request stats
```

### Route Registration

```bash
serve static <path> <dir>                 # Serve directory at URL path
serve static /app ./dist --index index.html
serve endpoint <path> <func>              # Route to shell function
serve endpoint /api/status handle_status --method GET
serve endpoint /api/submit handle_form --method POST
serve webhook <path> <func>               # Route with webhook helpers
serve webhook /hooks/github handle_push --secret $GITHUB_WEBHOOK_SECRET
serve remove <path>                       # Remove a route
serve routes                              # List all registered routes
```

### Request Context

Handlers receive request data as environment variables in their cloned context:

| Variable | Content |
|----------|---------|
| `$REQ_METHOD` | GET, POST, PUT, DELETE, etc. |
| `$REQ_PATH` | Request path (e.g., `/api/users/123`) |
| `$REQ_QUERY` | Raw query string |
| `$REQ_QUERY_*` | Individual query params (`$REQ_QUERY_page`) |
| `$REQ_BODY` | Request body (POST/PUT) |
| `$REQ_HEADER_*` | Headers, uppercased + underscored (`$REQ_HEADER_CONTENT_TYPE`) |
| `$REQ_REMOTE_ADDR` | Client IP address |

### Response Control

By default, stdout becomes the response body with status 200 and auto-detected Content-Type (JSON if valid JSON, text/plain otherwise). Non-zero exit code produces 500.

For full control, handlers use injected helper functions that write metadata to an in-memory channel (NOT fd 4, which is already `stdllm`):

```bash
handle_download() {
  serve:status 200
  serve:header "Content-Type" "application/pdf"
  serve:header "Content-Disposition" "attachment; filename=report.pdf"
  cat report.pdf
}
```

`serve:status <code>` — Set HTTP status code.
`serve:header <name> <value>` — Set a response header.

These helpers are pre-parsed shell functions injected into the handler's context at server start. They write structured metadata to an `mpsc::channel` shared between the handler thread and the response builder. Everything written to stdout is the response body.

**Note:** fd 3-6 are already assigned (stdcontext, stdllm, stdtool, stddiag). The serve metadata channel uses Rust-level `mpsc` instead of file descriptors to avoid collision.

### Tuple Emission

Every request emits:
```
("http_request", session_id, method, path, status_code, duration_ms)
```

## Architecture

### Server Runtime

`serve start` spawns a dedicated multi-threaded tokio runtime on a background OS thread. The runtime runs an axum router. The shell's main thread remains unblocked — normal shell interaction continues.

```
Shell Main Thread                    Server Thread (tokio runtime)
┌──────────────────┐                ┌──────────────────────────┐
│ REPL loop        │                │ axum router              │
│ serve start 8080 │──spawns───────▶│  ├─ static routes (async)│
│ serve endpoint...│──updates──────▶│  ├─ endpoint routes      │
│ serve stop       │──signals──────▶│  └─ webhook routes       │
│ (normal shell)   │                │                          │
└──────────────────┘                │ Thread pool (64 threads) │
                                    │  ├─ handler thread 1     │
                                    │  ├─ handler thread 2     │
                                    │  └─ ...                  │
                                    └──────────────────────────┘
```

### Route Types

**Static routes:** Handled entirely by tokio async I/O via `tower_http::ServeDir`. No shell execution. MIME type detection via file extension. Caching headers (ETag, Last-Modified). Index file support.

**Endpoint routes:** On each request:
1. Clone the shell's `ExpansionContext` (deep copy of variables, functions, aliases HashMaps — not Arc-shared, but acceptable for typical shell state sizes)
2. Create a lightweight per-request `Executor` with the cloned context, a no-op `JobTable`, a shared `TupleSpace` reference (for `http_request` tuple emission), and a dedicated `ChannelManager`
3. Spawn a thread from the pool
4. Inject `$REQ_*` variables into the cloned context
5. Inject `serve:status` and `serve:header` helper functions (pre-parsed `FunctionDef` ASTs stored at server start, injected into each cloned context's function table)
6. Execute the named function via `Executor::execute_function()`, capturing stdout
7. Parse response metadata from an in-memory channel (status code, headers written by helper functions)
8. Return HTTP response (stdout = body, metadata channel = headers/status, exit code = status fallback)

**Webhook routes:** Same as endpoint routes, plus:
- `$WEBHOOK_SIGNATURE` variable with the raw signature header
- `--secret` flag enables automatic HMAC-SHA256 verification (returns 401 on mismatch)
- Body always available as `$REQ_BODY` regardless of Content-Type

### Concurrency Model

- Static files: fully async, thousands of concurrent connections
- Endpoints: one OS thread per concurrent request, bounded by thread pool size (default 16)
- Context clone cost: deep copy of `ExpansionContext` HashMaps (variables, functions, aliases, environment). For a shell with typical state (~200 variables, ~50 functions), this is sub-millisecond. The `ExpansionContext` contains `Cell<>` fields making it `!Sync` but `Send` — each handler thread must own its clone exclusively. This is the same semantic as subshells.
- Per-request `Executor`: each handler creates a lightweight `Executor` wrapping the cloned context. The `Executor` provides `execute_function()` which is the actual shell function execution path. The per-request `Executor` gets a no-op `JobTable` (handlers don't manage jobs), a shared `TupleSpace` ref (for http_request emission), and a dedicated `ChannelManager`.
- Isolation: handler threads get a snapshot. Mutations don't propagate back. Same semantics as `$(subshell)`.

### Server State

The `ServeState` struct lives in the shell process. Route config is behind `RwLock`, stats are separate with atomics (no lock needed for hot-path increments):

```rust
struct ServeState {
    config: RwLock<ServeConfig>,
    stats: Arc<RequestStats>,  // separate — handler threads increment without lock
}

struct ServeConfig {
    routes: Vec<Route>,
    port: u16,
    host: String,
    thread_pool_size: usize,     // default: 16
    max_body_size: usize,         // default: 10MB
}

enum Route {
    Static { path: String, dir: PathBuf, index: Option<String> },
    Endpoint { path: String, func: String, method: Option<Method> },
    Webhook { path: String, func: String, secret: Option<String> },
}

struct RequestStats {
    total_requests: AtomicU64,
    total_errors: AtomicU64,
    total_duration_us: AtomicU64,
}
```

**Graceful shutdown:** `serve stop` stops accepting new connections, waits up to 5 seconds for in-flight requests to complete, then force-closes remaining connections.

**Function resolution:** Handler function names are resolved at request time (not registration time). If the function doesn't exist when a request arrives, the handler returns 500 with `"handler function not found: <name>"`.

Route changes (add/remove) rebuild the axum `Router` and hot-swap it via `arc_swap::ArcSwap<Router>`. The server's accept loop reads from the `ArcSwap` on each connection. No server restart needed, no lock contention on the hot path.

## Serve Monitor Panel

### `colony-serve-bin`

Panel binary that shows live request traffic and server stats. Same panel protocol as all other colony panel binaries.

**Binary name:** `serve-monitor` (crate: `colony-serve-bin`)

**Data pipeline:** The panel host process subscribes to `http_request` tuples in the tuple space and forwards them as JSON `InputEvent::Tuple` messages on the panel binary's stdin. The panel binary does not access the tuple space directly — it receives data through the standard panel protocol (same as all other panel binaries).

**Config subscription:** `["http_request"]`

**Two modes (Tab to toggle):**

**Traffic mode (default):**
```
● Running :8080  3 routes  247 req  0 err

GET  /api/status     200  12ms   14:32:05
POST /hooks/github   200  89ms   14:32:03
GET  /app/index.html 200   2ms   14:31:58
GET  /api/users      500  34ms   14:31:55
GET  /app/style.css  304   1ms   14:31:54

Reqs/min: 42  Avg: 18ms  Err: 0.4%
```

- Status line: running state, port, route count, total requests, error count
- Recent requests: method, path, status (red if 4xx/5xx), duration, timestamp
- Footer: requests/minute, average latency, error rate

**Routes mode:**
```
Routes:
 static   /app         ./dist
 endpoint /api/status  handle_status    142 req  12ms avg
 endpoint /api/users   get_users         89 req  34ms avg
 webhook  /hooks/gh    handle_push       16 req  89ms avg
```

- Route type, path, handler/directory
- Per-route stats: request count, average latency

**Key bindings:** Tab (toggle mode), j/k (scroll), c (clear history)

## Implementation Notes

### Dependencies

- `axum` — HTTP framework (composable routers, async)
- `tower-http` — ServeDir for static files, CORS, body size limit
- `hyper` — underlying HTTP server (comes with axum)
- `tokio` — already a workspace dependency
- `arc-swap` — lock-free hot router swapping
- `hmac` + `sha2` — webhook signature verification (sha2 already in workspace)

### Builtin Registration

Add `"serve"` to `is_builtin_base()` in `crates/colony-exec/src/builtins.rs`. Dispatch to `builtin_serve()` which handles subcommand parsing.

### Context Cloning

`ExpansionContext` already supports `clone()` for subshell execution. The same mechanism provides handler isolation. No new infrastructure needed.

### Hot Router Swap

When routes change, rebuild the axum `Router` and send it via a `tokio::sync::watch::Sender<Router>`. The server's accept loop reads from the watch receiver. Zero downtime route updates.

### Response Metadata Channel

`serve:status` and `serve:header` are pre-parsed shell functions (`FunctionDef` ASTs) constructed at server start and injected into each handler's cloned function table. They communicate metadata to the server thread via an `mpsc::Sender<ResponseMeta>` stored in the handler's environment as a serialized channel handle.

```rust
enum ResponseMeta {
    Status(u16),
    Header(String, String),  // name, value — no colon ambiguity
}
```

The server thread collects all `ResponseMeta` messages from the channel after the handler function completes, applies them to the HTTP response, then sends stdout as the body.

## Rack Application Server

Colony replaces Puma/Unicorn as the application server for Ruby/Rack apps (including Rails). No intermediary Ruby server — colony manages Ruby worker processes directly.

### Architecture

```
colony serve (Rust/axum)
  │
  ├─ HTTP request arrives at Rack route
  │
  ├─ Pick available worker from pool (round-robin)
  │
  ├─ Send request as JSON line to worker stdin
  │     {"method":"GET","path":"/users","query":"page=1",
  │      "headers":{"Accept":"text/html"},"body":""}
  │
  ├─ Worker (Ruby process) calls app.call(env)
  │
  ├─ Read response JSON line from worker stdout
  │     {"status":200,"headers":{"Content-Type":"text/html"},"body":"<html>..."}
  │
  └─ Return HTTP response to client
```

Same pattern as colony panel binaries: managed child processes communicating via JSON-over-stdio.

### Builtin Interface

```bash
# Serve a Rails app (auto-detects config.ru)
serve rack / ./my-rails-app --workers 4

# Rack app at a subpath
serve rack /api ./sinatra-app --workers 2

# Worker management
serve workers                  # show PIDs, requests handled, status, memory
serve workers scale 8          # increase pool size
serve workers restart          # graceful: finish in-flight, spawn new workers

# Mix with everything else
serve static /assets ./public
serve endpoint /api/deploy deploy_handler
serve webhook /hooks/github handle_push --secret $GH_SECRET
```

### Worker Pool

```rust
struct RackWorkerPool {
    workers: Vec<RackWorker>,
    app_dir: PathBuf,
    config_ru: PathBuf,
    next_worker: AtomicUsize,  // round-robin index
}

struct RackWorker {
    pid: u32,
    stdin: ChildStdin,           // send requests
    stdout: BufReader<ChildStdout>,  // read responses
    status: WorkerStatus,        // Idle, Busy, Dead
    requests_handled: u64,
    spawned_at: Instant,
}

enum WorkerStatus {
    Idle,
    Busy(Instant),  // start time of current request
    Dead,
}
```

### Worker Script

Colony bundles a small Ruby worker script (`~/.colony/lib/rack_worker.rb`) that loads the Rack app and enters a request loop:

```ruby
require 'rack'
require 'json'

app, _ = Rack::Builder.parse_file(ARGV[0] || 'config.ru')
STDOUT.sync = true

STDIN.each_line do |line|
  req = JSON.parse(line)

  rack_env = {
    'REQUEST_METHOD'  => req['method'],
    'PATH_INFO'       => req['path'],
    'QUERY_STRING'    => req['query'] || '',
    'SERVER_NAME'     => req['host'] || 'localhost',
    'SERVER_PORT'     => req['port'] || '8080',
    'SCRIPT_NAME'     => '',
    'rack.version'    => [1, 6],
    'rack.input'      => StringIO.new(req['body'] || ''),
    'rack.errors'     => STDERR,
    'rack.url_scheme'  => 'http',
    'rack.multithread' => false,
    'rack.multiprocess' => true,
    'rack.run_once'   => false,
    'SERVER_SOFTWARE'  => 'colony-shell',
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
  STDOUT.puts({ status: 500, headers: { 'Content-Type' => 'text/plain' }, body: "Internal Server Error: #{e.message}" }.to_json)
end
```

### Request/Response Protocol

**Request** (colony → worker, one JSON line on stdin):
```json
{
  "method": "POST",
  "path": "/users",
  "query": "format=json",
  "host": "localhost",
  "port": "8080",
  "headers": {
    "Content-Type": "application/json",
    "Accept": "application/json",
    "Authorization": "Bearer token123"
  },
  "body": "{\"name\":\"Alice\"}"
}
```

**Response** (worker → colony, one JSON line on stdout):
```json
{
  "status": 201,
  "headers": {
    "Content-Type": "application/json",
    "Location": "/users/42"
  },
  "body": "{\"id\":42,\"name\":\"Alice\"}"
}
```

One JSON line per request, one per response. Worker stderr goes to colony's log. If a worker's stdout produces invalid JSON or the worker crashes, colony returns 502 Bad Gateway and respawns the worker.

### Worker Lifecycle

**Spawning:**
```bash
cd <app_dir> && bundle exec ruby ~/.colony/lib/rack_worker.rb config.ru
```
Each worker is a child process managed by colony. Colony sets the working directory to the app root so Bundler and Rails find their gems.

**Health monitoring:**
- If a worker doesn't respond within 30 seconds (configurable), colony kills it and spawns a replacement
- If a worker crashes (process exits), colony logs the event, emits a tuple, and respawns
- `serve workers` shows per-worker stats: PID, requests handled, uptime, current status

**Graceful restart (`serve workers restart`):**
1. Spawn N new workers (they load the app fresh — picks up code changes)
2. Drain in-flight requests from old workers (wait up to 30s)
3. Kill old workers
4. Zero-downtime deployment — like Unicorn's USR2

**Scaling (`serve workers scale N`):**
- Increase: spawn additional workers immediately
- Decrease: drain and kill excess workers (newest first)

### What Colony Provides Over Puma

| Capability | Puma | Colony |
|-----------|------|--------|
| Rack serving | Yes | Yes |
| Static files | No (Rails handles) | Yes (tower-http, bypasses Ruby) |
| Shell endpoints alongside | No | Yes |
| Webhook handlers | No (needs Rails routes) | Yes (shell functions) |
| Request monitoring panel | No (external tools) | Built-in (serve-monitor panel) |
| Heartbeat health checks | No | Yes (HEARTBEAT.md) |
| Tuple emission per request | No | Yes (event workers react) |
| Worker management | CLI flags only | `serve workers` interactive |
| Mixed routing | No | Rack + static + endpoint + webhook on one port |
| Process supervision | Self-managed | Colony manages (crash restart, scaling) |

### CGI Route Type

For simpler cases (one-off scripts, non-Rack apps), colony also supports CGI-style execution:

```bash
# Execute any script as a CGI handler
serve cgi /report ./generate_report.rb
serve cgi /legacy ./cgi-bin/
```

CGI routes spawn the executable per-request with standard CGI environment variables (`REQUEST_METHOD`, `PATH_INFO`, `QUERY_STRING`, `HTTP_*`, `CONTENT_TYPE`, `CONTENT_LENGTH`). Request body on stdin, response on stdout. Headers parsed from stdout before the first blank line (standard CGI response format).

This works with any language — Ruby, Python, Perl, shell scripts. Higher latency than Rack workers (process spawn per request) but zero setup.

## Out of Scope (Future)

- TLS/HTTPS (use a reverse proxy for now)
- WebSocket support
- HTTP/2
- Streaming request bodies (multipart upload)
- Authentication middleware (handle in shell functions or Rails)
- Response compression (gzip/brotli)
- Config file loading (`serve load config.toml`)
- Virtual hosts
- Rate limiting per-route
- WSGI/ASGI support (Python equivalent of Rack)
- PHP-FPM protocol support
