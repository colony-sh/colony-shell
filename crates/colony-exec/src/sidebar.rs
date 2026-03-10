//! The `sidebar` builtin command.
//!
//! Usage:
//!   sidebar <position> [options] <command>   # add a panel
//!   sidebar reset                             # clear all panels
//!   sidebar reset <position>                  # clear panels at position

use colony_panel::PanelPosition;
use serde::{Deserialize, Serialize};

/// How multiple panels at the same position are arranged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutMode {
    /// Show one panel at a time with a tab bar (default).
    #[default]
    Tabs,
    /// Split the space and show all panels simultaneously.
    Stack,
}

/// Result of parsing a sidebar command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SidebarCommand {
    /// Add one or more panels at the given position.
    Add {
        position: PanelPosition,
        commands: Vec<(String, Vec<String>)>,
        layout_mode: LayoutMode,
        weight: u16,
        min_size: Option<u16>,
        max_size: Option<u16>,
        outer: bool,
        hotkey: Option<String>,
    },
    /// Reset all panels.
    Reset,
    /// Reset panels at a specific position.
    ResetPosition(PanelPosition),
}

/// Parse sidebar command arguments.
pub fn parse_sidebar_args(args: &[String]) -> Result<SidebarCommand, String> {
    if args.is_empty() {
        return Err(
            "usage: sidebar <position> [options] <command> | sidebar reset [position]".to_string(),
        );
    }

    if args[0] == "reset" {
        return if args.len() > 1 {
            let pos = parse_position(&args[1])?;
            Ok(SidebarCommand::ResetPosition(pos))
        } else {
            Ok(SidebarCommand::Reset)
        };
    }

    let position = parse_position(&args[0])?;
    let mut weight = 1u16;
    let mut min_size = None;
    let mut max_size = None;
    let mut outer = false;
    let mut hotkey = None;
    let mut command_start = 1;

    // Parse options
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--weight" => {
                i += 1;
                weight = args
                    .get(i)
                    .ok_or("--weight requires a value")?
                    .parse()
                    .map_err(|_| "--weight must be a number".to_string())?;
            }
            "--min-width" | "--min-height" => {
                i += 1;
                min_size = Some(
                    args.get(i)
                        .ok_or("--min-width/--min-height requires a value")?
                        .parse()
                        .map_err(|_| "size must be a number".to_string())?,
                );
            }
            "--max-width" | "--max-height" => {
                i += 1;
                max_size = Some(
                    args.get(i)
                        .ok_or("--max-width/--max-height requires a value")?
                        .parse()
                        .map_err(|_| "size must be a number".to_string())?,
                );
            }
            "--outer" => {
                if !matches!(position, PanelPosition::Top | PanelPosition::Bottom) {
                    return Err("--outer only applies to top/bottom panels".to_string());
                }
                outer = true;
            }
            "--hotkey" => {
                i += 1;
                let key = args
                    .get(i)
                    .ok_or("--hotkey requires a value (e.g. ctrl-b)")?;
                parse_hotkey_str(key)?; // validate the format
                hotkey = Some(key.clone());
            }
            _ => {
                command_start = i;
                break;
            }
        }
        i += 1;
        command_start = i;
    }

    // Parse --tabs or --stack flag
    let mut layout_mode = LayoutMode::default();
    while i < args.len() {
        match args[i].as_str() {
            "--tabs" => {
                layout_mode = LayoutMode::Tabs;
                i += 1;
                command_start = i;
            }
            "--stack" => {
                layout_mode = LayoutMode::Stack;
                i += 1;
                command_start = i;
            }
            _ => {
                command_start = i;
                break;
            }
        }
    }

    if command_start >= args.len() {
        return Err("sidebar requires at least one command".to_string());
    }

    // Remaining args: if only one command, the rest are its args.
    // If multiple commands, each is a bare command name (no per-command args).
    let remaining = &args[command_start..];
    let commands = if remaining.len() == 1 {
        vec![(remaining[0].clone(), Vec::new())]
    } else {
        // Check if any remaining args look like command args (start with --)
        // If so, treat as single command with args
        let has_flags = remaining[1..].iter().any(|a| a.starts_with('-'));
        if has_flags {
            vec![(remaining[0].clone(), remaining[1..].to_vec())]
        } else {
            remaining.iter().map(|cmd| (cmd.clone(), Vec::new())).collect()
        }
    };

    Ok(SidebarCommand::Add {
        position,
        commands,
        layout_mode,
        weight,
        min_size,
        max_size,
        outer,
        hotkey,
    })
}

