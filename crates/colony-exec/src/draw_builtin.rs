//! Draw builtin -- create whiteboard items on the Colony canvas.
//!
//! The `draw` builtin creates visual primitives on the canvas whiteboard
//! layer: sticky notes, rectangles, ellipses, lines, connectors, text
//! labels, and frames.  Each subcommand maps to a POST to the whiteboard
//! items endpoint, and the response includes the created item's UUID.
//!
//! # Subcommands
//!
//! | Subcommand | Description |
//! |---|---|
//! | `sticky <text>` | Create a sticky note |
//! | `rect` | Draw a rectangle |
//! | `ellipse` | Draw an ellipse |
//! | `line` | Draw a line |
//! | `connector <from> <to>` | Draw a connector between widgets |
//! | `text <label>` | Place a text label |
//! | `frame` | Draw a grouping frame |
//! | `list` | List whiteboard items |
//! | `remove <id>` | Remove a whiteboard item |
//! | `clear` | Remove all whiteboard items |
//! | `help` | Print help text |

use std::process::Command;

use crate::builtins::BuiltinResult;
use crate::Result;

/// Configuration extracted from the environment for draw API calls.
struct DrawEnv {
    api: String,
    token: String,
    canvas_id: String,
}

/// Read the required environment variables or print an error and return `None`.
fn read_env() -> Option<DrawEnv> {
    let api = match std::env::var("COLONY_API") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("draw: not on a canvas (COLONY_API not set)");
            return None;
        }
    };
    let token = match std::env::var("COLONY_SESSION_TOKEN") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("draw: no auth token (COLONY_SESSION_TOKEN not set)");
            return None;
        }
    };
    let canvas_id = match std::env::var("COLONY_CANVAS_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("draw: no canvas ID (COLONY_CANVAS_ID not set)");
            return None;
        }
    };
    Some(DrawEnv {
        api,
        token,
        canvas_id,
    })
}

