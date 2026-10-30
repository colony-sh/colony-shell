//! Canvas builtin -- read and write the Bellman room's canvas from the shell.
//!
//! Everything but `tasks` goes through the local Bellman bridge door (see
//! [`crate::bellman_door`]): the room is `BELLMAN_ROOM`, and the terminal holds no
//! token. `tasks` still reads the Colony API (`COLONY_API`, `COLONY_SESSION_TOKEN`,
//! `COLONY_CANVAS_ID`) via `curl`: Bellman has no task board.
//!
//! # Subcommands
//!
//! | Subcommand | Description |
//! |---|---|
//! | `status` | Show the room and how many canvas items it has |
//! | `participants` | List the room's humans and agents |
//! | `widgets` | List the canvas items |
//! | `announce <text>` | Post this seat's status card |
//! | `widget create\|move\|resize\|remove\|config` | Canvas item CRUD |
//! | `tasks` | Read the Colony task board |
//! | `markdown <key>` | Print an item's body |
//! | `help` | Print help text |

use std::process::Command;

use serde_json::{json, Map, Value};

use crate::bellman_door::{self, Door};
use crate::builtins::BuiltinResult;
use crate::Result;

/// What a subcommand gives back: `Err` is the message to print after `canvas: `.
type Outcome = std::result::Result<(), String>;

/// The kinds `canvas widget create` makes: the card kinds that carry a body.
const CARD_KINDS: [&str; 4] = ["text", "link", "diagram", "html"];

/// The item fields a write may carry back (`SurfaceItemShape` in bellman `src/surface.ts`,
/// less `key`, which the path gives, and `frame`, which is never stored). The schema is strict,
/// so what a read adds (`cursor`, `at`, `approved_by`, ...) must not go back.
const WRITABLE: [&str; 8] = [
    "kind",
    "title",
    "body",
    "ends",
    "placement",
    "blob",
    "shape",
    "locked",
];

/// Execute a `canvas` builtin invocation.
pub fn builtin_canvas(args: &[String]) -> Result<BuiltinResult> {
    let subcmd = args.first().map_or("status", String::as_str);
    let rest = args.get(1..).unwrap_or_default();

    let outcome = match subcmd {
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        "status" | "participants" | "widgets" => door_read(subcmd),
        "announce" => announce(rest),
        "widget" => widget(rest),
        "markdown" => markdown(rest),
        "tasks" => tasks(),
        other => Err(format!("unknown subcommand '{other}'. Try 'canvas help'.")),
    };

    Ok(match outcome {
        Ok(()) => BuiltinResult::Success(0),
        Err(e) => {
            eprintln!("canvas: {e}");
            BuiltinResult::Failure(1)
        }
    })
}

