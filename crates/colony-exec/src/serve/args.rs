//! Subcommand argument parsing and dispatch for the serve builtin.
//!
//! All subcommands parse args manually (no clap -- builtins parse args manually
//! following the flux/sidebar pattern).

use super::server;
use super::state::{MethodFilter, Route, ServeState};
use crate::builtins::BuiltinResult;
use crate::Result;
use colony_expand::ExpansionContext;
use colony_tuple::TupleSpace;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

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
                let Some(val) = args.get(i) else {
                    eprintln!("serve start: --threads requires a value");
                    return Ok(BuiltinResult::Failure(1));
                };
                let Ok(n) = val.parse::<usize>() else {
                    eprintln!("serve start: --threads must be a number");
                    return Ok(BuiltinResult::Failure(1));
                };
                threads = Some(n);
            }
            "--host" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    eprintln!("serve start: --host requires a value");
                    return Ok(BuiltinResult::Failure(1));
                };
                host = Some(val.clone());
            }
            arg if !arg.starts_with('-') && port.is_none() => {
                let Ok(p) = arg.parse::<u16>() else {
                    eprintln!("serve start: invalid port '{arg}'");
                    return Ok(BuiltinResult::Failure(1));
                };
                port = Some(p);
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
        let mut config = serve_state
            .config
            .write()
            .expect("serve config lock poisoned");
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
    let config = serve_state
        .config
        .read()
        .expect("serve config lock poisoned");
    match server::spawn_server(
        &config,
        serve_state.stats.clone(),
        context,
        tuple_space,
        session_id,
        serve_state.rack_pool.clone(),
    ) {
        Ok(runtime) => {
            let port = config.port;
            let host = config.host.clone();
            drop(config);
            *serve_state
                .runtime_handle
                .write()
                .expect("serve runtime lock poisoned") = Some(runtime);
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
#[allow(clippy::cast_possible_truncation)]
pub fn handle_status(serve_state: &ServeState) -> Result<BuiltinResult> {
    let config = serve_state
        .config
        .read()
        .expect("serve config lock poisoned");
    let running = serve_state.is_running();
    let total = serve_state.stats.total_requests.load(Ordering::Relaxed);
    let errors = serve_state.stats.total_errors.load(Ordering::Relaxed);
    let duration_us = serve_state.stats.total_duration_us.load(Ordering::Relaxed);

    let state_str = if running { "running" } else { "stopped" };
    let avg_ms = duration_us.checked_div(total).unwrap_or(0) / 1000;

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

    let mut config = serve_state
        .config
        .write()
        .expect("serve config lock poisoned");
    config.routes.push(Route::Static {
        path: url_path.clone(),
        dir,
        index,
    });
    drop(config);

    eprintln!("serve: static route added: {url_path}");
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

    let mut config = serve_state
        .config
        .write()
        .expect("serve config lock poisoned");
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

    let mut config = serve_state
        .config
        .write()
        .expect("serve config lock poisoned");
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

    let mut config = serve_state
        .config
        .write()
        .expect("serve config lock poisoned");
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
    let config = serve_state
        .config
        .read()
        .expect("serve config lock poisoned");

    if config.routes.is_empty() {
        println!("No routes registered.");
        return Ok(BuiltinResult::Success(0));
    }

    for route in &config.routes {
        match route {
            Route::Static {
                path, dir, index, ..
            } => {
                let idx = index.as_deref().unwrap_or("(none)");
                println!("  static   {path:<20} {:<30} index={idx}", dir.display());
            }
            Route::Endpoint {
                path, func, method, ..
            } => {
                let m = match method {
                    MethodFilter::Any => "ANY",
                    MethodFilter::Specific(m) => m,
                };
                println!("  endpoint {path:<20} {func:<30} method={m}");
            }
            Route::Webhook {
                path, func, secret, ..
            } => {
                let sig = if secret.is_some() { "yes" } else { "no" };
                println!("  webhook  {path:<20} {func:<30} secret={sig}");
            }
            Route::Rack {
                path,
                app_dir,
                workers,
                timeout_secs,
            } => {
                println!(
                    "  rack     {path:<20} {:<30} workers={workers} timeout={timeout_secs}s",
                    app_dir.display()
                );
            }
            Route::Cgi { path, executable } => {
                println!("  cgi      {path:<20} {}", executable.display());
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// `serve rack <path> <app_dir> [--workers N] [--timeout SECS]`
pub fn handle_rack(args: &[String], serve_state: &ServeState) -> Result<BuiltinResult> {
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
        eprintln!(
            "serve rack: directory '{}' does not exist",
            app_dir.display()
        );
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
    let mut config = serve_state
        .config
        .write()
        .expect("serve config lock poisoned");
    config.routes.push(Route::Rack {
        path: url_path.clone(),
        app_dir: app_dir.clone(),
        workers,
        timeout_secs,
    });
    drop(config);

    // Initialize or update the worker pool.
    let pool_result = super::rack_pool::RackWorkerPool::new(
        app_dir.clone(),
        worker_script,
        workers,
        timeout_secs,
    );

    // Temporary runtime for async pool assignment (tokio::sync::RwLock).
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create rack setup runtime");

    match pool_result {
        Ok(pool) => {
            rt.block_on(async {
                let mut pool_guard = serve_state.rack_pool.write().await;
                *pool_guard = Some(pool);
            });
            eprintln!(
                "serve: rack route added: {url_path} -> {} ({workers} workers)",
                app_dir.display()
            );
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("serve rack: failed to spawn workers: {e}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// `serve cgi <path> <executable>`
pub fn handle_cgi(args: &[String], serve_state: &ServeState) -> Result<BuiltinResult> {
    if args.len() < 2 {
        eprintln!("Usage: serve cgi <path> <executable>");
        return Ok(BuiltinResult::Failure(1));
    }

    let url_path = args[0].clone();
    let executable = PathBuf::from(&args[1]);

    if !executable.exists() {
        eprintln!(
            "serve cgi: executable '{}' does not exist",
            executable.display()
        );
        return Ok(BuiltinResult::Failure(1));
    }

    let mut config = serve_state
        .config
        .write()
        .expect("serve config lock poisoned");
    config.routes.push(Route::Cgi {
        path: url_path.clone(),
        executable: executable.clone(),
    });
    drop(config);

    eprintln!(
        "serve: cgi route added: {url_path} -> {}",
        executable.display()
    );
    Ok(BuiltinResult::Success(0))
}

/// `serve workers` -- display worker pool status.
pub fn handle_workers(serve_state: &ServeState) -> Result<BuiltinResult> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create workers display runtime");

    let info = rt.block_on(async {
        let pool_guard = serve_state.rack_pool.read().await;
        let pool = (*pool_guard).as_ref()?;
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

    println!(
        "  {:<6} {:<8} {:<8} {:<12} UPTIME",
        "IDX", "PID", "STATUS", "REQUESTS"
    );
    for w in &workers {
        let pid_str = w.pid.map_or("-".to_string(), |p| p.to_string());
        let uptime = format_uptime(w.uptime_secs);
        println!(
            "  {:<6} {:<8} {:<8} {:<12} {}",
            w.index, pid_str, w.status, w.requests_handled, uptime
        );
    }

    Ok(BuiltinResult::Success(0))
}

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

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

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
