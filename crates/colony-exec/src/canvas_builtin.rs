//! Canvas builtin -- interact with Colony Nerve canvas from the shell.
//!
//! Reads `COLONY_API`, `COLONY_SESSION_TOKEN`, and `COLONY_CANVAS_ID` from
//! the environment and makes REST calls via `curl`.  This avoids adding an
//! HTTP client dependency to colony-exec; curl is universally available on
//! macOS and Linux.
//!
//! # Subcommands
//!
//! | Subcommand | Description |
//! |---|---|
//! | `status` | Show canvas info |
//! | `participants` | List humans + agents |
//! | `widgets` | List all widgets |
//! | `announce <text>` | Broadcast working status |
//! | `widget create\|move\|resize\|remove\|config` | Widget CRUD |
//! | `tasks` | Read task board |
//! | `markdown <id>` | Read markdown widget |
//! | `help` | Print help text |

use std::process::Command;

use crate::builtins::BuiltinResult;
use crate::Result;

/// Configuration extracted from the environment for canvas API calls.
struct CanvasEnv {
    api: String,
    token: String,
    canvas_id: String,
}

/// Read the required environment variables or print an error and return `None`.
fn read_env() -> Option<CanvasEnv> {
    let api = match std::env::var("COLONY_API") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("canvas: not on a canvas (COLONY_API not set)");
            return None;
        }
    };
    let token = match std::env::var("COLONY_SESSION_TOKEN") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("canvas: no auth token (COLONY_SESSION_TOKEN not set)");
            return None;
        }
    };
    let canvas_id = match std::env::var("COLONY_CANVAS_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("canvas: no canvas ID (COLONY_CANVAS_ID not set)");
            return None;
        }
    };
    Some(CanvasEnv {
        api,
        token,
        canvas_id,
    })
}