/// Print `status`, `participants` or `widgets` for `BELLMAN_ROOM`, read off the door.
fn door_read(subcmd: &str) -> Outcome {
    let door = Door::open()?;
    let lines = match subcmd {
        "participants" => participant_lines(&door.rooms()?, door.room())
            .ok_or_else(|| bellman_door::DoorError::NoSeat(door.room().to_string()))?,
        "widgets" => widget_lines(&door.surface()?),
        _ => status_lines(&door.surface()?, door.room()),
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

/// `GET /rooms/<id>/surface` as the room's name and how many items its canvas holds.
pub(crate) fn status_lines(surface: &Value, room: &str) -> Vec<String> {
    let name = surface["room"]["text"]["data"]["room"]
        .as_str()
        .map_or_else(|| "unnamed".to_string(), |n| bellman_door::clean(n, 80));
    let items = surface["surface"]["items"].as_array().map_or(0, Vec::len);
    vec![format!("Room: {name} ({room})"), format!("Items: {items}")]
}

/// `GET /rooms` as one line per active member of `room`, or `None` when the bridge holds no seat there.
pub(crate) fn participant_lines(rooms: &Value, room: &str) -> Option<Vec<String>> {
    let found = rooms["rooms"]
        .as_array()?
        .iter()
        .find(|r| r["session_id"].as_str() == Some(room))?;
    let text = |v: &Value, max| {
        v.as_str()
            .map_or_else(|| "\u{2014}".to_string(), |s| bellman_door::clean(s, max))
    };
    let lines = found["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["active"].as_bool() != Some(false))
        .map(|m| {
            let kind = if m["agent"].is_null() {
                "HUMAN"
            } else {
                "AGENT"
            };
            let role = text(&m["room_role"], 10);
            let status = text(&m["status"], 10);
            let label = text(&m["label"], 60);
            let id = text(&m["member_id"], 20);
            format!("{kind:<6} {role:<10} {status:<10} {label} ({id})")
        })
        .collect();
    Some(lines)
}

/// `GET /rooms/<id>/surface` as a table of the canvas items, each item's `data` out of its envelope.
pub(crate) fn widget_lines(surface: &Value) -> Vec<String> {
    let header = format!(
        "{:<24} {:<10} {:<12} {:<10} TITLE",
        "KEY", "KIND", "POSITION", "SIZE"
    );
    let rows = surface["surface"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            let d = &item["data"];
            let key = bellman_door::clean(d["key"].as_str().unwrap_or("?"), 24);
            let kind = bellman_door::clean(d["kind"].as_str().unwrap_or("?"), 10);
            let title = bellman_door::clean(d["title"].as_str().unwrap_or(""), 60);
            let p = &d["placement"];
            let pos = match (p["x"].as_f64(), p["y"].as_f64()) {
                (Some(x), Some(y)) => format!("{x:.0},{y:.0}"),
                _ => "\u{2014}".to_string(),
            };
            let size = match (p["w"].as_f64(), p["h"].as_f64()) {
                (Some(w), Some(h)) => format!("{w:.0}x{h:.0}"),
                _ => "\u{2014}".to_string(),
            };
            format!("{key:<24} {kind:<10} {pos:<12} {size:<10} {title}")
        });
    std::iter::once(header).chain(rows).collect()
}

/// `announce <text> [--files f...]` writes this seat's status card, `announce-<member id>`;
/// `announce --clear` removes it.
fn announce(args: &[String]) -> Outcome {
    const USAGE: &str = "usage: canvas announce <status text> [--files f1 f2 ...] | --clear";
    let Some(first) = args.first() else {
        return Err(USAGE.to_string());
    };
    let door = Door::open()?;
    let seat = door.seat()?;
    let key = announce_key(&seat);

    if first == "--clear" {
        match door.item(&key) {
            Err(bellman_door::DoorError::NoItem(_)) => println!("Nothing to clear."),
            Err(e) => return Err(e.into()),
            Ok(_) => {
                door.delete(&key)?;
                println!("Status cleared.");
            }
        }
        return Ok(());
    }

    let (detail, files) = split_files(args);
    if detail.is_empty() && files.is_empty() {
        return Err(USAGE.to_string());
    }
    let mut text = detail.clone();
    if !files.is_empty() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&format!("Files: {}", files.join(", ")));
    }
    door.put(
        &key,
        &json!({"kind": "text", "title": "Working on", "body": text}),
    )?;
    println!(
        "Announced: {}",
        if detail.is_empty() { &text } else { &detail }
    );
    Ok(())
}

/// The status card's key for a seat: `announce-` and the member id, as a valid key.
fn announce_key(member_id: &str) -> String {
    let id: String = member_id
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(22)
        .collect();
    format!("announce-{id}")
}

/// `announce`'s words, and the names after `--files` (up to the next `--` flag).
fn split_files(args: &[String]) -> (String, Vec<String>) {
    let mut words = Vec::new();
    let mut files = Vec::new();
    let mut in_files = false;
    for arg in args {
        if arg == "--files" {
            in_files = true;
        } else if in_files && !arg.starts_with("--") {
            files.push(arg.clone());
        } else {
            in_files = false;
            words.push(arg.as_str());
        }
    }
    (words.join(" "), files)
}

fn widget(args: &[String]) -> Outcome {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("create") => widget_create(rest),
        Some("move") => widget_place(rest, "move", "<x,y>"),
        Some("resize") => widget_place(rest, "resize", "<WxH>"),
        Some("remove") => {
            let key = rest.first().ok_or("usage: canvas widget remove <key>")?;
            Door::open()?.delete(key)?;
            println!("Removed item {key}");
            Ok(())
        }
        Some("config") => widget_config(rest),
        _ => Err("usage: canvas widget <create|move|resize|remove|config> ...".to_string()),
    }
}