/// Execute a `draw` builtin invocation.
pub fn builtin_draw(args: &[String]) -> Result<BuiltinResult> {
    let subcmd = args.first().map(String::as_str).unwrap_or("help");

    // Help works off-canvas too
    if matches!(subcmd, "help" | "--help" | "-h") {
        print_help();
        return Ok(BuiltinResult::Success(0));
    }

    let Some(env) = read_env() else {
        return Ok(BuiltinResult::Failure(1));
    };

    let code = match subcmd {
        "sticky" => cmd_sticky(&env, &args[1..]),
        "rect" => cmd_shape(&env, "rect", &args[1..]),
        "ellipse" => cmd_shape(&env, "ellipse", &args[1..]),
        "line" => cmd_line(&env, &args[1..]),
        "connector" => cmd_connector(&env, &args[1..]),
        "text" => cmd_text(&env, &args[1..]),
        "frame" => cmd_frame(&env, &args[1..]),
        "list" => cmd_list(&env),
        "remove" => cmd_remove(&env, &args[1..]),
        "clear" => cmd_clear(&env),
        other => {
            eprintln!("draw: unknown subcommand '{other}'. Try 'draw help'.");
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
// HTTP helpers (same curl approach as canvas_builtin)
// ---------------------------------------------------------------------------

fn curl_get(api: &str, token: &str, path: &str) -> Option<String> {
    let url = format!("{api}{path}");
    let output = Command::new("curl")
        .args(["-s", "-H", &format!("Authorization: Bearer {token}"), &url])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

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
// Common argument parsing
// ---------------------------------------------------------------------------

/// Parse common draw options: --position X,Y --size WxH --color COLOR --z N
struct DrawOpts {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: String,
    z_index: i32,
}

impl Default for DrawOpts {
    fn default() -> Self {
        Self {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 150.0,
            color: "yellow".to_string(),
            z_index: 0,
        }
    }
}

fn parse_draw_opts(args: &[String], defaults: DrawOpts) -> (DrawOpts, Vec<String>) {
    let mut opts = defaults;
    let mut remaining = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--position" | "--pos" | "-p" => {
                i += 1;
                if let Some(pos) = args.get(i) {
                    if let Some((px, py)) = pos.split_once(',') {
                        opts.x = px.parse().unwrap_or(opts.x);
                        opts.y = py.parse().unwrap_or(opts.y);
                    }
                }
            }
            "--size" | "-s" => {
                i += 1;
                if let Some(size) = args.get(i) {
                    if let Some((sw, sh)) = size.split_once('x') {
                        opts.width = sw.parse().unwrap_or(opts.width);
                        opts.height = sh.parse().unwrap_or(opts.height);
                    }
                }
            }
            "--color" | "-c" => {
                i += 1;
                if let Some(c) = args.get(i) {
                    opts.color = c.clone();
                }
            }
            "--z" => {
                i += 1;
                if let Some(z) = args.get(i) {
                    opts.z_index = z.parse().unwrap_or(0);
                }
            }
            _ => {
                remaining.push(args[i].clone());
            }
        }
        i += 1;
    }

    (opts, remaining)
}

/// Build the JSON body for creating a whiteboard item.
fn build_item_body(item_type: &str, properties: &serde_json::Value, z_index: i32) -> String {
    let body = serde_json::json!({
        "item_type": item_type,
        "properties": properties,
        "z_index": z_index,
    });
    body.to_string()
}

/// Post an item to the whiteboard and print the result.
fn post_item(env: &DrawEnv, item_type: &str, body: &str) -> i32 {
    let path = format!("/api/canvases/{}/whiteboard", env.canvas_id);
    let Some(resp) = curl_post(&env.api, &env.token, &path, body) else {
        eprintln!("draw: failed to create {item_type}");
        return 1;
    };

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resp) {
        let data = if v.get("data").is_some() {
            &v["data"]
        } else {
            &v
        };
        let id = data["id"].as_str().unwrap_or("?");
        println!("Created {item_type}: {id}");
    }

    0
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

fn cmd_sticky(env: &DrawEnv, args: &[String]) -> i32 {
    let defaults = DrawOpts {
        width: 200.0,
        height: 200.0,
        color: "yellow".to_string(),
        ..DrawOpts::default()
    };
    let (opts, remaining) = parse_draw_opts(args, defaults);
    let text = if remaining.is_empty() {
        String::new()
    } else {
        remaining.join(" ")
    };

    let props = serde_json::json!({
        "x": opts.x,
        "y": opts.y,
        "width": opts.width,
        "height": opts.height,
        "color": opts.color,
        "text": text,
    });

    let body = build_item_body("sticky", &props, opts.z_index);
    post_item(env, "sticky", &body)
}

fn cmd_shape(env: &DrawEnv, shape: &str, args: &[String]) -> i32 {
    let (opts, _) = parse_draw_opts(args, DrawOpts::default());

    let props = serde_json::json!({
        "x": opts.x,
        "y": opts.y,
        "width": opts.width,
        "height": opts.height,
        "color": opts.color,
    });

    let body = build_item_body(shape, &props, opts.z_index);
    post_item(env, shape, &body)
}

fn cmd_line(env: &DrawEnv, args: &[String]) -> i32 {
    // draw line --from X1,Y1 --to X2,Y2 [--color C] [--z N]
    let mut x1 = 0.0_f64;
    let mut y1 = 0.0_f64;
    let mut x2 = 100.0_f64;
    let mut y2 = 100.0_f64;
    let mut color = "black".to_string();
    let mut z_index = 0;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => {
                i += 1;
                if let Some(pos) = args.get(i) {
                    if let Some((px, py)) = pos.split_once(',') {
                        x1 = px.parse().unwrap_or(0.0);
                        y1 = py.parse().unwrap_or(0.0);
                    }
                }
            }
            "--to" => {
                i += 1;
                if let Some(pos) = args.get(i) {
                    if let Some((px, py)) = pos.split_once(',') {
                        x2 = px.parse().unwrap_or(100.0);
                        y2 = py.parse().unwrap_or(100.0);
                    }
                }
            }
            "--color" | "-c" => {
                i += 1;
                if let Some(c) = args.get(i) {
                    color = c.clone();
                }
            }
            "--z" => {
                i += 1;
                if let Some(z) = args.get(i) {
                    z_index = z.parse().unwrap_or(0);
                }
            }
            _ => {}
        }
        i += 1;
    }

    let props = serde_json::json!({
        "x1": x1, "y1": y1,
        "x2": x2, "y2": y2,
        "color": color,
    });

    let body = build_item_body("line", &props, z_index);
    post_item(env, "line", &body)
}

