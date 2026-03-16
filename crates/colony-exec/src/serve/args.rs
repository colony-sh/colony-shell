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
    let duration_us = serve_state
        .stats
        .total_duration_us
        .load(Ordering::Relaxed);

    let state_str = if running { "running" } else { "stopped" };
    let avg_ms = if total > 0 {
        duration_us / total / 1000
    } else {
        0
    };

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
        eprintln!(
            "serve static: directory '{}' does not exist",
            dir.display()
        );
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
        }
    }

    Ok(BuiltinResult::Success(0))
}