/// Validate a hotkey string like "ctrl-b", "ctrl+b", "ctrl-shift-f", "f1".
///
/// Returns `(modifiers, key_char_or_name)` if valid.
/// Accepts both `-` and `+` as separators (normalized to `-` internally).
pub fn parse_hotkey_str(s: &str) -> Result<(Vec<&str>, &str), String> {
    // Normalize: split on either - or +
    let parts: Vec<&str> = s.split(|c| c == '-' || c == '+').collect();
    if parts.is_empty() {
        return Err("empty hotkey".to_string());
    }

    let key = parts.last().unwrap();
    let modifiers = &parts[..parts.len() - 1];

    for m in modifiers {
        match m.to_lowercase().as_str() {
            "ctrl" | "alt" | "shift" => {}
            _ => return Err(format!("unknown modifier '{}': expected ctrl, alt, or shift", m)),
        }
    }

    // Validate key part
    let lower = key.to_lowercase();
    let valid_key = lower.len() == 1
        || matches!(
            lower.as_str(),
            "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12"
                | "space" | "tab" | "esc" | "enter" | "backspace" | "delete" | "insert"
                | "home" | "end" | "pageup" | "pagedown" | "up" | "down" | "left" | "right"
        );

    if !valid_key {
        return Err(format!("unknown key '{}' in hotkey", key));
    }

    Ok((modifiers.to_vec(), key))
}