/// Execute a `canvas` builtin invocation.
pub fn builtin_canvas(args: &[String]) -> Result<BuiltinResult> {
    let subcmd = args.first().map(String::as_str).unwrap_or("status");

    // Help works off-canvas too
    if matches!(subcmd, "help" | "--help" | "-h") {
        print_help();
        return Ok(BuiltinResult::Success(0));
    }

    let Some(env) = read_env() else {
        return Ok(BuiltinResult::Failure(1));
    };

    let code = match subcmd {
        "status" => cmd_status(&env),
        "participants" => cmd_participants(&env),
        "widgets" => cmd_widgets(&env),
        "announce" => cmd_announce(&env, &args[1..]),
        "widget" => cmd_widget_crud(&env, &args[1..]),
        "tasks" => cmd_tasks(&env),
        "markdown" => cmd_markdown(&env, &args[1..]),
        other => {
            eprintln!("canvas: unknown subcommand '{other}'. Try 'canvas help'.");
            1
        }
    };

    if code == 0 {
        Ok(BuiltinResult::Success(0))
    } else {
        Ok(BuiltinResult::Failure(code))
    }
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

/// Perform a GET request via curl. Returns the response body on success.
fn curl_get(api: &str, token: &str, path: &str) -> Option<String> {
    let url = format!("{api}{path}");
    let output = Command::new("curl")
        .args([
            "-s",
            "-H",
            &format!("Authorization: Bearer {token}"),
            &url,
        ])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

/// Perform a POST request via curl with a JSON body.
fn curl_post(api: &str, token: &str, path: &str, body: &str) -> Option<String> {
    let url = format!("{api}{path}");
    let output = Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            "-H",
            &format!("Authorization: Bearer {token}"),
            "-H",
            "Content-Type: application/json",
            "-d",
            body,
            &url,
        ])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

/// Perform a PATCH request via curl with a JSON body.
fn curl_patch(api: &str, token: &str, path: &str, body: &str) -> Option<String> {
    let url = format!("{api}{path}");
    let output = Command::new("curl")
        .args([
            "-s",
            "-X",
            "PATCH",
            "-H",
            &format!("Authorization: Bearer {token}"),
            "-H",
            "Content-Type: application/json",
            "-d",
            body,
            &url,
        ])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

/// Perform a DELETE request via curl.
fn curl_delete(api: &str, token: &str, path: &str) -> Option<String> {
    let url = format!("{api}{path}");
    let output = Command::new("curl")
        .args([
            "-s",
            "-X",
            "DELETE",
            "-H",
            &format!("Authorization: Bearer {token}"),
            &url,
        ])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

fn cmd_status(env: &CanvasEnv) -> i32 {
    let path = format!("/api/canvases/{}", env.canvas_id);
    let Some(json) = curl_get(&env.api, &env.token, &path) else {
        eprintln!("canvas: failed to fetch canvas status");
        return 1;
    };

    let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else {
        println!("{json}");
        return 0;
    };

    let data = if v.get("data").is_some() { &v["data"] } else { &v };
    let name = data["name"].as_str().unwrap_or("unknown");
    let id_display = if env.canvas_id.len() >= 8 {
        &env.canvas_id[..8]
    } else {
        &env.canvas_id
    };
    println!("Canvas: {name} ({id_display})");

    if let Some(widgets) = data["widgets"].as_array() {
        println!("Widgets: {}", widgets.len());
    }

    0
}

fn cmd_participants(env: &CanvasEnv) -> i32 {
    let path = format!("/api/canvases/{}/participants", env.canvas_id);
    let Some(json) = curl_get(&env.api, &env.token, &path) else {
        eprintln!("canvas: failed to fetch participants");
        return 1;
    };

    let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else {
        println!("{json}");
        return 0;
    };

    let Some(participants) = v["participants"].as_array() else {
        println!("{json}");
        return 0;
    };

    for p in participants {
        let kind = if p["is_agent"].as_bool().unwrap_or(false) {
            "AGENT"
        } else {
            "HUMAN"
        };
        let name = p["display_name"].as_str().unwrap_or("unknown");
        let status = p["status"].as_str().unwrap_or("online");
        let detail = p["detail"].as_str().unwrap_or("\u{2014}");
        println!("{kind:<6} {name:<15} {status:<12} {detail}");
    }

    0
}

fn cmd_widgets(env: &CanvasEnv) -> i32 {
    let path = format!("/api/canvases/{}", env.canvas_id);
    let Some(json) = curl_get(&env.api, &env.token, &path) else {
        eprintln!("canvas: failed to fetch widgets");
        return 1;
    };

    let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else {
        println!("{json}");
        return 0;
    };

    let data = if v.get("data").is_some() { &v["data"] } else { &v };

    let Some(widgets) = data["widgets"].as_array() else {
        eprintln!("canvas: no widgets in response");
        return 1;
    };

    println!(
        "{:<10} {:<12} {:<14} {:<12} {}",
        "ID", "TYPE", "POSITION", "SIZE", "NAME"
    );

    for w in widgets {
        let id = w["id"].as_str().unwrap_or("?");
        let plugin = w["plugin_id"].as_str().unwrap_or("?");
        let x = w["position_x"]
            .as_f64()
            .or_else(|| w["placement"]["x"].as_f64())
            .unwrap_or(0.0);
        let y = w["position_y"]
            .as_f64()
            .or_else(|| w["placement"]["y"].as_f64())
            .unwrap_or(0.0);
        let width = w["width"]
            .as_f64()
            .or_else(|| w["placement"]["width"].as_f64())
            .unwrap_or(0.0);
        let height = w["height"]
            .as_f64()
            .or_else(|| w["placement"]["height"].as_f64())
            .unwrap_or(0.0);
        let id_short = &id[..8.min(id.len())];
        println!(
            "{id_short:<10} {plugin:<12} {x:.0},{y:.0}{:<6} {width:.0}x{height:.0}{:<4} \u{2014}",
            "", ""
        );
    }

    0
}

fn cmd_announce(env: &CanvasEnv, args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!("Usage: canvas announce <status text>");
        eprintln!("       canvas announce --files file1.rs file2.rs");
        eprintln!("       canvas announce --clear");
        return 1;
    }

    if args[0] == "--clear" {
        let body = r#"{"status": "idle", "detail": null, "files": []}"#;
        let path = format!("/api/canvases/{}/announce", env.canvas_id);
        curl_post(&env.api, &env.token, &path, body);
        println!("Status cleared.");
        return 0;
    }

    let mut detail = String::new();
    let mut files: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--files" {
            i += 1;
            while i < args.len() && !args[i].starts_with("--") {
                files.push(args[i].clone());
                i += 1;
            }
        } else {
            if !detail.is_empty() {
                detail.push(' ');
            }
            detail.push_str(&args[i]);
            i += 1;
        }
    }

    let files_json: Vec<String> = files.iter().map(|f| format!("\"{f}\"")).collect();
    let escaped_detail = detail.replace('"', "\\\"");
    let body = format!(
        r#"{{"status": "working", "detail": "{escaped_detail}", "files": [{}]}}"#,
        files_json.join(",")
    );
    let path = format!("/api/canvases/{}/announce", env.canvas_id);
    curl_post(&env.api, &env.token, &path, &body);
    println!("Announced: {detail}");
    0
}

