//! Event router: persistent tuple event automation (F081)
//!
//! Provides:
//! - Listener registry (SQLite-backed, survives shell sessions)
//! - Filter expression parser/evaluator (no shell eval)
//! - `on` builtin command for managing listeners and the daemon
//!
//! The registry stores listeners in the shared reflection DB
//! (`~/.colony/reflection/observations.db`). The `colony-events` daemon
//! reads this table and polls the persistent tuple space for new tuples.
//! SQLite (WAL mode) is the communication channel between the builtin
//! and the daemon -- no custom IPC protocol needed.

pub mod filter;
pub mod registry;

pub use filter::{evaluate_filter, matches_filter, parse_filter, FilterError, FilterExpr};
pub use registry::{
    EventListener, EventRegistry, HandlerRun, HandlerType, RegistryError, RegistryResult,
};

use crate::builtins::BuiltinResult;
use std::path::PathBuf;

/// Detect handler type from the handler string.
///
/// - Paths (starting with `/`, `./`, `../`, or ending in `.sh`/`.py`/etc.) -> Script
/// - Otherwise -> CommandEval
///
/// Function type is only used when the caller explicitly knows it is a
/// function (e.g., via an `--function` flag, or by matching exported function
/// names in the expansion context). The default path is CommandEval.
pub fn detect_handler_type(handler: &str) -> HandlerType {
    let trimmed = handler.trim();

    if trimmed.starts_with('/') || trimmed.starts_with("./") || trimmed.starts_with("../") {
        return HandlerType::Script;
    }

    // Check for common script extensions
    let extensions = [".sh", ".py", ".rb", ".pl", ".js", ".ts"];
    if extensions.iter().any(|ext| trimmed.ends_with(ext)) {
        return HandlerType::Script;
    }

    HandlerType::CommandEval
}

/// Check if the daemon is running by reading the PID file.
pub fn daemon_pid() -> Option<u32> {
    let pid_path = daemon_pid_path();
    let pid_str = std::fs::read_to_string(&pid_path).ok()?;
    let pid: u32 = pid_str.trim().parse().ok()?;

    // Check if the process is actually alive
    // SAFETY: kill(pid, 0) checks if process exists without sending a signal.
    // It is safe to call with any PID and signal 0.
    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
    if alive {
        Some(pid)
    } else {
        // Stale PID file -- clean up
        let _ = std::fs::remove_file(&pid_path);
        None
    }
}

/// Path to the daemon PID file.
pub fn daemon_pid_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".colony/events/router.pid")
}

/// Path to the colony-events binary.
fn daemon_binary_path() -> PathBuf {
    // Try to find it next to the current executable first
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe
            .parent()
            .unwrap_or_else(|| std::path::Path::new("/usr/local/bin"));
        let candidate = dir.join("colony-events");
        if candidate.exists() {
            return candidate;
        }
    }

    // Fall back to PATH
    PathBuf::from("colony-events")
}

/// Start the daemon if not already running.
fn start_daemon() -> Result<(), String> {
    if daemon_pid().is_some() {
        return Ok(()); // already running
    }

    // Ensure PID directory exists
    let pid_path = daemon_pid_path();
    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create pid dir: {e}"))?;
    }

    let binary = daemon_binary_path();
    let result = std::process::Command::new(&binary)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    match result {
        Ok(_child) => {
            // Give the daemon a moment to start and write PID file
            std::thread::sleep(std::time::Duration::from_millis(200));
            Ok(())
        }
        Err(e) => Err(format!(
            "failed to start colony-events daemon ({}): {e}",
            binary.display()
        )),
    }
}

/// Stop the daemon by sending SIGTERM.
fn stop_daemon() -> Result<(), String> {
    let Some(pid) = daemon_pid() else {
        return Err("event router daemon is not running".to_string());
    };

    // SAFETY: kill(pid, SIGTERM) sends a termination signal. We verified the PID
    // is valid above via kill(pid, 0).
    let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    if result != 0 {
        return Err(format!("failed to send SIGTERM to daemon (pid {pid})"));
    }

    // Wait briefly for PID file removal
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if daemon_pid().is_none() {
            return Ok(());
        }
    }

    Ok(())
}