fn parse_position(s: &str) -> Result<PanelPosition, String> {
    match s {
        "left" => Ok(PanelPosition::Left),
        "right" => Ok(PanelPosition::Right),
        "top" => Ok(PanelPosition::Top),
        "bottom" => Ok(PanelPosition::Bottom),
        _ => Err(format!(
            "unknown position '{}': expected left, right, top, or bottom",
            s
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn test_parse_simple() {
        let cmd = parse_sidebar_args(&args(&["left", "filetree"])).unwrap();
        match cmd {
            SidebarCommand::Add {
                position, commands, layout_mode, ..
            } => {
                assert_eq!(position, PanelPosition::Left);
                assert_eq!(commands.len(), 1);
                assert_eq!(commands[0].0, "filetree");
                assert_eq!(layout_mode, LayoutMode::Tabs);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_with_options() {
        let cmd =
            parse_sidebar_args(&args(&["left", "--weight", "2", "--max-width", "30", "filetree"]))
                .unwrap();
        match cmd {
            SidebarCommand::Add {
                weight, max_size, ..
            } => {
                assert_eq!(weight, 2);
                assert_eq!(max_size, Some(30));
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_with_command_args() {
        let cmd =
            parse_sidebar_args(&args(&["right", "preview", "--theme", "dark"])).unwrap();
        match cmd {
            SidebarCommand::Add { commands, .. } => {
                assert_eq!(commands.len(), 1);
                assert_eq!(commands[0].0, "preview");
                assert_eq!(commands[0].1, vec!["--theme", "dark"]);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_multi_command_tabs() {
        let cmd = parse_sidebar_args(&args(&["left", "filetree", "filedetails"])).unwrap();
        match cmd {
            SidebarCommand::Add { commands, layout_mode, .. } => {
                assert_eq!(commands.len(), 2);
                assert_eq!(commands[0].0, "filetree");
                assert_eq!(commands[1].0, "filedetails");
                assert_eq!(layout_mode, LayoutMode::Tabs);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_stack_mode() {
        let cmd = parse_sidebar_args(&args(&["left", "--stack", "filetree", "filedetails"])).unwrap();
        match cmd {
            SidebarCommand::Add { commands, layout_mode, .. } => {
                assert_eq!(commands.len(), 2);
                assert_eq!(layout_mode, LayoutMode::Stack);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_explicit_tabs_mode() {
        let cmd = parse_sidebar_args(&args(&["left", "--tabs", "filetree", "filedetails"])).unwrap();
        match cmd {
            SidebarCommand::Add { layout_mode, .. } => {
                assert_eq!(layout_mode, LayoutMode::Tabs);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_reset() {
        assert!(matches!(
            parse_sidebar_args(&args(&["reset"])).unwrap(),
            SidebarCommand::Reset
        ));
    }

    #[test]
    fn test_parse_reset_position() {
        assert!(matches!(
            parse_sidebar_args(&args(&["reset", "left"])).unwrap(),
            SidebarCommand::ResetPosition(PanelPosition::Left)
        ));
    }

    #[test]
    fn test_parse_outer() {
        match parse_sidebar_args(&args(&["bottom", "--outer", "logs"])).unwrap() {
            SidebarCommand::Add { outer, .. } => assert!(outer),
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_outer_on_left_fails() {
        assert!(parse_sidebar_args(&args(&["left", "--outer", "filetree"])).is_err());
    }

    #[test]
    fn test_empty_args_fails() {
        assert!(parse_sidebar_args(&[]).is_err());
    }

    #[test]
    fn test_no_command_fails() {
        assert!(parse_sidebar_args(&args(&["left"])).is_err());
    }

    #[test]
    fn test_parse_hotkey() {
        let cmd = parse_sidebar_args(&args(&["left", "--hotkey", "ctrl-b", "filetree"])).unwrap();
        match cmd {
            SidebarCommand::Add { hotkey, commands, .. } => {
                assert_eq!(hotkey, Some("ctrl-b".to_string()));
                assert_eq!(commands[0].0, "filetree");
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_hotkey_with_stack() {
        let cmd = parse_sidebar_args(&args(&["left", "--hotkey", "ctrl-b", "--stack", "filetree", "filedetails"])).unwrap();
        match cmd {
            SidebarCommand::Add { hotkey, layout_mode, commands, .. } => {
                assert_eq!(hotkey, Some("ctrl-b".to_string()));
                assert_eq!(layout_mode, LayoutMode::Stack);
                assert_eq!(commands.len(), 2);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_hotkey_plus_syntax() {
        let cmd = parse_sidebar_args(&args(&["right", "--hotkey", "ctrl+b", "preview"])).unwrap();
        match cmd {
            SidebarCommand::Add { hotkey, .. } => {
                assert_eq!(hotkey, Some("ctrl+b".to_string()));
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_no_hotkey() {
        let cmd = parse_sidebar_args(&args(&["left", "filetree"])).unwrap();
        match cmd {
            SidebarCommand::Add { hotkey, .. } => assert!(hotkey.is_none()),
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_hotkey_invalid_modifier() {
        assert!(parse_sidebar_args(&args(&["left", "--hotkey", "meta-b", "filetree"])).is_err());
    }

    #[test]
    fn test_parse_hotkey_str_valid() {
        assert!(parse_hotkey_str("ctrl-b").is_ok());
        assert!(parse_hotkey_str("ctrl+shift+f").is_ok());
        assert!(parse_hotkey_str("f1").is_ok());
        assert!(parse_hotkey_str("alt-f12").is_ok());
    }

    #[test]
    fn test_parse_hotkey_str_invalid() {
        assert!(parse_hotkey_str("meta-b").is_err());
        assert!(parse_hotkey_str("ctrl-foo").is_err());
    }
}