/// `widget create <kind> --body B [--title T] [--key K] [--position X,Y [--size WxH]]`.
fn widget_create(args: &[String]) -> Outcome {
    const USAGE: &str = "usage: canvas widget create <text|link|diagram|html> --body B [--title T] [--key K] [--position X,Y] [--size WxH]";
    let kind = args.first().ok_or(USAGE)?;
    if !CARD_KINDS.contains(&kind.as_str()) {
        return Err(format!(
            "canvas widget create makes text, link, diagram or html items, not '{kind}'; use draw for shapes, frames and connectors"
        ));
    }
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let body = opt("--body").ok_or_else(|| format!("a {kind} item needs --body"))?;
    let key = opt("--key").unwrap_or_else(|| bellman_door::new_key(kind));

    let mut item = json!({"kind": kind, "body": body});
    if let Some(title) = opt("--title") {
        item["title"] = json!(title);
    }
    match (opt("--position"), opt("--size")) {
        (Some(pos), size) => {
            let (x, y) = parse_pair(&pos, ',').ok_or("--position takes X,Y")?;
            let mut placement = json!({"x": x, "y": y});
            if let Some(size) = size {
                let (w, h) = parse_size(&size)?;
                placement["w"] = json!(w);
                placement["h"] = json!(h);
            }
            item["placement"] = placement;
        }
        (None, Some(_)) => return Err("--size needs --position".to_string()),
        (None, None) => {} // Bellman places it in the next free grid slot.
    }

    Door::open()?.put(&key, &item)?;
    println!("Created {kind} item: {key}");
    Ok(())
}

/// `widget move <key> <x,y>` and `widget resize <key> <WxH>`: the item rewritten with a new placement.
fn widget_place(args: &[String], verb: &str, what: &str) -> Outcome {
    let usage = || format!("usage: canvas widget {verb} <key> {what}");
    let (Some(key), Some(value)) = (args.first(), args.get(1)) else {
        return Err(usage());
    };
    let door = Door::open()?;
    let mut item = writable(&door.item(key)?);
    let mut placement = item
        .remove("placement")
        .and_then(|p| p.as_object().cloned())
        .unwrap_or_default();
    if verb == "move" {
        let (x, y) = parse_pair(value, ',').ok_or_else(usage)?;
        placement.insert("x".into(), json!(x));
        placement.insert("y".into(), json!(y));
    } else {
        if !placement.contains_key("x") {
            return Err(format!("item {key} has no position yet; move it first"));
        }
        let (w, h) = parse_size(value)?;
        placement.insert("w".into(), json!(w));
        placement.insert("h".into(), json!(h));
    }
    item.insert("placement".into(), Value::Object(placement));
    door.put(key, &Value::Object(item))?;
    println!(
        "{} {key} to {value}",
        if verb == "move" { "Moved" } else { "Resized" }
    );
    Ok(())
}

/// `widget config <key>` prints the item's writable fields; `widget config <key> <json>` merges
/// a JSON object into them (a `null` field removes it) and writes the item back.
fn widget_config(args: &[String]) -> Outcome {
    let key = args
        .first()
        .ok_or("usage: canvas widget config <key> [json]")?;
    let door = Door::open()?;
    let item = writable(&door.item(key)?);
    let Some(patch) = args.get(1) else {
        let pretty = serde_json::to_string_pretty(&item).map_err(|e| e.to_string())?;
        println!("{}", bellman_door::clean_block(&pretty));
        return Ok(());
    };
    let patch: Value =
        serde_json::from_str(patch).map_err(|e| format!("config is not JSON: {e}"))?;
    let merged = merge(item, &patch).ok_or("config must be a JSON object")?;
    door.put(key, &Value::Object(merged))?;
    println!("Updated {key}");
    Ok(())
}