fn cmd_connector(env: &DrawEnv, args: &[String]) -> i32 {
    // draw connector <from-widget-id> <to-widget-id> [--color C] [--label L] [--z N]
    let from_id = match args.first() {
        Some(id) => id.clone(),
        None => {
            eprintln!(
                "Usage: draw connector <from-widget-id> <to-widget-id> [--color C] [--label L]"
            );
            return 1;
        }
    };
    let to_id = match args.get(1) {
        Some(id) => id.clone(),
        None => {
            eprintln!(
                "Usage: draw connector <from-widget-id> <to-widget-id> [--color C] [--label L]"
            );
            return 1;
        }
    };

    let mut color = "black".to_string();
    let mut label = String::new();
    let mut z_index = 0;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--color" | "-c" => {
                i += 1;
                if let Some(c) = args.get(i) {
                    color = c.clone();
                }
            }
            "--label" | "-l" => {
                i += 1;
                if let Some(l) = args.get(i) {
                    label = l.clone();
                }
            }
            "--z" => {
                i += 1;
                if let Some(z) = args.get(i) {
                    z_index = z.parse().unwrap_or(0);
                }
            }
            _ => {}
        }
        i += 1;
    }

    let props = serde_json::json!({
        "from_id": from_id,
        "to_id": to_id,
        "color": color,
        "label": label,
    });

    let body = build_item_body("connector", &props, z_index);
    post_item(env, "connector", &body)
}

fn cmd_text(env: &DrawEnv, args: &[String]) -> i32 {
    let defaults = DrawOpts {
        width: 300.0,
        height: 50.0,
        color: "black".to_string(),
        ..DrawOpts::default()
    };
    let (opts, remaining) = parse_draw_opts(args, defaults);
    let text = if remaining.is_empty() {
        eprintln!("Usage: draw text <label> [--position X,Y] [--color C]");
        return 1;
    } else {
        remaining.join(" ")
    };

    let props = serde_json::json!({
        "x": opts.x,
        "y": opts.y,
        "width": opts.width,
        "height": opts.height,
        "color": opts.color,
        "text": text,
    });

    let body = build_item_body("text", &props, opts.z_index);
    post_item(env, "text", &body)
}

fn cmd_frame(env: &DrawEnv, args: &[String]) -> i32 {
    let defaults = DrawOpts {
        width: 600.0,
        height: 400.0,
        color: "gray".to_string(),
        ..DrawOpts::default()
    };
    let (opts, remaining) = parse_draw_opts(args, defaults);
    let label = remaining.join(" ");

    let props = serde_json::json!({
        "x": opts.x,
        "y": opts.y,
        "width": opts.width,
        "height": opts.height,
        "color": opts.color,
        "label": label,
    });

    let body = build_item_body("frame", &props, opts.z_index);
    post_item(env, "frame", &body)
}

fn cmd_list(env: &DrawEnv) -> i32 {
    let path = format!("/api/canvases/{}/whiteboard", env.canvas_id);
    let Some(json) = curl_get(&env.api, &env.token, &path) else {
        eprintln!("draw: failed to list whiteboard items");
        return 1;
    };

    let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else {
        println!("{json}");
        return 0;
    };

    let items = if let Some(arr) = v["data"].as_array() {
        arr
    } else if let Some(arr) = v.as_array() {
        arr
    } else {
        println!("{json}");
        return 0;
    };

    println!(
        "{:<10} {:<12} {:<14} {:<12} Z",
        "ID", "TYPE", "POSITION", "SIZE"
    );

    for item in items {
        let id = item["id"].as_str().unwrap_or("?");
        let item_type = item["item_type"].as_str().unwrap_or("?");
        let props = &item["properties"];
        let x = props["x"].as_f64().unwrap_or(0.0);
        let y = props["y"].as_f64().unwrap_or(0.0);
        let w = props["width"].as_f64().unwrap_or(0.0);
        let h = props["height"].as_f64().unwrap_or(0.0);
        let z = item["z_index"].as_i64().unwrap_or(0);
        let id_short = &id[..8.min(id.len())];
        println!(
            "{id_short:<10} {item_type:<12} {x:.0},{y:.0}{:<6} {w:.0}x{h:.0}{:<4} {z}",
            "", ""
        );
    }

    0
}

fn cmd_remove(env: &DrawEnv, args: &[String]) -> i32 {
    let Some(id) = args.first() else {
        eprintln!("Usage: draw remove <item-id>");
        return 1;
    };
    let path = format!("/api/canvases/{}/whiteboard/{id}", env.canvas_id);
    curl_delete(&env.api, &env.token, &path);
    println!("Removed whiteboard item {id}");
    0
}

fn cmd_clear(env: &DrawEnv) -> i32 {
    let path = format!("/api/canvases/{}/whiteboard", env.canvas_id);
    curl_delete(&env.api, &env.token, &path);
    println!("Cleared all whiteboard items");
    0
}

