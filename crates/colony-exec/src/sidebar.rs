//! The `sidebar` builtin command.
//!
//! Usage:
//!   sidebar <position> [options] <command>   # add a panel
//!   sidebar reset                             # clear all panels
//!   sidebar reset <position>                  # clear panels at position

use colony_panel::PanelPosition;
use serde::Serialize;

/// Result of parsing a sidebar command.
#[derive(Debug, Serialize)]
pub enum SidebarCommand {
    /// Add a panel at the given position.
    Add {
        position: PanelPosition,
        command: String,
        args: Vec<String>,
        weight: u16,
        min_size: Option<u16>,
        max_size: Option<u16>,
        outer: bool,
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
            _ => {
                command_start = i;
                break;
            }
        }
        i += 1;
        command_start = i;
    }

    if command_start >= args.len() {
        return Err("sidebar requires a command".to_string());
    }

    let command = args[command_start].clone();
    let cmd_args = args[command_start + 1..].to_vec();

    Ok(SidebarCommand::Add {
        position,
        command,
        args: cmd_args,
        weight,
        min_size,
        max_size,
        outer,
    })
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
                position, command, ..
            } => {
                assert_eq!(position, PanelPosition::Left);
                assert_eq!(command, "filetree");
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
            SidebarCommand::Add {
                command,
                args: cmd_args,
                ..
            } => {
                assert_eq!(command, "preview");
                assert_eq!(cmd_args, vec!["--theme", "dark"]);
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
}
