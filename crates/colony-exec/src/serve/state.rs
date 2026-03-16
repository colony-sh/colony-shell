//! Core state types for the serve builtin.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use super::rack_pool::RackWorkerPool;

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
    Rack {
        path: String,
        app_dir: PathBuf,
        workers: usize,
        timeout_secs: u64,
    },
    Cgi {
        path: String,
        executable: PathBuf,
    },
}

impl Route {
    pub fn path(&self) -> &str {
        match self {
            Route::Static { path, .. }
            | Route::Endpoint { path, .. }
            | Route::Webhook { path, .. }
            | Route::Rack { path, .. }
            | Route::Cgi { path, .. } => path,
        }
    }

    pub fn kind(&self) -> &str {
        match self {
            Route::Static { .. } => "static",
            Route::Endpoint { .. } => "endpoint",
            Route::Webhook { .. } => "webhook",
            Route::Rack { .. } => "rack",
            Route::Cgi { .. } => "cgi",
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
        self.total_duration_us
            .fetch_add(duration_us, Ordering::Relaxed);
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

/// Handle to the background server runtime.
pub(crate) struct ServeRuntime {
    /// Shutdown signal sender.
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
    /// Join handle for the OS thread running the tokio runtime.
    pub thread_handle: Option<std::thread::JoinHandle<()>>,
    /// Router swap handle for hot-reloading routes.
    pub router_swap: Arc<arc_swap::ArcSwap<axum::Router>>,
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
    /// Rack worker pool (None if no rack routes registered).
    /// Uses `tokio::sync::RwLock` because worker dispatch is async.
    pub(crate) rack_pool: Arc<tokio::sync::RwLock<Option<RackWorkerPool>>>,
}

impl ServeState {
    pub fn new() -> Self {
        Self {
            config: RwLock::new(ServeConfig::default()),
            stats: Arc::new(RequestStats::default()),
            runtime_handle: RwLock::new(None),
            rack_pool: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.runtime_handle
            .read()
            .expect("serve runtime lock poisoned")
            .is_some()
    }
}

impl Default for ServeState {
    fn default() -> Self {
        Self::new()
    }
}

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
        assert_eq!(r.path(), "/api");
        assert_eq!(r.kind(), "endpoint");

        let r = Route::Webhook {
            path: "/hooks".to_string(),
            func: "on_push".to_string(),
            secret: Some("s3cret".to_string()),
        };
        assert_eq!(r.path(), "/hooks");
        assert_eq!(r.kind(), "webhook");
    }

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
}
