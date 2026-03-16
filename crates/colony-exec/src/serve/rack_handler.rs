//! Axum handler for Rack routes.
//!
//! Receives HTTP requests, serializes them to the JSON protocol,
//! dispatches to the `RackWorkerPool`, and returns the response.

use super::rack_pool::{DispatchError, RackRequest, RackWorkerPool};
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared state for Rack route handlers.
#[derive(Clone)]
struct RackState {
    pool: Arc<RwLock<Option<RackWorkerPool>>>,
    /// The path prefix for this rack route (for stripping).
    prefix: String,
    /// Host from server config for the worker protocol.
    host: String,
    /// Port from server config for the worker protocol.
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

    let handler = |axum::extract::State(state): axum::extract::State<RackState>, req: Request| async move {
        handle_rack_request(state, req).await
    };

    let clean_path = url_path.trim_end_matches('/');
    if clean_path.is_empty() || clean_path == "/" {
        // Mount at root: catch all requests via fallback_service
        let service = axum::routing::any(handler).with_state(state);
        router.fallback_service(service)
    } else {
        // Mount at subpath with wildcard
        let pattern = format!("{clean_path}/*rest");
        router
            .route(
                &pattern,
                axum::routing::any(handler).with_state(state.clone()),
            )
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
    let Ok(body_bytes) = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    // Build headers map
    let mut header_map = HashMap::new();
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
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "rack worker pool not initialized",
        )
            .into_response();
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

/// Convert a `RackResponse` into an axum HTTP Response.
fn rack_response_to_http(rack_resp: super::rack_pool::RackResponse) -> Response {
    let status =
        StatusCode::from_u16(rack_resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
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

#[cfg(test)]
mod tests {
    use super::super::rack_pool::RackResponse;
    use super::*;

    #[test]
    fn rack_response_to_http_basic() {
        let resp = RackResponse {
            status: 200,
            headers: [("Content-Type".to_string(), "text/html".to_string())]
                .into_iter()
                .collect(),
            body: "<h1>Hello</h1>".to_string(),
        };
        let http = rack_response_to_http(resp);
        assert_eq!(http.status(), StatusCode::OK);
    }

    #[test]
    fn rack_response_to_http_nonstandard_status_passes_through() {
        // 999 is a valid 3-digit HTTP status code (100-999 range)
        let resp = RackResponse {
            status: 999,
            headers: HashMap::new(),
            body: String::new(),
        };
        let http = rack_response_to_http(resp);
        assert_eq!(http.status().as_u16(), 999);
    }

    #[test]
    fn rack_response_to_http_truly_invalid_status_defaults_to_500() {
        // Status codes outside 100-999 are invalid and should default to 500
        let resp = RackResponse {
            status: 0,
            headers: HashMap::new(),
            body: String::new(),
        };
        let http = rack_response_to_http(resp);
        assert_eq!(http.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn rack_response_to_http_preserves_headers() {
        let resp = RackResponse {
            status: 301,
            headers: [("Location".to_string(), "/new-path".to_string())]
                .into_iter()
                .collect(),
            body: String::new(),
        };
        let http = rack_response_to_http(resp);
        assert_eq!(http.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(http.headers().get("location").unwrap(), "/new-path");
    }
}
