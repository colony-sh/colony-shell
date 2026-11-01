//! Draw builtin -- draw on the Bellman room's canvas.
//!
//! Each subcommand writes one canvas item through the local Bellman bridge door
//! (see [`crate::bellman_door`]); the room is `BELLMAN_ROOM`. Items are named by key,
//! `<kind>-<8 hex>` unless `--key` gives one, and the key is what `remove` and
//! `connector` take.
//!
//! # Subcommands
//!
//! | Subcommand | Bellman item |
//! |---|---|
//! | `sticky <text>` | `text` card |
//! | `text <label>` | `text` card |
//! | `rect [label]`, `ellipse [label]` | `shape` |
//! | `line --from X,Y --to X,Y` | `shape` of form `line` |
//! | `connector <from-key> <to-key>` | `connector` |
//! | `frame [label]` | `frame` |
//! | `list` | List the canvas items |
//! | `remove <key>` | Remove an item |
//! | `clear` | Remove every item this seat wrote |
//! | `help` | Print help text |

use serde_json::{json, Value};

use crate::bellman_door::{self, Door};
use crate::builtins::BuiltinResult;
use crate::canvas_builtin::{parse_pair, parse_size, widget_lines};
use crate::Result;

/// What a subcommand gives back: `Err` is the message to print after `draw: `.
type Outcome = std::result::Result<(), String>;

/// A shape's colours on Bellman: these names, or a hex `#rgb` / `#rrggbb`.
const COLORS: [&str; 6] = ["slate", "blue", "green", "amber", "red", "violet"];

/// Execute a `draw` builtin invocation.
pub fn builtin_draw(args: &[String]) -> Result<BuiltinResult> {
    let subcmd = args.first().map_or("help", String::as_str);
    let rest = args.get(1..).unwrap_or_default();

    let outcome = match subcmd {
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        "sticky" => card("sticky", rest),
        "text" => card("text", rest),
        "rect" | "ellipse" => shape(subcmd, rest),
        "line" => line(rest),
        "connector" => connector(rest),
        "frame" => frame(rest),
        "list" => Door::open()
            .and_then(|door| door.surface())
            .map(|surface| widget_lines(&surface).iter().for_each(|l| println!("{l}")))
            .map_err(String::from),
        "remove" => remove(rest),
        "clear" => clear(),
        other => Err(format!("unknown subcommand '{other}'. Try 'draw help'.")),
    };

    Ok(match outcome {
        Ok(()) => BuiltinResult::Success(0),
        Err(e) => {
            eprintln!("draw: {e}");
            BuiltinResult::Failure(1)
        }
    })
}

/// The options every subcommand shares, and the words left over.
#[derive(Debug, Default)]
struct DrawOpts {
    position: Option<(f64, f64)>,
    size: Option<(f64, f64)>,
    color: Option<String>,
    key: Option<String>,
    /// `line`'s ends.
    from: Option<(f64, f64)>,
    to: Option<(f64, f64)>,
    /// `connector`'s label.
    label: Option<String>,
    words: Vec<String>,
}

/// Parse `--position X,Y` (`--pos`, `-p`), `--size WxH` (`-s`), `--color C` (`-c`), `--key K`,
/// and `line`'s `--from X,Y` / `--to X,Y` and `connector`'s `--label L` (`-l`).
fn parse_draw_opts(args: &[String]) -> std::result::Result<DrawOpts, String> {
    let mut opts = DrawOpts::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--position" | "--pos" | "-p" => {
                let v = value()?;
                opts.position =
                    Some(parse_pair(v, ',').ok_or_else(|| format!("position '{v}' is not X,Y"))?);
            }
            "--size" | "-s" => opts.size = Some(parse_size(value()?)?),
            "--color" | "-c" => opts.color = Some(check_color(value()?)?),
            "--key" => opts.key = Some(value()?.clone()),
            "--from" | "--to" => {
                let v = value()?;
                let point = parse_pair(v, ',').ok_or_else(|| format!("point '{v}' is not X,Y"))?;
                if arg == "--from" {
                    opts.from = Some(point);
                } else {
                    opts.to = Some(point);
                }
            }
            "--label" | "-l" => opts.label = Some(value()?.clone()),
            _ => opts.words.push(arg.clone()),
        }
    }
    Ok(opts)
}

/// A colour Bellman takes, or why not.
fn check_color(color: &str) -> std::result::Result<String, String> {
    let hex = color
        .strip_prefix('#')
        .is_some_and(|h| matches!(h.len(), 3 | 6) && h.chars().all(|c| c.is_ascii_hexdigit()));
    if COLORS.contains(&color) || hex {
        Ok(color.to_string())
    } else {
        Err(format!(
            "colour '{color}' is not one of {} or a hex colour like #3b82f6",
            COLORS.join(", ")
        ))
    }
}

