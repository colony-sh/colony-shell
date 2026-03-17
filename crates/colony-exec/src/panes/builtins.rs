//! Pane management builtin commands
//!
//! Each command emits an OSC 2337 Pane message to the terminal.

use crate::builtins::BuiltinResult;
use crate::Result;
use colony_protocol::{
    emit_pane_close, emit_pane_focus, emit_pane_layout, emit_pane_split, PaneFocusDirection,
    PaneSplitDirection,
};

use super::layout::PaneLayout;

/// `split [--horizontal|--vertical]` — tell terminal to split current pane
///
/// Defaults to horizontal split if no direction specified.
pub fn builtin_split(args: &[String]) -> Result<BuiltinResult> {
    let mut direction = PaneSplitDirection::Horizontal;
    let mut ratio = 0.5;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--horizontal" | "-h" => direction = PaneSplitDirection::Horizontal,
            "--vertical" | "-v" => direction = PaneSplitDirection::Vertical,
            "--ratio" | "-r" => {
                i += 1;
                let Some(r) = args.get(i) else {
                    eprintln!("split: --ratio requires a value (0.0-1.0)");
                    return Ok(BuiltinResult::Failure(1));
                };
                let Ok(parsed) = r.parse::<f64>() else {
                    eprintln!("split: invalid ratio '{r}'");
                    return Ok(BuiltinResult::Failure(1));
                };
                if !(0.0..=1.0).contains(&parsed) {
                    eprintln!("split: ratio must be between 0.0 and 1.0");
                    return Ok(BuiltinResult::Failure(1));
                }
                ratio = parsed;
            }
            other => {
                eprintln!("split: unknown option '{other}'");
                eprintln!("Usage: split [--horizontal|--vertical] [--ratio <0.0-1.0>]");
                return Ok(BuiltinResult::Failure(1));
            }
        }
        i += 1;
    }

    emit_pane_split(direction, ratio);
    Ok(BuiltinResult::Success(0))
}

/// `vsplit` — shorthand for `split --vertical`
pub fn builtin_vsplit(args: &[String]) -> Result<BuiltinResult> {
    let mut ratio = 0.5;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--ratio" | "-r" => {
                i += 1;
                let Some(r) = args.get(i) else {
                    eprintln!("vsplit: --ratio requires a value (0.0-1.0)");
                    return Ok(BuiltinResult::Failure(1));
                };
                let Ok(parsed) = r.parse::<f64>() else {
                    eprintln!("vsplit: invalid ratio '{r}'");
                    return Ok(BuiltinResult::Failure(1));
                };
                if !(0.0..=1.0).contains(&parsed) {
                    eprintln!("vsplit: ratio must be between 0.0 and 1.0");
                    return Ok(BuiltinResult::Failure(1));
                }
                ratio = parsed;
            }
            other => {
                eprintln!("vsplit: unknown option '{other}'");
                eprintln!("Usage: vsplit [--ratio <0.0-1.0>]");
                return Ok(BuiltinResult::Failure(1));
            }
        }
        i += 1;
    }

    emit_pane_split(PaneSplitDirection::Vertical, ratio);
    Ok(BuiltinResult::Success(0))
}

/// `layout <name>` — apply a named layout preset
pub fn builtin_layout(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        let layout = PaneLayout::new();
        let names = layout.preset_names();
        eprintln!("Available layouts: {}", names.join(", "));
        eprintln!("Usage: layout <name>");
        return Ok(BuiltinResult::Failure(1));
    }

    let preset_name = &args[0];
    let mut layout = PaneLayout::new();

    // Try loading user layouts from ~/.colony/layouts.toml
    if let Some(config_dir) = dirs::home_dir() {
        let layouts_path = config_dir.join(".colony/layouts.toml");
        if layouts_path.exists() {
            if let Ok(toml_str) = std::fs::read_to_string(&layouts_path) {
                if let Err(e) = layout.load_from_toml(&toml_str) {
                    eprintln!("layout: warning: failed to parse layouts.toml: {e}");
                }
            }
        }
    }

    let Some(preset) = layout.get(preset_name) else {
        eprintln!("layout: unknown preset '{preset_name}'");
        let names = layout.preset_names();
        eprintln!("Available: {}", names.join(", "));
        return Ok(BuiltinResult::Failure(1));
    };

    let panes_json = serde_json::to_value(&preset.panes).ok();
    emit_pane_layout(preset_name, panes_json);
    Ok(BuiltinResult::Success(0))
}

/// `pane <subcommand>` — pane management
///
/// Subcommands:
///   `pane focus <left|right|up|down>` — move focus
///   `pane close` — close current pane
pub fn builtin_pane(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("Usage: pane <focus|close> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    match args[0].as_str() {
        "focus" => {
            let Some(dir_str) = args.get(1) else {
                eprintln!("pane focus: missing direction");
                eprintln!("Usage: pane focus <left|right|up|down>");
                return Ok(BuiltinResult::Failure(1));
            };
            let direction = match dir_str.as_str() {
                "left" | "l" => PaneFocusDirection::Left,
                "right" | "r" => PaneFocusDirection::Right,
                "up" | "u" => PaneFocusDirection::Up,
                "down" | "d" => PaneFocusDirection::Down,
                other => {
                    eprintln!("pane focus: unknown direction '{other}'");
                    eprintln!("Directions: left, right, up, down");
                    return Ok(BuiltinResult::Failure(1));
                }
            };
            emit_pane_focus(direction);
        }
        "close" => {
            let pane_id = args.get(1).and_then(|s| s.parse::<u32>().ok());
            emit_pane_close(pane_id);
        }
        other => {
            eprintln!("pane: unknown subcommand '{other}'");
            eprintln!("Usage: pane <focus|close> [args...]");
            return Ok(BuiltinResult::Failure(1));
        }
    }

    Ok(BuiltinResult::Success(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_no_args_defaults() {
        let result = builtin_split(&[]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_split_horizontal() {
        let result = builtin_split(&["--horizontal".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_split_vertical() {
        let result = builtin_split(&["--vertical".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_split_with_ratio() {
        let result = builtin_split(&[
            "--vertical".to_string(),
            "--ratio".to_string(),
            "0.3".to_string(),
        ])
        .unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_split_invalid_ratio() {
        let result = builtin_split(&["--ratio".to_string(), "1.5".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_split_missing_ratio_value() {
        let result = builtin_split(&["--ratio".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_split_unknown_option() {
        let result = builtin_split(&["--unknown".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_vsplit_no_args() {
        let result = builtin_vsplit(&[]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_vsplit_with_ratio() {
        let result = builtin_vsplit(&["--ratio".to_string(), "0.7".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_layout_no_args() {
        let result = builtin_layout(&[]).unwrap();
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_layout_dev() {
        let result = builtin_layout(&["dev".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_layout_unknown() {
        let result = builtin_layout(&["nonexistent".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_pane_no_args() {
        let result = builtin_pane(&[]).unwrap();
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_pane_focus_left() {
        let result = builtin_pane(&["focus".to_string(), "left".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_pane_focus_shorthand() {
        let result = builtin_pane(&["focus".to_string(), "r".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_pane_focus_missing_direction() {
        let result = builtin_pane(&["focus".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_pane_focus_invalid_direction() {
        let result = builtin_pane(&["focus".to_string(), "diagonal".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_pane_close() {
        let result = builtin_pane(&["close".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_pane_close_with_id() {
        let result = builtin_pane(&["close".to_string(), "3".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Success(0)));
    }

    #[test]
    fn test_pane_unknown_subcommand() {
        let result = builtin_pane(&["resize".to_string()]).unwrap();
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }
}