/// Execute the `on` builtin command.
///
/// This is called from `execute_builtin()` in `builtins.rs`.
pub fn builtin_on(args: &[String]) -> crate::Result<BuiltinResult> {
    if args.is_empty() {
        print_usage();
        return Ok(BuiltinResult::Failure(1));
    }

    let result = match args[0].as_str() {
        "--list" | "-l" => cmd_list(),
        "--remove" | "-r" => {
            let Some(id) = args.get(1) else {
                eprintln!("on --remove: missing listener ID");
                return Ok(BuiltinResult::Failure(1));
            };
            cmd_remove(id)
        }
        "--remove-all" => cmd_remove_all(),
        "--enable" => {
            let Some(id) = args.get(1) else {
                eprintln!("on --enable: missing listener ID");
                return Ok(BuiltinResult::Failure(1));
            };
            cmd_enable(id)
        }
        "--disable" => {
            let Some(id) = args.get(1) else {
                eprintln!("on --disable: missing listener ID");
                return Ok(BuiltinResult::Failure(1));
            };
            cmd_disable(id)
        }
        "--daemon" | "-d" => {
            let subcmd = args.get(1).map(String::as_str).unwrap_or("status");
            cmd_daemon(subcmd)
        }
        "--render" => cmd_render(),
        "--export" => cmd_export(),
        "--import" => {
            let Some(file) = args.get(1) else {
                eprintln!("on --import: missing file path");
                return Ok(BuiltinResult::Failure(1));
            };
            cmd_import(file)
        }
        _ => {
            // Parse: on [--once] [--filter '<expr>'] <pattern> <handler>
            let mut once = false;
            let mut filter = String::new();
            let mut i = 0;

            while i < args.len() {
                match args[i].as_str() {
                    "--once" => {
                        once = true;
                        i += 1;
                    }
                    "--filter" | "-f" => {
                        let Some(expr) = args.get(i + 1) else {
                            eprintln!("on --filter: missing filter expression");
                            return Ok(BuiltinResult::Failure(1));
                        };
                        // Validate the filter expression at registration time
                        if let Err(e) = parse_filter(expr) {
                            eprintln!("on: invalid filter expression: {e}");
                            return Ok(BuiltinResult::Failure(1));
                        }
                        filter = expr.clone();
                        i += 2;
                    }
                    _ => break,
                }
            }

            let remaining = &args[i..];
            if remaining.len() < 2 {
                eprintln!("on: expected <pattern> <handler>");
                print_usage();
                return Ok(BuiltinResult::Failure(1));
            }

            let pattern = &remaining[0];
            let handler = remaining[1..].join(" ");

            cmd_register(pattern, &handler, &filter, once)
        }
    };

    Ok(result)
}

/// Register a new listener.
fn cmd_register(pattern: &str, handler: &str, filter: &str, once: bool) -> BuiltinResult {
    let Ok(registry) = open_registry() else {
        return BuiltinResult::Failure(1);
    };
    let handler_type = detect_handler_type(handler);

    let id = match registry.add_listener(pattern, handler, handler_type.clone(), filter, once, "") {
        Ok(id) => id,
        Err(e) => {
            eprintln!("on: failed to register listener: {e}");
            return BuiltinResult::Failure(1);
        }
    };

    let short_id = &id[..8];
    let mut desc = format!("Registered: {short_id} on {pattern}");
    if !filter.is_empty() {
        desc.push_str(&format!(" [filter: {filter}]"));
    }
    if once {
        desc.push_str(" (once)");
    }
    desc.push_str(&format!(" -> {handler} ({})", handler_type));
    eprintln!("{desc}");

    // Auto-start daemon
    if let Err(e) = start_daemon() {
        eprintln!("on: warning: could not auto-start daemon: {e}");
    }

    BuiltinResult::Success(0)
}

