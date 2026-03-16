//! Static file serving via `tower_http::ServeDir`.
//!
//! Serves files from a local directory at a given URL path prefix.
//! MIME types are auto-detected from file extensions. ETag and Last-Modified
//! caching headers are handled automatically by tower-http.

use axum::Router;
use std::path::Path;
use tower_http::services::ServeDir;

/// Add a static file route to the router.
///
/// Uses `tower_http::ServeDir` for async file serving with:
/// - MIME type detection via file extension
/// - ETag and Last-Modified caching headers (automatic)
/// - Index file support (`index.html` by default when directory is requested)
pub fn add_static_route(router: Router, url_path: &str, dir: &Path) -> Router {
    let service = ServeDir::new(dir).append_index_html_on_directories(true);

    // Nest the service under the URL path
    let clean_path = url_path.trim_end_matches('/');
    if clean_path.is_empty() {
        router.fallback_service(service)
    } else {
        router.nest_service(clean_path, service)
    }
}
