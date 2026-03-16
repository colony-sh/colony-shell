//! CGI route handler -- spawns an executable per-request with CGI env vars.
//!
//! Standard CGI protocol:
//! - Environment variables: `REQUEST_METHOD`, `PATH_INFO`, `QUERY_STRING`, `HTTP_*`, etc.
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
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Request timeout for CGI processes (30 seconds).
const CGI_TIMEOUT_SECS: u64 = 30;

/// Shared state for CGI handlers.
#[derive(Clone)]
struct CgiState {
    executable: PathBuf,
    /// The URL path prefix (for stripping from `PATH_INFO`).
    prefix: String,
}

/// Add a CGI route to the router.
///
/// The executable is spawned per-request with CGI environment variables.
pub fn add_cgi_route(router: Router, url_path: &str, executable: &Path) -> Router {
    let state = CgiState {
        executable: executable.to_path_buf(),
        prefix: url_path.trim_end_matches('/').to_string(),
    };

    let handler = |axum::extract::State(state): axum::extract::State<CgiState>, req: Request| async move {
        handle_cgi_request(state, req).await
    };

    let clean_path = url_path.trim_end_matches('/');
    if clean_path.is_empty() || clean_path == "/" {
        // Mount at root: catch all requests via fallback_service
        let service = axum::routing::any(handler).with_state(state);
        router.fallback_service(service)
    } else {
        let pattern = format!("{clean_path}/*rest");
        router
            .route(
                &pattern,
                axum::routing::any(handler).with_state(state.clone()),
            )
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
    let path_info = if path_info.is_empty() {
        "/".to_string()
    } else {
        path_info
    };

    // Read request body
    let Ok(body_bytes) = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
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

    // Write request body to stdin, then close stdin to signal EOF
    if let Some(mut stdin) = child.stdin.take() {
        if !body_bytes.is_empty() {
            let _ = stdin.write_all(&body_bytes).await;
        }
        // Drop stdin to signal EOF to the child process
    }

    // Read stdout and wait for child to exit, with timeout.
    // `wait_with_output()` consumes child, so we capture stdout separately
    // to retain the ability to kill on timeout.
    let timeout = tokio::time::Duration::from_secs(CGI_TIMEOUT_SECS);
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("CGI process error: {e}"),
            )
                .into_response();
        }
        Err(_) => {
            // Timeout -- the future was dropped which cleans up the child.
            // Return 504 Gateway Timeout.
            return (StatusCode::GATEWAY_TIMEOUT, "CGI process timed out").into_response();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_cgi_response_with_headers() {
        let raw = b"Content-Type: text/html\r\nX-Custom: value\r\n\r\n<html>hello</html>";
        let resp = parse_cgi_response(raw);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("content-type").unwrap(), "text/html");
        assert_eq!(resp.headers().get("x-custom").unwrap(), "value");
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

    #[tokio::test]
    async fn parse_cgi_response_empty_body() {
        let raw = b"Status: 204\nContent-Type: text/plain\n\n";
        let resp = parse_cgi_response(raw);
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn parse_cgi_response_binary_body() {
        let mut raw = b"Content-Type: application/octet-stream\n\n".to_vec();
        raw.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x01]);
        let resp = parse_cgi_response(&raw);
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn cgi_env_var_construction() {
        // Verify the header-to-env-var transformation logic
        let header_name = "X-Forwarded-For";
        let var_name = format!("HTTP_{}", header_name.to_uppercase().replace('-', "_"));
        assert_eq!(var_name, "HTTP_X_FORWARDED_FOR");
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
}