fn print_help() {
    println!("Usage: draw <subcommand>");
    println!();
    println!("Subcommands:");
    println!("  sticky <text>                   Create a sticky note");
    println!("  rect                            Draw a rectangle");
    println!("  ellipse                         Draw an ellipse");
    println!("  line --from X,Y --to X,Y        Draw a line");
    println!("  connector <from-id> <to-id>     Connect two widgets");
    println!("  text <label>                    Place a text label");
    println!("  frame [label]                   Draw a grouping frame");
    println!("  list                            List whiteboard items");
    println!("  remove <id>                     Remove item");
    println!("  clear                           Remove all items");
    println!("  help                            This help");
    println!();
    println!("Common options:");
    println!("  --position X,Y  (or --pos, -p)  Position on canvas");
    println!("  --size WxH      (or -s)          Dimensions");
    println!("  --color COLOR   (or -c)          Color name");
    println!("  --z N                             Z-index layer");
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
        let result = builtin_draw(&["help".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Success(0));
    }

    #[test]
    fn test_unknown_subcommand_returns_failure() {
        std::env::set_var("COLONY_API", "http://localhost:4000");
        std::env::set_var("COLONY_SESSION_TOKEN", "test-token");
        std::env::set_var("COLONY_CANVAS_ID", "test-canvas-id");

        let result = builtin_draw(&["bogus".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));

        std::env::remove_var("COLONY_API");
        std::env::remove_var("COLONY_SESSION_TOKEN");
        std::env::remove_var("COLONY_CANVAS_ID");
    }

    #[test]
    fn test_missing_env_returns_failure() {
        std::env::remove_var("COLONY_API");
        std::env::remove_var("COLONY_SESSION_TOKEN");
        std::env::remove_var("COLONY_CANVAS_ID");

        let result = builtin_draw(&["sticky".to_string(), "hello".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));
    }

    #[test]
    fn test_parse_draw_opts_defaults() {
        let args: Vec<String> = vec!["hello".to_string(), "world".to_string()];
        let (opts, remaining) = parse_draw_opts(&args, DrawOpts::default());
        assert!((opts.x - 100.0).abs() < f64::EPSILON);
        assert!((opts.y - 100.0).abs() < f64::EPSILON);
        assert_eq!(remaining, vec!["hello", "world"]);
    }

    #[test]
    fn test_parse_draw_opts_with_position_and_color() {
        let args: Vec<String> = vec![
            "--position".to_string(),
            "50,75".to_string(),
            "--color".to_string(),
            "red".to_string(),
            "label".to_string(),
        ];
        let (opts, remaining) = parse_draw_opts(&args, DrawOpts::default());
        assert!((opts.x - 50.0).abs() < f64::EPSILON);
        assert!((opts.y - 75.0).abs() < f64::EPSILON);
        assert_eq!(opts.color, "red");
        assert_eq!(remaining, vec!["label"]);
    }

    #[test]
    fn test_parse_draw_opts_with_size() {
        let args: Vec<String> = vec!["--size".to_string(), "300x250".to_string()];
        let (opts, _) = parse_draw_opts(&args, DrawOpts::default());
        assert!((opts.width - 300.0).abs() < f64::EPSILON);
        assert!((opts.height - 250.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_connector_no_args_returns_failure() {
        std::env::set_var("COLONY_API", "http://localhost:4000");
        std::env::set_var("COLONY_SESSION_TOKEN", "test-token");
        std::env::set_var("COLONY_CANVAS_ID", "test-canvas-id");

        let result = builtin_draw(&["connector".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));

        std::env::remove_var("COLONY_API");
        std::env::remove_var("COLONY_SESSION_TOKEN");
        std::env::remove_var("COLONY_CANVAS_ID");
    }

    #[test]
    fn test_text_no_args_returns_failure() {
        std::env::set_var("COLONY_API", "http://localhost:4000");
        std::env::set_var("COLONY_SESSION_TOKEN", "test-token");
        std::env::set_var("COLONY_CANVAS_ID", "test-canvas-id");

        let result = builtin_draw(&["text".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));

        std::env::remove_var("COLONY_API");
        std::env::remove_var("COLONY_SESSION_TOKEN");
        std::env::remove_var("COLONY_CANVAS_ID");
    }

    #[test]
    fn test_remove_no_args_returns_failure() {
        std::env::set_var("COLONY_API", "http://localhost:4000");
        std::env::set_var("COLONY_SESSION_TOKEN", "test-token");
        std::env::set_var("COLONY_CANVAS_ID", "test-canvas-id");

        let result = builtin_draw(&["remove".to_string()]);
        let br = result.expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));

        std::env::remove_var("COLONY_API");
        std::env::remove_var("COLONY_SESSION_TOKEN");
        std::env::remove_var("COLONY_CANVAS_ID");
    }
}