/// `{x, y, w, h}` from the options, with these defaults.
fn placement(opts: &DrawOpts, (x, y): (f64, f64), (w, h): (f64, f64)) -> Value {
    let (x, y) = opts.position.unwrap_or((x, y));
    let (w, h) = opts.size.unwrap_or((w, h));
    json!({"x": x, "y": y, "w": w, "h": h})
}

/// Write `item` under the options' key or a fresh one, and say so.
fn write(opts: &DrawOpts, prefix: &str, item: &Value) -> Outcome {
    let key = opts
        .key
        .clone()
        .unwrap_or_else(|| bellman_door::new_key(prefix));
    Door::open()?.put(&key, item)?;
    println!("Created {prefix}: {key}");
    Ok(())
}

/// `sticky <text>` and `text <label>`: a text card. Without `--position`, Bellman places it.
fn card(name: &str, args: &[String]) -> Outcome {
    let opts = parse_draw_opts(args)?;
    if opts.words.is_empty() {
        return Err(format!(
            "usage: draw {name} <text> [--position X,Y] [--size WxH] [--key K]"
        ));
    }
    if opts.color.is_some() {
        return Err("text cards have no colour on Bellman".to_string());
    }
    let mut item = json!({"kind": "text", "body": opts.words.join(" ")});
    match (opts.position, opts.size) {
        (Some(_), _) => item["placement"] = placement(&opts, (0.0, 0.0), (320.0, 200.0)),
        (None, Some(_)) => return Err("--size needs --position".to_string()),
        (None, None) => {}
    }
    write(&opts, name, &item)
}

/// `rect [label]` and `ellipse [label]`.
fn shape(form: &str, args: &[String]) -> Outcome {
    let opts = parse_draw_opts(args)?;
    let mut item = json!({
        "kind": "shape",
        "shape": {"form": form, "color": opts.color.as_deref().unwrap_or("slate")},
        "placement": placement(&opts, (100.0, 100.0), (200.0, 150.0)),
    });
    if !opts.words.is_empty() {
        item["title"] = json!(opts.words.join(" "));
    }
    write(&opts, form, &item)
}

/// `line --from X1,Y1 --to X2,Y2 [--color C] [--key K]`: a `line` shape over the box between the
/// two points, flipped when it runs from bottom-left to top-right.
fn line(args: &[String]) -> Outcome {
    let opts = parse_draw_opts(args)?;
    let (Some(from), Some(to)) = (opts.from, opts.to) else {
        return Err("usage: draw line --from X1,Y1 --to X2,Y2 [--color C] [--key K]".to_string());
    };
    write(&opts, "line", &line_item(from, to, opts.color.as_deref()))
}

/// The `line` shape between two points.
/// ponytail: a line's box keeps no direction, so an arrow form would need `flip` for both diagonals; add when draw gets arrows.
fn line_item((x1, y1): (f64, f64), (x2, y2): (f64, f64), color: Option<&str>) -> Value {
    json!({
        "kind": "shape",
        "shape": {
            "form": "line",
            "color": color.unwrap_or("slate"),
            "flip": (x2 - x1) * (y2 - y1) < 0.0,
        },
        // Bellman refuses a zero size, so a level or upright line gets one unit of height or width.
        "placement": {
            "x": x1.min(x2),
            "y": y1.min(y2),
            "w": (x2 - x1).abs().max(1.0),
            "h": (y2 - y1).abs().max(1.0),
        },
    })
}

/// `connector <from-key> <to-key> [--label L] [--key K]`.
fn connector(args: &[String]) -> Outcome {
    const USAGE: &str = "usage: draw connector <from-key> <to-key> [--label L] [--key K]";
    let opts = parse_draw_opts(args)?;
    let [from, to] = opts.words.as_slice() else {
        return Err(USAGE.to_string());
    };
    for end in [from, to] {
        if !bellman_door::is_key(end) {
            return Err(bellman_door::DoorError::BadKey(end.clone()).into());
        }
    }
    let mut item = json!({"kind": "connector", "ends": {"from": from, "to": to}});
    if let Some(label) = &opts.label {
        item["title"] = json!(label);
    }
    write(&opts, "connector", &item)
}

/// `frame [label]`: a frame needs a title, so an unlabelled one is "Frame".
fn frame(args: &[String]) -> Outcome {
    let opts = parse_draw_opts(args)?;
    if opts.color.is_some() {
        return Err("frames have no colour on Bellman".to_string());
    }
    let title = if opts.words.is_empty() {
        "Frame".to_string()
    } else {
        opts.words.join(" ")
    };
    let item = json!({
        "kind": "frame",
        "title": title,
        "placement": placement(&opts, (100.0, 100.0), (600.0, 400.0)),
    });
    write(&opts, "frame", &item)
}

fn remove(args: &[String]) -> Outcome {
    let key = args.first().ok_or("usage: draw remove <key>")?;
    Door::open()?.delete(key)?;
    println!("Removed {key}");
    Ok(())
}