/// List all listeners.
fn cmd_list() -> BuiltinResult {
    let Ok(registry) = open_registry() else {
        return BuiltinResult::Failure(1);
    };
    let Ok(listeners) = registry.list_listeners() else {
        eprintln!("on --list: failed to query listeners");
        return BuiltinResult::Failure(1);
    };

    if listeners.is_empty() {
        eprintln!("No event listeners registered.");
        return BuiltinResult::Success(0);
    }

    eprintln!(
        "{:<10} {:<6} {:<20} {:<30} {:<8} {}",
        "ID", "ON", "PATTERN", "HANDLER", "TYPE", "FILTER"
    );
    eprintln!("{}", "-".repeat(90));

    for listener in &listeners {
        let short_id = &listener.id[..8.min(listener.id.len())];
        let enabled_mark = if listener.enabled { "yes" } else { "no" };
        let handler_display = if listener.handler.len() > 28 {
            format!("{}...", &listener.handler[..25])
        } else {
            listener.handler.clone()
        };
        let once_suffix = if listener.once { " (once)" } else { "" };

        eprintln!(
            "{:<10} {:<6} {:<20} {:<30} {:<8} {}{}",
            short_id,
            enabled_mark,
            listener.pattern,
            handler_display,
            listener.handler_type,
            listener.filter,
            once_suffix,
        );
    }

    eprintln!("\n{} listener(s) registered.", listeners.len());

    // Show daemon status
    if let Some(pid) = daemon_pid() {
        eprintln!("Daemon: running (pid {pid})");
    } else {
        eprintln!("Daemon: not running");
    }

    BuiltinResult::Success(0)
}

/// Remove a listener.
fn cmd_remove(id: &str) -> BuiltinResult {
    let Ok(registry) = open_registry() else {
        return BuiltinResult::Failure(1);
    };
    match registry.remove_listener(id) {
        Ok(true) => {
            eprintln!("Removed listener {id}");
            BuiltinResult::Success(0)
        }
        Ok(false) | Err(RegistryError::NotFound(_)) => {
            eprintln!("on --remove: listener not found: {id}");
            BuiltinResult::Failure(1)
        }
        Err(RegistryError::AmbiguousPrefix(_)) => {
            eprintln!("on --remove: ambiguous prefix '{id}' matches multiple listeners");
            BuiltinResult::Failure(1)
        }
        Err(e) => {
            eprintln!("on --remove: {e}");
            BuiltinResult::Failure(1)
        }
    }
}

/// Remove all listeners.
fn cmd_remove_all() -> BuiltinResult {
    let Ok(registry) = open_registry() else {
        return BuiltinResult::Failure(1);
    };
    match registry.remove_all() {
        Ok(count) => {
            eprintln!("Removed {count} listener(s).");
            BuiltinResult::Success(0)
        }
        Err(e) => {
            eprintln!("on --remove-all: {e}");
            BuiltinResult::Failure(1)
        }
    }
}

/// Enable a listener.
fn cmd_enable(id: &str) -> BuiltinResult {
    let Ok(registry) = open_registry() else {
        return BuiltinResult::Failure(1);
    };
    match registry.enable_listener(id) {
        Ok(true) => {
            eprintln!("Enabled listener {id}");
            BuiltinResult::Success(0)
        }
        Ok(false) => {
            eprintln!("on --enable: listener not found: {id}");
            BuiltinResult::Failure(1)
        }
        Err(e) => {
            eprintln!("on --enable: {e}");
            BuiltinResult::Failure(1)
        }
    }
}

/// Disable a listener.
fn cmd_disable(id: &str) -> BuiltinResult {
    let Ok(registry) = open_registry() else {
        return BuiltinResult::Failure(1);
    };
    match registry.disable_listener(id) {
        Ok(true) => {
            eprintln!("Disabled listener {id}");
            BuiltinResult::Success(0)
        }
        Ok(false) => {
            eprintln!("on --disable: listener not found: {id}");
            BuiltinResult::Failure(1)
        }
        Err(e) => {
            eprintln!("on --disable: {e}");
            BuiltinResult::Failure(1)
        }
    }
}

