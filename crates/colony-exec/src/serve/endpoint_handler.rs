//! Endpoint handler: executes shell functions as HTTP request handlers.
//!
//! Each request gets a cloned `ExpansionContext` (subshell-style isolation),
//! a lightweight per-request `Executor`, and injected `$REQ_*` variables.
//! stdout from the handler function becomes the HTTP response body.

use super::state::{MethodFilter, RequestStats, ResponseMeta};
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use colony_expand::{ExpansionContext, Variable};
use colony_tuple::TupleSpace;
use std::sync::Arc;

// Per-request response metadata sender, stored in a thread-local so that
// serve:status and serve:header builtins can send metadata without
// polluting the ExpansionContext with HTTP-specific state.
thread_local! {
    static RESPONSE_META_TX: std::cell::RefCell<Option<tokio::sync::mpsc::UnboundedSender<ResponseMeta>>> =
        const { std::cell::RefCell::new(None) };
}

/// Set the per-request response metadata sender (called before handler execution).
pub fn set_response_meta_sender(tx: tokio::sync::mpsc::UnboundedSender<ResponseMeta>) {
    RESPONSE_META_TX.with(|cell| {
        *cell.borrow_mut() = Some(tx);
    });
}

/// Clear the per-request response metadata sender (called after handler execution).
fn clear_response_meta_sender() {
    RESPONSE_META_TX.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Send a response metadata message (called by `serve:status` and `serve:header` builtins).
pub fn send_response_meta(meta: ResponseMeta) -> bool {
    RESPONSE_META_TX.with(|cell| {
        let borrow = cell.borrow();
        if let Some(ref tx) = *borrow {
            tx.send(meta).is_ok()
        } else {
            false
        }
    })
}

/// Shared state passed to each endpoint handler via axum State.
///
/// `ExpansionContext` is `!Sync` (contains `Cell` fields), so we wrap it in
/// a `Mutex` to satisfy axum's `State: Clone + Send + Sync` requirement.
/// The mutex is only held briefly to clone the context for each request.
#[derive(Clone)]
struct EndpointState {
    func_name: String,
    context: Arc<parking_lot::Mutex<ExpansionContext>>,
}

/// Add an endpoint route to the router.
///
/// Stats and tuple emission are handled by the global middleware layer
/// in `server::build_router`, so they are not passed to individual handlers.
#[allow(clippy::too_many_arguments)]
pub fn add_endpoint_route(
    router: Router,
    url_path: &str,
    func_name: &str,
    method: &MethodFilter,
    context: ExpansionContext,
    _tuple_space: TupleSpace,
    _session_id: String,
    _stats: Arc<RequestStats>,
) -> Router {
    let state = EndpointState {
        func_name: func_name.to_string(),
        context: Arc::new(parking_lot::Mutex::new(context)),
    };

    let handler = |axum::extract::State(state): axum::extract::State<EndpointState>,
                   req: Request| async move {
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
            "HEAD" => axum::routing::head(handler),
            "OPTIONS" => axum::routing::options(handler),
            _ => axum::routing::any(handler),
        },
    };

    router.route(url_path, route.with_state(state))
}