/// `clear`: remove the items this bridge's seat wrote last. The canvas is the whole room's, so
/// other members' items stay.
fn clear() -> Outcome {
    let door = Door::open()?;
    let seat = door.seat()?;
    let keys = own_keys(&door.surface()?, &seat);
    for (done, key) in keys.iter().enumerate() {
        door.delete(key)
            .map_err(|e| format!("{e} (removed {done} of {} first)", keys.len()))?;
    }
    println!("Removed {} items this seat wrote.", keys.len());
    Ok(())
}

/// The keys of the items whose envelope names `seat` as their origin.
fn own_keys(surface: &Value, seat: &str) -> Vec<String> {
    surface["surface"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["origin"]["memberId"].as_str() == Some(seat))
        .filter_map(|i| i["data"]["key"].as_str().map(String::from))
        .collect()
}

fn print_help() {
    println!("Usage: draw <subcommand>");
    println!();
    println!("Subcommands:");
    println!("  sticky <text>                   Create a text card");
    println!("  text <label>                    Create a text card");
    println!("  rect [label]                    Draw a rectangle");
    println!("  ellipse [label]                 Draw an ellipse");
    println!("  line --from X,Y --to X,Y        Draw a line");
    println!("  connector <from-key> <to-key>   Connect two items [--label L]");
    println!("  frame [label]                   Draw a grouping frame");
    println!("  list                            List canvas items");
    println!("  remove <key>                    Remove an item");
    println!("  clear                           Remove every item this seat wrote");
    println!("  help                            This help");
    println!();
    println!("Common options:");
    println!("  --position X,Y  (or --pos, -p)  Position on canvas");
    println!("  --size WxH      (or -s)         Dimensions");
    println!(
        "  --color C       (or -c)         Shape colour: {} or #rrggbb",
        COLORS.join(", ")
    );
    println!("  --key K                         Item key (default <kind>-<8 hex>)");
    println!();
    println!("Writes go through the local Bellman bridge door to the room in BELLMAN_ROOM.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn test_help_returns_success() {
        let br = builtin_draw(&args(&["help"])).expect("should not error");
        assert_eq!(br, BuiltinResult::Success(0));
    }

    #[test]
    fn test_unknown_subcommand_returns_failure() {
        let br = builtin_draw(&args(&["bogus"])).expect("should not error");
        assert_eq!(br, BuiltinResult::Failure(1));
    }

    #[test]
    fn test_parse_draw_opts() {
        let opts = parse_draw_opts(&args(&[
            "hello", "-p", "50,75", "--size", "300x200", "-c", "#3B8", "--key", "k1", "world",
        ]))
        .unwrap();
        assert_eq!(opts.position, Some((50.0, 75.0)));
        assert_eq!(opts.size, Some((300.0, 200.0)));
        assert_eq!(opts.color.as_deref(), Some("#3B8"));
        assert_eq!(opts.key.as_deref(), Some("k1"));
        assert_eq!(opts.words, vec!["hello", "world"]);
        assert!(parse_draw_opts(&args(&["--color", "yellow"]))
            .unwrap_err()
            .contains("slate"));
        assert!(parse_draw_opts(&args(&["--size"]))
            .unwrap_err()
            .contains("needs a value"));
    }

    #[test]
    fn test_line_item_box_and_flip() {
        let down = line_item((10.0, 10.0), (110.0, 60.0), None);
        assert_eq!(
            down["placement"],
            json!({"x": 10.0, "y": 10.0, "w": 100.0, "h": 50.0})
        );
        assert_eq!(down["shape"]["flip"], json!(false));
        let up = line_item((10.0, 60.0), (110.0, 10.0), Some("red"));
        assert_eq!(up["placement"]["y"], json!(10.0));
        assert_eq!(
            up["shape"],
            json!({"form": "line", "color": "red", "flip": true})
        );
        let level = line_item((0.0, 5.0), (40.0, 5.0), None);
        assert_eq!(level["placement"]["h"], json!(1.0));
    }

    #[test]
    fn test_usage_errors_need_no_door() {
        assert!(card("text", &[]).is_err());
        assert!(card("sticky", &args(&["hi", "-c", "red"]))
            .unwrap_err()
            .contains("no colour"));
        assert!(line(&args(&["--from", "0,0"])).is_err());
        assert!(connector(&args(&["only-one"])).is_err());
        assert!(connector(&args(&["a", "Bad/Key"]))
            .unwrap_err()
            .contains("bad key"));
        assert!(remove(&[]).is_err());
    }

    #[test]
    fn test_own_keys_only_this_seat() {
        let surface = json!({"surface": {"items": [
            {"origin": {"memberId": "m_me"}, "data": {"key": "mine"}},
            {"origin": {"memberId": "m_other"}, "data": {"key": "theirs"}},
        ]}});
        assert_eq!(own_keys(&surface, "m_me"), vec!["mine"]);
    }
}