/// The fields of a read item that a write may send back, `null`s left out.
fn writable(data: &Value) -> Map<String, Value> {
    WRITABLE
        .iter()
        .filter_map(|&k| {
            data.get(k)
                .filter(|v| !v.is_null())
                .map(|v| (k.to_string(), v.clone()))
        })
        .collect()
}

/// `patch`'s fields over `item`: a `null` removes the field, and `key` is left to the path.
fn merge(mut item: Map<String, Value>, patch: &Value) -> Option<Map<String, Value>> {
    for (k, v) in patch.as_object()? {
        if k == "key" {
            continue;
        }
        if v.is_null() {
            item.remove(k);
        } else {
            item.insert(k.clone(), v.clone());
        }
    }
    Some(item)
}

/// `markdown <key>`: the item's body, safe to print.
fn markdown(args: &[String]) -> Outcome {
    let key = args.first().ok_or("usage: canvas markdown <key>")?;
    let item = Door::open()?.item(key)?;
    let body = item["body"]
        .as_str()
        .ok_or_else(|| format!("item {key} has no body"))?;
    println!("{}", bellman_door::clean_block(body));
    Ok(())
}

/// `X<sep>Y` as two numbers.
pub(crate) fn parse_pair(text: &str, sep: char) -> Option<(f64, f64)> {
    let (a, b) = text.split_once(sep)?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// `WxH`, both positive (Bellman refuses a zero size).
pub(crate) fn parse_size(text: &str) -> std::result::Result<(f64, f64), String> {
    parse_pair(text, 'x')
        .filter(|&(w, h)| w > 0.0 && h > 0.0)
        .ok_or_else(|| format!("size '{text}' is not WxH with both above zero"))
}

/// `tasks`: the Colony task board, still on the Colony API (Bellman has none).
fn tasks() -> Outcome {
    let var = |name: &str| {
        std::env::var(name).map_err(|_| format!("{name} not set (tasks still read the Colony API)"))
    };
    let api = var("COLONY_API")?;
    let token = var("COLONY_SESSION_TOKEN")?;
    let canvas_id = var("COLONY_CANVAS_ID")?;
    let output = Command::new("curl")
        .args(["-s", "-f", "-H", &format!("Authorization: Bearer {token}")])
        .arg(format!("{api}/api/canvases/{canvas_id}/tasks"))
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;
    if !output.status.success() {
        return Err("failed to fetch tasks".to_string());
    }
    println!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

fn print_help() {
    println!("Usage: canvas <subcommand>");
    println!();
    println!("Subcommands:");
    println!("  status                          Show the room and its item count");
    println!("  participants                    List humans + agents");
    println!("  widgets                         List canvas items");
    println!("  announce <text>                 Post this seat's status card");
    println!("  announce --files f1 f2          Announce with file list");
    println!("  announce --clear                Remove the status card");
    println!("  widget create <kind> --body B   Create a text, link, diagram or html item");
    println!("         [--title T] [--key K] [--position X,Y] [--size WxH]");
    println!("  widget move <key> <x,y>         Move an item");
    println!("  widget resize <key> <WxH>       Resize an item");
    println!("  widget remove <key>             Remove an item");
    println!("  widget config <key> [json]      Print an item, or merge JSON into it");
    println!("  tasks                           Read the Colony task board");
    println!("  markdown <key>                  Print an item's body");
    println!("  help                            This help");
    println!();
    println!("Everything but tasks goes through the local Bellman bridge door.");
    println!();
    println!("Environment:");
    println!("  BELLMAN_ROOM             Bellman room (session) id");
    println!("  BELLMAN_DOOR_ROOT        Bridge door folder (default ~/.claude/bellman/door)");
    println!("  COLONY_API, COLONY_SESSION_TOKEN, COLONY_CANVAS_ID   for tasks only");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn test_help_returns_success() {
        let br = builtin_canvas(&args(&["help"])).expect("should not error");
        assert_eq!(br, BuiltinResult::Success(0));
    }

    #[test]
    fn test_unknown_subcommand_returns_failure() {
        let br = builtin_canvas(&args(&["bogus"])).expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));
    }

    #[test]
    fn test_usage_errors_need_no_door() {
        assert!(announce(&[]).is_err());
        assert!(widget(&[]).is_err());
        assert!(widget_create(&args(&["shape"]))
            .unwrap_err()
            .contains("use draw"));
        assert!(widget_create(&args(&["text"]))
            .unwrap_err()
            .contains("needs --body"));
        assert!(widget_place(&args(&["k"]), "move", "<x,y>").is_err());
        assert!(markdown(&[]).is_err());
    }

    #[test]
    fn test_announce_key_and_files() {
        assert_eq!(announce_key("m_9A5fe64f"), "announce-m_9a5fe64f");
        assert!(bellman_door::is_key(&announce_key(&"m_x".repeat(20))));
        let (detail, files) = split_files(&args(&[
            "fixing", "--files", "a.rs", "b.rs", "--x", "parser",
        ]));
        assert_eq!(detail, "fixing --x parser");
        assert_eq!(files, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn test_writable_drops_read_only_and_null_fields() {
        let data = json!({"key": "k", "kind": "text", "body": "b", "title": null, "cursor": 7,
            "at": "t", "approved_by": null, "placement": {"x": 1, "y": 2}});
        let item = writable(&data);
        assert_eq!(
            Value::Object(item),
            json!({"kind": "text", "body": "b", "placement": {"x": 1, "y": 2}})
        );
    }

    #[test]
    fn test_merge_sets_removes_and_ignores_key() {
        let item = writable(&json!({"kind": "text", "body": "b", "title": "t"}));
        let merged = merge(item, &json!({"body": "new", "title": null, "key": "other"})).unwrap();
        assert_eq!(
            Value::Object(merged),
            json!({"kind": "text", "body": "new"})
        );
        assert_eq!(merge(Map::new(), &json!([1])), None);
    }

    #[test]
    fn test_parse_size_refuses_zero() {
        assert_eq!(parse_size("300x200"), Ok((300.0, 200.0)));
        assert!(parse_size("0x200").is_err());
        assert!(parse_size("300").is_err());
    }

    fn surface() -> Value {
        serde_json::json!({
            "session_id": "qs_room",
            "room": {"text": {"trust": "untrusted", "data": {"room": "planning\u{1b}[2J"}}},
            "surface": {"cursor": 3, "items": [
                {"trust": "untrusted", "data": {"key": "plan", "kind": "text", "title": "The plan",
                    "placement": {"x": 720, "y": 240, "w": 420, "h": 1000}}},
                {"trust": "untrusted", "data": {"key": "note", "kind": "html", "title": "Loose",
                    "placement": {"x": -97, "y": 0}}}
            ]}
        })
    }

    #[test]
    fn test_status_lines_name_and_count() {
        assert_eq!(
            status_lines(&surface(), "qs_room"),
            vec!["Room: planning[2J (qs_room)", "Items: 2"]
        );
    }

    #[test]
    fn test_widget_lines_table() {
        let lines = widget_lines(&surface());
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("KEY"));
        assert_eq!(
            lines[1],
            format!(
                "{:<24} {:<10} {:<12} {:<10} The plan",
                "plan", "text", "720,240", "420x1000"
            )
        );
        assert!(lines[2].contains("-97,0") && lines[2].contains('\u{2014}'));
    }

    #[test]
    fn test_participant_lines_active_members_of_the_room() {
        let rooms = serde_json::json!({"rooms": [
            {"session_id": "qs_other", "members": []},
            {"session_id": "qs_room", "members": [
                {"member_id": "m_1", "label": "ada", "agent": null, "room_role": "planner", "status": "idle", "active": true},
                {"member_id": "m_2", "label": "bot", "agent": {"model": "x"}, "room_role": "coder", "status": "working", "active": true},
                {"member_id": "m_3", "label": "gone", "agent": null, "room_role": "coder", "active": false}
            ]}
        ]});
        let lines = participant_lines(&rooms, "qs_room").expect("room is listed");
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            format!("{:<6} {:<10} {:<10} ada (m_1)", "HUMAN", "planner", "idle")
        );
        assert!(lines[1].starts_with("AGENT  coder"));
        assert_eq!(participant_lines(&rooms, "qs_missing"), None);
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
