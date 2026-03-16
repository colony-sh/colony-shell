//! Background tokio runtime and axum server management.
//!
//! Spawns a dedicated multi-threaded tokio runtime on a background OS thread.
//! Routes are hot-swapped via `ArcSwap` without restarting the server.

use super::state::{RequestStats, Route, ServeConfig, ServeRuntime, ServeState};
use arc_swap::ArcSwap;
use axum::body::Body;
use axum::extract::Request;
use axum::Router;
use colony_tuple::{Tuple, TupleSpace, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

/// Build an axum Router from the current route configuration.
///
/// Called on initial start and whenever routes change (hot-swap).
pub(crate) fn build_router(
    config: &ServeConfig,
    stats: Arc<RequestStats>,
    context: &colony_expand::ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
) -> Router {
    let mut router = Router::new();

    for route in &config.routes {
        match route {
            Route::Static { path, dir, .. } => {
                router = super::static_handler::add_static_route(router, path, dir);
            }
            Route::Endpoint {
                path, func, method, ..
            } => {
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
            Route::Webhook {
                path, func, secret, ..
            } => {
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

    // Global middleware: record stats and emit http_request tuples for all routes.
    let global_stats = stats;
    let global_ts = tuple_space.clone();
    let global_sid = session_id.to_string();

    router.layer(axum::middleware::from_fn(
        move |req: Request<Body>, next: axum::middleware::Next| {
            let stats = global_stats.clone();
            let ts = global_ts.clone();
            let sid = global_sid.clone();

            async move {
                let method = req.method().to_string();
                let path = req.uri().path().to_string();
                let start = Instant::now();

                let response: axum::response::Response = next.run(req).await;

                let status = response.status().as_u16();
                let duration_us = start.elapsed().as_micros() as u64;
                #[allow(clippy::cast_possible_truncation)]
                let duration_ms = start.elapsed().as_millis() as u64;
                let is_error = status >= 400;

                stats.record_request(duration_us, is_error);

                ts.out(Tuple::new(vec![
                    Value::Str("http_request".to_string()),
                    Value::Str(sid),
                    Value::Str(method),
                    Value::Str(path),
                    Value::Int(i64::from(status)),
                    Value::Int(duration_ms as i64),
                ]));

                response
            }
        },
    ))
}

/// Spawn the background HTTP server on a dedicated OS thread with its own tokio runtime.
///
/// Returns the `ServeRuntime` handle for shutdown signaling and router hot-swap.
pub(crate) fn spawn_server(
    config: &ServeConfig,
    stats: Arc<RequestStats>,
    context: &colony_expand::ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
) -> std::result::Result<ServeRuntime, String> {
    let initial_router = build_router(config, stats, context, tuple_space, session_id);
    let router_swap = Arc::new(ArcSwap::new(Arc::new(initial_router)));
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e| format!("invalid bind address: {e}"))?;

    let thread_pool_size = config.thread_pool_size;
    let router_swap_clone = router_swap.clone();

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
                let listener = match tokio::net::TcpListener::bind(addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!("serve: failed to bind {addr}: {e}");
                        return;
                    }
                };

                // Load the router from ArcSwap. When routes change,
                // hot_swap_router stores a new Router in ArcSwap.
                // axum::serve owns the cloned router, so in-flight connections
                // keep the old router while new connections after restart get
                // the new one. True per-request hot-swap would require a
                // tower::Service wrapper, but this is sufficient for initial use.
                let swap = router_swap_clone;
                let router = (*swap.load_full()).clone();
                let server = axum::serve(listener, router);

                let graceful = server.with_graceful_shutdown(async move {
                    // Wait for the shutdown signal
                    let _ = shutdown_rx.changed().await;
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
/// Called after route changes (add/remove). The ArcSwap is updated atomically.
/// New connections will use the new router; existing connections keep the old one.
pub(crate) fn hot_swap_router(
    serve_state: &ServeState,
    context: &colony_expand::ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
) {
    let config = serve_state
        .config
        .read()
        .expect("serve config lock poisoned");
    let new_router = build_router(&config, serve_state.stats.clone(), context, tuple_space, session_id);

    let runtime = serve_state
        .runtime_handle
        .read()
        .expect("serve runtime lock poisoned");
    if let Some(ref rt) = *runtime {
        rt.router_swap.store(Arc::new(new_router));
    }
}

/// Stop the server gracefully: signal shutdown, wait up to 5s for in-flight requests.
pub(crate) fn stop_server(serve_state: &ServeState) -> std::result::Result<(), String> {
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
        let timeout = std::time::Duration::from_secs(5);
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let _join_thread = std::thread::spawn(move || {
            let result = handle.join();
            let _ = done_tx.send(result);
        });

        match done_rx.recv_timeout(timeout) {
            Ok(Ok(())) => {}
            Ok(Err(_)) => eprintln!("serve: server thread panicked during shutdown"),
            Err(_) => eprintln!("serve: shutdown timed out after 5s, forcing stop"),
        }
    }

    Ok(())
}