/// Daemon management subcommand.
fn cmd_daemon(subcmd: &str) -> BuiltinResult {
    match subcmd {
        "start" => match start_daemon() {
            Ok(()) => {
                if let Some(pid) = daemon_pid() {
                    eprintln!("Event router daemon started (pid {pid})");
                } else {
                    eprintln!("Event router daemon started");
                }
                BuiltinResult::Success(0)
            }
            Err(e) => {
                eprintln!("on --daemon start: {e}");
                BuiltinResult::Failure(1)
            }
        },

        "stop" => match stop_daemon() {
            Ok(()) => {
                eprintln!("Event router daemon stopped");
                BuiltinResult::Success(0)
            }
            Err(e) => {
                eprintln!("on --daemon stop: {e}");
                BuiltinResult::Failure(1)
            }
        },

        "restart" => {
            let _ = stop_daemon();
            std::thread::sleep(std::time::Duration::from_millis(500));
            match start_daemon() {
                Ok(()) => {
                    eprintln!("Event router daemon restarted");
                    BuiltinResult::Success(0)
                }
                Err(e) => {
                    eprintln!("on --daemon restart: {e}");
                    BuiltinResult::Failure(1)
                }
            }
        }

        "status" => {
            if let Some(pid) = daemon_pid() {
                eprintln!("Event router daemon: running (pid {pid})");

                // Show listener count
                if let Ok(registry) = open_registry() {
                    if let Ok(listeners) = registry.list_listeners() {
                        let enabled = listeners.iter().filter(|l| l.enabled).count();
                        eprintln!("Listeners: {} total, {} enabled", listeners.len(), enabled);
                    }
                    if let Ok(runs) = registry.recent_runs(1) {
                        if let Some(last) = runs.first() {
                            let ago = chrono::Utc::now().timestamp() - last.timestamp;
                            eprintln!("Last execution: {}s ago", ago);
                        }
                    }
                }
            } else {
                eprintln!("Event router daemon: not running");
            }
            BuiltinResult::Success(0)
        }

        _ => {
            eprintln!("on --daemon: unknown subcommand: {subcmd}");
            eprintln!("Usage: on --daemon start|stop|status|restart");
            BuiltinResult::Failure(1)
        }
    }
}

/// Render the HTML dashboard to stdout.
fn cmd_render() -> BuiltinResult {
    let Ok(registry) = open_registry() else {
        return BuiltinResult::Failure(1);
    };
    let Ok(listeners) = registry.list_listeners() else {
        eprintln!("on --render: failed to query listeners");
        return BuiltinResult::Failure(1);
    };
    let Ok(runs) = registry.recent_runs(50) else {
        eprintln!("on --render: failed to query runs");
        return BuiltinResult::Failure(1);
    };

    let html = render_dashboard(&listeners, &runs);
    print!("{html}");

    BuiltinResult::Success(0)
}

/// Export listeners as TOML.
fn cmd_export() -> BuiltinResult {
    let Ok(registry) = open_registry() else {
        return BuiltinResult::Failure(1);
    };
    match registry.export_listeners() {
        Ok(toml_output) => {
            print!("{toml_output}");
            BuiltinResult::Success(0)
        }
        Err(e) => {
            eprintln!("on --export: {e}");
            BuiltinResult::Failure(1)
        }
    }
}

/// Import listeners from a TOML file.
fn cmd_import(file: &str) -> BuiltinResult {
    let Ok(content) = std::fs::read_to_string(file) else {
        eprintln!("on --import: cannot read {file}");
        return BuiltinResult::Failure(1);
    };

    let Ok(registry) = open_registry() else {
        return BuiltinResult::Failure(1);
    };
    match registry.import_listeners(&content) {
        Ok(count) => {
            eprintln!("Imported {count} listener(s) from {file}");
            BuiltinResult::Success(0)
        }
        Err(e) => {
            eprintln!("on --import: {e}");
            BuiltinResult::Failure(1)
        }
    }
}

/// Open the event registry (shared reflection DB).
fn open_registry() -> Result<EventRegistry, RegistryError> {
    let path = EventRegistry::default_path();
    EventRegistry::open(&path).map_err(|e| {
        eprintln!("on: cannot open registry: {e}");
        e
    })
}