fn cmd_widget_crud(env: &CanvasEnv, args: &[String]) -> i32 {
    let subcmd = args.first().map(String::as_str).unwrap_or("help");

    match subcmd {
        "create" => widget_create(env, &args[1..]),
        "move" => widget_move(env, &args[1..]),
        "resize" => widget_resize(env, &args[1..]),
        "remove" => widget_remove(env, &args[1..]),
        "config" => widget_config(env, &args[1..]),
        _ => {
            eprintln!("Usage: canvas widget <create|move|resize|remove|config> ...");
            1
        }
    }
}

fn widget_create(env: &CanvasEnv, args: &[String]) -> i32 {
    let Some(widget_type) = args.first() else {
        eprintln!("Usage: canvas widget create <type> [--title X] [--position X,Y] [--size WxH]");
        return 1;
    };

    let mut title = String::new();
    let mut x = 100.0_f64;
    let mut y = 100.0_f64;
    let mut w = 400.0_f64;
    let mut h = 300.0_f64;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--title" => {
                i += 1;
                title = args.get(i).cloned().unwrap_or_default();
            }
            "--position" => {
                i += 1;
                if let Some(pos) = args.get(i) {
                    if let Some((px, py)) = pos.split_once(',') {
                        x = px.parse().unwrap_or(100.0);
                        y = py.parse().unwrap_or(100.0);
                    }
                }
            }
            "--size" => {
                i += 1;
                if let Some(size) = args.get(i) {
                    if let Some((sw, sh)) = size.split_once('x') {
                        w = sw.parse().unwrap_or(400.0);
                        h = sh.parse().unwrap_or(300.0);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    let _ = title; // Reserved for future use in config_data
    let body = format!(
        r#"{{"plugin_id": "{widget_type}", "position_x": {x}, "position_y": {y}, "width": {w}, "height": {h}, "config_data": {{}}}}"#,
    );
    let path = format!("/api/canvases/{}/widgets", env.canvas_id);

    let Some(resp) = curl_post(&env.api, &env.token, &path, &body) else {
        eprintln!("canvas: failed to create widget");
        return 1;
    };

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resp) {
        let data = if v.get("data").is_some() { &v["data"] } else { &v };
        let id = data["id"].as_str().unwrap_or("?");
        println!("Created {widget_type} widget: {id}");
    }

    0
}

fn widget_move(env: &CanvasEnv, args: &[String]) -> i32 {
    let Some(id) = args.first() else {
        eprintln!("Usage: canvas widget move <id> <x,y>");
        return 1;
    };
    let Some(pos) = args.get(1) else {
        eprintln!("Usage: canvas widget move <id> <x,y>");
        return 1;
    };
    let (px, py) = pos.split_once(',').unwrap_or(("0", "0"));
    let body = format!(r#"{{"position_x": {px}, "position_y": {py}}}"#);
    let path = format!("/api/widgets/{id}");
    curl_patch(&env.api, &env.token, &path, &body);
    println!("Moved {id} to {pos}");
    0
}

fn widget_resize(env: &CanvasEnv, args: &[String]) -> i32 {
    let Some(id) = args.first() else {
        eprintln!("Usage: canvas widget resize <id> <WxH>");
        return 1;
    };
    let Some(size) = args.get(1) else {
        eprintln!("Usage: canvas widget resize <id> <WxH>");
        return 1;
    };
    let (w, h) = size.split_once('x').unwrap_or(("400", "300"));
    let body = format!(r#"{{"width": {w}, "height": {h}}}"#);
    let path = format!("/api/widgets/{id}");
    curl_patch(&env.api, &env.token, &path, &body);
    println!("Resized {id} to {size}");
    0
}

fn widget_remove(env: &CanvasEnv, args: &[String]) -> i32 {
    let Some(id) = args.first() else {
        eprintln!("Usage: canvas widget remove <id>");
        return 1;
    };
    let path = format!("/api/widgets/{id}");
    curl_delete(&env.api, &env.token, &path);
    println!("Removed widget {id}");
    0
}

fn widget_config(env: &CanvasEnv, args: &[String]) -> i32 {
    let Some(id) = args.first() else {
        eprintln!("Usage: canvas widget config <id> [json]");
        return 1;
    };

    if let Some(json_body) = args.get(1) {
        // PATCH config
        let body = format!(r#"{{"config_data": {json_body}}}"#);
        let path = format!("/api/widgets/{id}");
        curl_patch(&env.api, &env.token, &path, &body);
        println!("Updated config for {id}");
    } else {
        // GET config
        let path = format!("/api/widgets/{id}/content");
        if let Some(resp) = curl_get(&env.api, &env.token, &path) {
            println!("{resp}");
        }
    }

    0
}

fn cmd_tasks(env: &CanvasEnv) -> i32 {
    let path = format!("/api/canvases/{}/tasks", env.canvas_id);
    let Some(json) = curl_get(&env.api, &env.token, &path) else {
        eprintln!("canvas: failed to fetch tasks");
        return 1;
    };
    println!("{json}");
    0
}

fn cmd_markdown(env: &CanvasEnv, args: &[String]) -> i32 {
    let Some(id) = args.first() else {
        eprintln!("Usage: canvas markdown <widget-id>");
        return 1;
    };
    let path = format!("/api/widgets/{id}/content");
    if let Some(json) = curl_get(&env.api, &env.token, &path) {
        println!("{json}");
    }
    0
}

fn print_help() {
    println!("Usage: canvas <subcommand>");
    println!();
    println!("Subcommands:");
    println!("  status                          Show canvas info");
    println!("  participants                    List humans + agents");
    println!("  widgets                         List all widgets");
    println!("  announce <text>                 Broadcast working status");
    println!("  announce --files f1 f2          Announce with file list");
    println!("  announce --clear                Clear status");
    println!("  widget create <type> [opts]     Create widget");
    println!("  widget move <id> <x,y>          Move widget");
    println!("  widget resize <id> <WxH>        Resize widget");
    println!("  widget remove <id>              Remove widget");
    println!("  widget config <id> [json]       Read/write config");
    println!("  tasks                           Read task board");
    println!("  markdown <id>                   Read markdown widget");
    println!("  help                            This help");
    println!();
    println!("Environment:");
    println!("  COLONY_API               Base URL (e.g. https://api.colony.sh)");
    println!("  COLONY_SESSION_TOKEN     Auth bearer token");
    println!("  COLONY_CANVAS_ID         Current canvas UUID");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_returns_success() {
        let result = builtin_canvas(&["help".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Success(0));
    }

    #[test]
    fn test_unknown_subcommand_returns_failure() {
        // Set dummy env vars so we don't fail on env check
        std::env::set_var("COLONY_API", "http://localhost:4000");
        std::env::set_var("COLONY_SESSION_TOKEN", "test-token");
        std::env::set_var("COLONY_CANVAS_ID", "test-canvas-id");

        let result = builtin_canvas(&["bogus".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));

        // Clean up
        std::env::remove_var("COLONY_API");
        std::env::remove_var("COLONY_SESSION_TOKEN");
        std::env::remove_var("COLONY_CANVAS_ID");
    }

    #[test]
    fn test_missing_env_returns_failure() {
        std::env::remove_var("COLONY_API");
        std::env::remove_var("COLONY_SESSION_TOKEN");
        std::env::remove_var("COLONY_CANVAS_ID");

        let result = builtin_canvas(&["status".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));
    }

    #[test]
    fn test_announce_no_args_returns_failure() {
        std::env::set_var("COLONY_API", "http://localhost:4000");
        std::env::set_var("COLONY_SESSION_TOKEN", "test-token");
        std::env::set_var("COLONY_CANVAS_ID", "test-canvas-id");

        let result = builtin_canvas(&["announce".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));

        std::env::remove_var("COLONY_API");
        std::env::remove_var("COLONY_SESSION_TOKEN");
        std::env::remove_var("COLONY_CANVAS_ID");
    }

    #[test]
    fn test_widget_no_subcommand_returns_failure() {
        std::env::set_var("COLONY_API", "http://localhost:4000");
        std::env::set_var("COLONY_SESSION_TOKEN", "test-token");
        std::env::set_var("COLONY_CANVAS_ID", "test-canvas-id");

        let result = builtin_canvas(&["widget".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));

        std::env::remove_var("COLONY_API");
        std::env::remove_var("COLONY_SESSION_TOKEN");
        std::env::remove_var("COLONY_CANVAS_ID");
    }
}