/// Handle an individual endpoint request.
///
/// 1. Extract request data (method, path, query, headers, body)
/// 2. Clone `ExpansionContext` for isolation
/// 3. Inject `$REQ_*` variables
/// 4. Create mpsc channel for `ResponseMeta`
/// 5. Spawn blocking: create per-request `Executor`, execute function, capture stdout
/// 6. Collect `ResponseMeta`, build HTTP response
async fn handle_endpoint_request(state: EndpointState, req: Request) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    let headers = req.headers().clone();

    // Read body
    let body_bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
    };
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    // Clone context for isolation (same semantics as subshells).
    // Lock is held only for the duration of the clone.
    let mut ctx = state.context.lock().clone();

    // Inject $REQ_* variables
    inject_request_vars(&mut ctx, &method, &path, &query, &headers, &body);

    // Create mpsc channel for ResponseMeta (serve:status, serve:header)
    let (meta_tx, mut meta_rx) = tokio::sync::mpsc::unbounded_channel::<ResponseMeta>();

    let func_name = state.func_name.clone();

    // Execute handler on a blocking thread (shell execution is synchronous)
    let result =
        tokio::task::spawn_blocking(move || execute_handler_function(ctx, &func_name, meta_tx))
            .await;

    let (handler_status, handler_body) = match result {
        Ok(r) => r,
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("handler panicked: {e}"),
        ),
    };

    // Collect ResponseMeta from the channel
    let mut status = handler_status;
    let mut response_headers = HeaderMap::new();

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
        let content_type = if serde_json::from_str::<serde_json::Value>(&handler_body).is_ok() {
            "application/json"
        } else {
            "text/plain; charset=utf-8"
        };
        if let Ok(v) = HeaderValue::from_str(content_type) {
            response_headers.insert(axum::http::header::CONTENT_TYPE, v);
        }
    }

    // Build response
    let mut response = Response::builder().status(status);
    for (name, value) in &response_headers {
        response = response.header(name, value);
    }
    response
        .body(Body::from(handler_body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Execute the shell handler function on a blocking thread.
///
/// Creates a per-request `Executor` with the cloned context, looks up the
/// function by name, executes it while capturing stdout, and returns the
/// HTTP status and body.
fn execute_handler_function(
    ctx: ExpansionContext,
    func_name: &str,
    meta_tx: tokio::sync::mpsc::UnboundedSender<ResponseMeta>,
) -> (StatusCode, String) {
    // Set up the thread-local response meta sender so serve:status/serve:header work
    set_response_meta_sender(meta_tx);

    let mut executor = crate::Executor::new(ctx);

    // Look up the function
    let Some(func_def) = executor.context().get_function(func_name).cloned() else {
        clear_response_meta_sender();
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("handler function not found: {func_name}"),
        );
    };

    // Capture stdout: redirect stdout to a pipe so we can collect the output
    use std::os::fd::AsRawFd;

    let (pipe_read, pipe_write) = match nix::unistd::pipe() {
        Ok(fds) => fds,
        Err(e) => {
            clear_response_meta_sender();
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to create pipe: {e}"),
            );
        }
    };

    let write_raw = pipe_write.as_raw_fd();

    // Save original stdout (dup returns RawFd)
    let saved_stdout = match nix::unistd::dup(nix::libc::STDOUT_FILENO) {
        Ok(fd) => fd,
        Err(e) => {
            drop(pipe_read);
            drop(pipe_write);
            clear_response_meta_sender();
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to dup stdout: {e}"),
            );
        }
    };

    // Redirect stdout to write end of pipe
    if nix::unistd::dup2(write_raw, nix::libc::STDOUT_FILENO).is_err() {
        drop(pipe_read);
        drop(pipe_write);
        let _ = nix::unistd::close(saved_stdout);
        clear_response_meta_sender();
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to redirect stdout".to_string(),
        );
    }
    // Close write end -- stdout now owns it via dup2
    drop(pipe_write);

    // Execute function on a temporary tokio runtime
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create handler tokio runtime");

    let exit_status = rt.block_on(async { executor.execute_function(&func_def, vec![]).await });

    // Flush stdout to ensure all output is in the pipe
    use std::io::Write;
    let _ = std::io::stdout().flush();

    // Restore original stdout and close the saved fd
    let _ = nix::unistd::dup2(saved_stdout, nix::libc::STDOUT_FILENO);
    let _ = nix::unistd::close(saved_stdout);

    // Read captured output from pipe
    let mut output = String::new();
    {
        use std::io::Read;
        let mut file = std::fs::File::from(pipe_read);
        let _ = file.read_to_string(&mut output);
    }

    clear_response_meta_sender();

    match exit_status {
        Ok(status) if status.code() != 0 => (
            StatusCode::INTERNAL_SERVER_ERROR,
            if output.is_empty() {
                format!("handler exited with code {}", status.code())
            } else {
                output
            },
        ),
        Ok(_) => (StatusCode::OK, output),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("handler error: {e}"),
        ),
    }
}