/// Print usage help.
fn print_usage() {
    eprintln!("Usage: on [options] <pattern> <handler>");
    eprintln!("       on --list                         List registered listeners");
    eprintln!("       on --remove <id>                  Remove listener by ID/prefix");
    eprintln!("       on --remove-all                   Remove all listeners");
    eprintln!("       on --enable <id>                  Enable a disabled listener");
    eprintln!("       on --disable <id>                 Disable a listener");
    eprintln!("       on --daemon start|stop|status     Manage event router daemon");
    eprintln!("       on --render                       Output HTML dashboard");
    eprintln!("       on --export                       Export listeners as TOML");
    eprintln!("       on --import <file>                Import listeners from TOML");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --once              Fire once, then auto-remove");
    eprintln!("  --filter '<expr>'   Only fire when filter matches");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  on http_request 'echo \"$2 $3 -> $4\"'");
    eprintln!("  on --filter '$4 >= 500' http_request alert_errors");
    eprintln!("  on --once deploy_complete 'notify-send \"Deploy done\"'");
}

// ---------------------------------------------------------------------------
// Dashboard HTML rendering
// ---------------------------------------------------------------------------

/// Render the event router dashboard as self-contained HTML.
fn render_dashboard(listeners: &[EventListener], runs: &[HandlerRun]) -> String {
    let listeners_html = render_listeners_section(listeners);
    let runs_html = render_runs_section(runs);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="refresh" content="5">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Colony Event Router</title>
<style>
{css}
</style>
</head>
<body>
<div class="container">
<h1>Colony Event Router</h1>
{daemon_status}
{listeners_html}
{runs_html}
<footer>
<p>Auto-refreshes every 5s. Generated by <code>on --render</code>.</p>
</footer>
</div>
</body>
</html>"#,
        css = DASHBOARD_CSS,
        daemon_status = render_daemon_status(),
    )
}

fn render_daemon_status() -> String {
    let status = if daemon_pid().is_some() {
        r#"<span class="badge running">running</span>"#
    } else {
        r#"<span class="badge stopped">stopped</span>"#
    };

    format!(
        r#"<section class="status">
<h2>Daemon Status</h2>
<p>Event Router: {status}</p>
</section>"#
    )
}

fn render_listeners_section(listeners: &[EventListener]) -> String {
    if listeners.is_empty() {
        return r#"<section class="listeners">
<h2>Listeners</h2>
<p class="empty">No listeners registered. Use <code>on &lt;pattern&gt; &lt;handler&gt;</code> to add one.</p>
</section>"#
            .to_string();
    }

    let mut rows = String::new();
    for listener in listeners {
        let short_id = &listener.id[..8.min(listener.id.len())];
        let enabled_class = if listener.enabled {
            "enabled"
        } else {
            "disabled"
        };
        let enabled_icon = if listener.enabled {
            "&#x2713;"
        } else {
            "&#x2717;"
        };
        let once_badge = if listener.once {
            r#" <span class="badge once">once</span>"#
        } else {
            ""
        };
        let handler_escaped = html_escape(&listener.handler);
        let filter_cell = if listener.filter.is_empty() {
            String::new()
        } else {
            format!("<code>{}</code>", html_escape(&listener.filter))
        };

        rows.push_str(&format!(
            r#"<tr class="{enabled_class}">
<td><code>{short_id}</code></td>
<td>{enabled_icon}</td>
<td><code>{pattern}</code>{once_badge}</td>
<td><code>{handler_escaped}</code></td>
<td>{handler_type}</td>
<td>{filter_cell}</td>
</tr>
"#,
            pattern = html_escape(&listener.pattern),
            handler_type = listener.handler_type,
        ));
    }

    format!(
        r#"<section class="listeners">
<h2>Listeners ({count})</h2>
<table>
<thead><tr><th>ID</th><th>On</th><th>Pattern</th><th>Handler</th><th>Type</th><th>Filter</th></tr></thead>
<tbody>
{rows}
</tbody>
</table>
</section>"#,
        count = listeners.len(),
    )
}

