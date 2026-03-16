//! Webhook handler: endpoint handler + HMAC-SHA256 signature verification.
//!
//! Extends the endpoint handler with optional signature verification for
//! webhooks (e.g., GitHub `X-Hub-Signature-256`).

use super::endpoint_handler;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use colony_expand::{ExpansionContext, Variable};
use std::sync::Arc;

/// Shared state for webhook handlers.
///
/// `ExpansionContext` is `!Sync`, wrapped in `Mutex` for axum State.
#[derive(Clone)]
struct WebhookState {
    func_name: String,
    secret: Option<String>,
    context: Arc<parking_lot::Mutex<ExpansionContext>>,
}

/// Add a webhook route to the router.
///
/// Same as endpoint but with optional HMAC-SHA256 signature verification.
/// If `secret` is provided, the handler verifies the `X-Hub-Signature-256`
/// header (GitHub-style) against `HMAC-SHA256(secret, body)` and returns
/// 401 on mismatch.
#[allow(clippy::too_many_arguments)]
pub fn add_webhook_route(
    router: Router,
    url_path: &str,
    func_name: &str,
    secret: Option<&str>,
    context: ExpansionContext,
    _tuple_space: colony_tuple::TupleSpace,
    _session_id: String,
    _stats: Arc<super::state::RequestStats>,
) -> Router {
    let state = WebhookState {
        func_name: func_name.to_string(),
        secret: secret.map(ToString::to_string),
        context: Arc::new(parking_lot::Mutex::new(context)),
    };

    let handler = |axum::extract::State(state): axum::extract::State<WebhookState>,
                   req: Request| async move {
        handle_webhook_request(state, req).await
    };

    // Webhooks accept any method (typically POST, but we don't restrict)
    let route = axum::routing::any(handler);
    router.route(url_path, route.with_state(state))
}

/// Handle an individual webhook request.
///
/// 1. Read body
/// 2. If secret is configured, verify HMAC-SHA256 signature
/// 3. Inject `$WEBHOOK_SIGNATURE` variable
/// 4. Delegate to endpoint handler logic
async fn handle_webhook_request(state: WebhookState, req: Request) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    let headers = req.headers().clone();

    // Extract signature header before consuming request
    let signature_header = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // Read body
    let body_bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
    };

    // Verify signature if secret is configured
    if let Some(ref secret) = state.secret {
        if !verify_signature(secret, &body_bytes, &signature_header) {
            return (StatusCode::UNAUTHORIZED, "invalid webhook signature").into_response();
        }
    }

    let body = String::from_utf8_lossy(&body_bytes).to_string();

    // Clone context for isolation (lock held only for clone duration)
    let mut ctx = state.context.lock().clone();

    // Inject standard $REQ_* variables
    endpoint_handler::inject_request_vars(&mut ctx, &method, &path, &query, &headers, &body);

    // Inject webhook-specific variable
    ctx.set_variable(
        "WEBHOOK_SIGNATURE".to_string(),
        Variable::String(signature_header),
    );

    // Create mpsc channel for ResponseMeta
    let (meta_tx, mut meta_rx) =
        tokio::sync::mpsc::unbounded_channel::<super::state::ResponseMeta>();

    let func_name = state.func_name.clone();

    // Execute handler on a blocking thread
    let result = tokio::task::spawn_blocking(move || {
        endpoint_handler::set_response_meta_sender(meta_tx);
        let mut executor = crate::Executor::new(ctx);

        let Some(func_def) = executor.context().get_function(&func_name).cloned() else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("handler function not found: {func_name}"),
            );
        };

        // Capture stdout via pipe
        use std::os::fd::AsRawFd;

        let (pipe_read, pipe_write) = match nix::unistd::pipe() {
            Ok(fds) => fds,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to create pipe: {e}"),
                );
            }
        };

        let write_raw = pipe_write.as_raw_fd();

        let saved_stdout = match nix::unistd::dup(nix::libc::STDOUT_FILENO) {
            Ok(fd) => fd,
            Err(e) => {
                drop(pipe_read);
                drop(pipe_write);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to dup stdout: {e}"),
                );
            }
        };

        if nix::unistd::dup2(write_raw, nix::libc::STDOUT_FILENO).is_err() {
            drop(pipe_read);
            drop(pipe_write);
            let _ = nix::unistd::close(saved_stdout);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to redirect stdout".to_string(),
            );
        }
        drop(pipe_write);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create handler tokio runtime");

        let exit_status = rt.block_on(async { executor.execute_function(&func_def, vec![]).await });

        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = nix::unistd::dup2(saved_stdout, nix::libc::STDOUT_FILENO);
        let _ = nix::unistd::close(saved_stdout);

        let mut output = String::new();
        {
            use std::io::Read;
            let mut file = std::fs::File::from(pipe_read);
            let _ = file.read_to_string(&mut output);
        }

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
    })
    .await;

    let (mut status, handler_body) = match result {
        Ok(r) => r,
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("handler panicked: {e}"),
        ),
    };

    // Collect ResponseMeta from the channel
    let mut response_headers = axum::http::HeaderMap::new();

    while let Ok(meta) = meta_rx.try_recv() {
        match meta {
            super::state::ResponseMeta::Status(code) => {
                status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            }
            super::state::ResponseMeta::Header(name, value) => {
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

    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
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

    #[test]
    fn test_verify_invalid_hex() {
        assert!(!verify_signature("secret", b"body", "sha256=not_hex!!!"));
    }
}