/// Inject `$REQ_*` environment variables into the handler's context.
pub(crate) fn inject_request_vars(
    ctx: &mut ExpansionContext,
    method: &str,
    path: &str,
    query: &str,
    headers: &HeaderMap,
    body: &str,
) {
    ctx.set_variable(
        "REQ_METHOD".to_string(),
        Variable::String(method.to_string()),
    );
    ctx.set_variable("REQ_PATH".to_string(), Variable::String(path.to_string()));
    ctx.set_variable("REQ_QUERY".to_string(), Variable::String(query.to_string()));
    ctx.set_variable("REQ_BODY".to_string(), Variable::String(body.to_string()));
    ctx.set_variable(
        "REQ_REMOTE_ADDR".to_string(),
        Variable::String(String::new()),
    );

    // Parse individual query params: $REQ_QUERY_<name>
    if !query.is_empty() {
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                let var_name = format!("REQ_QUERY_{}", key.to_uppercase());
                ctx.set_variable(var_name, Variable::String(value.to_string()));
            }
        }
    }

    // Headers: $REQ_HEADER_<NAME> (uppercased, hyphens to underscores)
    for (name, value) in headers {
        let var_name = format!(
            "REQ_HEADER_{}",
            name.as_str().to_uppercase().replace('-', "_")
        );
        if let Ok(v) = value.to_str() {
            ctx.set_variable(var_name, Variable::String(v.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_request_vars_basic() {
        let mut ctx = ExpansionContext::new();
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert("x-custom-header", HeaderValue::from_static("value123"));

        inject_request_vars(
            &mut ctx,
            "POST",
            "/api/users",
            "page=1&limit=10",
            &headers,
            r#"{"name":"test"}"#,
        );

        assert_eq!(
            ctx.get_variable("REQ_METHOD").map(Variable::as_str),
            Some("POST")
        );
        assert_eq!(
            ctx.get_variable("REQ_PATH").map(Variable::as_str),
            Some("/api/users")
        );
        assert_eq!(
            ctx.get_variable("REQ_QUERY").map(Variable::as_str),
            Some("page=1&limit=10")
        );
        assert_eq!(
            ctx.get_variable("REQ_QUERY_PAGE").map(Variable::as_str),
            Some("1")
        );
        assert_eq!(
            ctx.get_variable("REQ_QUERY_LIMIT").map(Variable::as_str),
            Some("10")
        );
        assert_eq!(
            ctx.get_variable("REQ_HEADER_CONTENT_TYPE")
                .map(Variable::as_str),
            Some("application/json")
        );
        assert_eq!(
            ctx.get_variable("REQ_HEADER_X_CUSTOM_HEADER")
                .map(Variable::as_str),
            Some("value123")
        );
    }

    #[test]
    fn test_inject_request_vars_empty_query() {
        let mut ctx = ExpansionContext::new();
        let headers = HeaderMap::new();
        inject_request_vars(&mut ctx, "GET", "/", "", &headers, "");

        assert_eq!(
            ctx.get_variable("REQ_METHOD").map(Variable::as_str),
            Some("GET")
        );
        assert_eq!(
            ctx.get_variable("REQ_QUERY").map(Variable::as_str),
            Some("")
        );
        // No REQ_QUERY_* variables should be set
        assert!(ctx.get_variable("REQ_QUERY_").is_none());
    }

    #[test]
    fn test_response_meta_thread_local() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ResponseMeta>();
        set_response_meta_sender(tx);

        assert!(send_response_meta(ResponseMeta::Status(201)));
        assert!(send_response_meta(ResponseMeta::Header(
            "X-Custom".to_string(),
            "value".to_string()
        )));

        clear_response_meta_sender();
        assert!(!send_response_meta(ResponseMeta::Status(500)));

        let meta1 = rx.try_recv().unwrap();
        assert!(matches!(meta1, ResponseMeta::Status(201)));

        let meta2 = rx.try_recv().unwrap();
        assert!(
            matches!(meta2, ResponseMeta::Header(ref n, ref v) if n == "X-Custom" && v == "value")
        );
    }
}
