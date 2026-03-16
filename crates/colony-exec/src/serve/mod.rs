//! `serve` -- Embedded HTTP server builtin for colony-shell.
//!
//! Runs an axum server on a background tokio runtime. Supports static file
//! serving, shell function endpoints, webhook routes with HMAC verification,
//! Rack application serving via worker pool, and CGI script execution.

mod args;
mod cgi_handler;
pub(crate) mod endpoint_handler;
mod rack_handler;
mod rack_pool;
mod rack_script;
pub(crate) mod server;
pub(crate) mod state;
mod static_handler;
mod webhook_handler;

pub use state::ServeState;

use crate::builtins::BuiltinResult;
use crate::Result;
use colony_expand::ExpansionContext;
use colony_tuple::TupleSpace;

/// Entry point for the `serve` builtin. Dispatches subcommands.
pub fn builtin_serve(
    args: &[String],
    context: &ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
    serve_state: &ServeState,
) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("serve: missing subcommand");
        eprintln!(
            "Usage: serve <start|stop|status|static|endpoint|webhook|rack|cgi|workers|remove|routes> [args...]"
        );
        return Ok(BuiltinResult::Failure(1));
    }

    let sub = args[0].as_str();
    let sub_args = &args[1..];

    let result = match sub {
        "start" => args::handle_start(sub_args, context, tuple_space, session_id, serve_state),
        "stop" => args::handle_stop(serve_state),
        "status" => args::handle_status(serve_state),
        "static" => args::handle_static(sub_args, serve_state),
        "endpoint" => args::handle_endpoint(sub_args, serve_state),
        "webhook" => args::handle_webhook(sub_args, serve_state),
        "rack" => args::handle_rack(sub_args, serve_state),
        "cgi" => args::handle_cgi(sub_args, serve_state),
        "workers" => {
            // Sub-dispatch: `serve workers`, `serve workers scale N`, `serve workers restart`
            if sub_args.is_empty() {
                args::handle_workers(serve_state)
            } else {
                match sub_args[0].as_str() {
                    "scale" => args::handle_workers_scale(&sub_args[1..], serve_state),
                    "restart" => args::handle_workers_restart(serve_state),
                    other => {
                        eprintln!("serve workers: unknown subcommand '{other}'");
                        Ok(BuiltinResult::Failure(1))
                    }
                }
            }
        }
        "remove" => args::handle_remove(sub_args, serve_state),
        "routes" => args::handle_routes(serve_state),
        other => {
            eprintln!("serve: unknown subcommand '{other}'");
            Ok(BuiltinResult::Failure(1))
        }
    };

    // After route-modifying subcommands, hot-swap the router if server is running
    if let Ok(ref r) = result {
        if r.is_success()
            && serve_state.is_running()
            && matches!(
                sub,
                "static" | "endpoint" | "webhook" | "rack" | "cgi" | "remove"
            )
        {
            server::hot_swap_router(serve_state, context, tuple_space, session_id);
        }
    }

    result
}