fn render_runs_section(runs: &[HandlerRun]) -> String {
    if runs.is_empty() {
        return r#"<section class="runs">
<h2>Execution History</h2>
<p class="empty">No handler executions yet.</p>
</section>"#
            .to_string();
    }

    let mut rows = String::new();
    for run in runs {
        let short_listener = &run.listener_id[..8.min(run.listener_id.len())];
        let exit_class = if run.exit_code == Some(0) {
            "success"
        } else {
            "error"
        };
        let exit_display = run
            .exit_code
            .map_or("running".to_string(), |c| c.to_string());
        let ts = chrono::DateTime::from_timestamp(run.timestamp, 0)
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "?".to_string());

        rows.push_str(&format!(
            r#"<tr class="{exit_class}">
<td>{ts}</td>
<td><code>{short_listener}</code></td>
<td><code>{pattern}</code></td>
<td>exit:{exit_display}</td>
<td>{duration}ms</td>
</tr>
"#,
            pattern = html_escape(&run.pattern),
            duration = run.duration_ms,
        ));
    }

    format!(
        r#"<section class="runs">
<h2>Execution History</h2>
<table>
<thead><tr><th>Time</th><th>Listener</th><th>Pattern</th><th>Exit</th><th>Duration</th></tr></thead>
<tbody>
{rows}
</tbody>
</table>
</section>"#,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const DASHBOARD_CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, monospace; background: #0d1117; color: #c9d1d9; padding: 20px; }
.container { max-width: 1000px; margin: 0 auto; }
h1 { color: #58a6ff; margin-bottom: 20px; font-size: 1.5em; }
h2 { color: #8b949e; font-size: 1.1em; margin-bottom: 12px; border-bottom: 1px solid #21262d; padding-bottom: 6px; }
section { margin-bottom: 24px; background: #161b22; border: 1px solid #21262d; border-radius: 6px; padding: 16px; }
table { width: 100%; border-collapse: collapse; font-size: 0.9em; }
th { text-align: left; color: #8b949e; padding: 6px 8px; border-bottom: 1px solid #21262d; }
td { padding: 6px 8px; border-bottom: 1px solid #21262d; }
tr:hover { background: #1c2128; }
tr.error td { color: #f85149; }
tr.disabled td { opacity: 0.5; }
code { background: #1c2128; padding: 2px 6px; border-radius: 3px; font-size: 0.9em; }
.badge { display: inline-block; padding: 2px 8px; border-radius: 10px; font-size: 0.75em; font-weight: 600; margin-left: 6px; }
.badge.running { background: #238636; color: #fff; }
.badge.stopped { background: #da3633; color: #fff; }
.badge.once { background: #1f6feb; color: #fff; }
.empty { color: #8b949e; font-style: italic; }
footer { color: #484f58; font-size: 0.8em; margin-top: 20px; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_handler_type_command() {
        assert_eq!(detect_handler_type("echo $2"), HandlerType::CommandEval);
        assert_eq!(
            detect_handler_type("alert_errors"),
            HandlerType::CommandEval
        );
    }

    #[test]
    fn test_detect_handler_type_script() {
        assert_eq!(
            detect_handler_type("./scripts/notify.sh"),
            HandlerType::Script
        );
        assert_eq!(
            detect_handler_type("/usr/local/bin/handler"),
            HandlerType::Script
        );
        assert_eq!(detect_handler_type("handler.py"), HandlerType::Script);
        assert_eq!(detect_handler_type("../handler.sh"), HandlerType::Script);
    }

    #[test]
    fn test_daemon_pid_path() {
        let path = daemon_pid_path();
        assert!(path.ends_with(".colony/events/router.pid"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"hello\""), "&quot;hello&quot;");
    }

    #[test]
    fn test_render_dashboard_empty() {
        let html = render_dashboard(&[], &[]);
        assert!(html.contains("Colony Event Router"));
        assert!(html.contains("No listeners registered"));
        assert!(html.contains("No handler executions yet"));
    }

    #[test]
    fn test_render_dashboard_with_listeners() {
        let listener = EventListener {
            id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
            pattern: "http_request".to_string(),
            handler: "echo $2".to_string(),
            handler_type: HandlerType::CommandEval,
            filter: "$4 >= 500".to_string(),
            once: false,
            enabled: true,
            description: String::new(),
            created_at: 1_700_000_000,
        };

        let html = render_dashboard(&[listener], &[]);
        assert!(html.contains("http_request"));
        assert!(html.contains("echo $2"));
        assert!(html.contains("aaaaaaaa")); // short ID
    }
}
