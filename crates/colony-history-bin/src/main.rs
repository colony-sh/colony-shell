//! history-panel: a panel binary that displays recent command history.
//!
//! Emits frames to stdout following the panel protocol.
//! Subscribes to `cmd_complete` tuples to prepend new commands.
//! Reads key events from stdin for j/k navigation.

use colony_panel::protocol::{FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

fn main() {
    let mut state = HistoryState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["cmd_complete".to_string()],
            help: Vec::new(),
        };
        writeln!(
            out,
            "{}",
            serde_json::to_string(&config).expect("config must serialize")
        )
        .ok();
        out.flush().ok();
    }

    // Initial render
    emit_frame(&stdout, &state);

    // Event loop: read stdin for events
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(event) = serde_json::from_str::<InputEvent>(&line) else {
            continue;
        };

        match event {
            InputEvent::Key {
                ref key,
                modifiers: _,
            } => {
                let changed = handle_key(&mut state, key);
                if changed {
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Tuple { ref data } => {
                if let Some(key) = data.get("key").and_then(|k| k.as_str()) {
                    if key == "cmd_complete" {
                        if let Some(cmd) = data
                            .get("value")
                            .and_then(|v| v.get("command"))
                            .and_then(|c| c.as_str())
                        {
                            state.prepend_command(cmd);
                            emit_frame(&stdout, &state);
                        }
                    }
                }
            }
            InputEvent::Resize { width, height } => {
                state.set_size(width, height);
                emit_frame(&stdout, &state);
            }
            InputEvent::Shutdown => break,
            InputEvent::Focus { .. } => {}
        }
    }
}

fn handle_key(state: &mut HistoryState, key: &str) -> bool {
    match key {
        "j" | "Down" => state.cursor_down(),
        "k" | "Up" => state.cursor_up(),
        _ => false,
    }
}

fn emit_frame(stdout: &io::Stdout, state: &HistoryState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in state.render_lines() {
        writeln!(
            out,
            "{}",
            serde_json::to_string(&line).expect("frame line must serialize")
        )
        .ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

// ---------------------------------------------------------------------------
// History state
// ---------------------------------------------------------------------------

struct HistoryState {
    /// Commands in reverse chronological order (most recent first).
    commands: Vec<String>,
    cursor: usize,
    _width: u16,
    _height: u16,
}

impl HistoryState {
    fn new() -> Self {
        let commands = load_history();
        Self {
            commands,
            cursor: 0,
            _width: 40,
            _height: 40,
        }
    }

    fn set_size(&mut self, width: u16, height: u16) {
        self._width = width;
        self._height = height;
    }

    /// Prepend a command, collapsing consecutive duplicates.
    fn prepend_command(&mut self, cmd: &str) {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return;
        }
        // Collapse consecutive duplicate: skip if the most recent command matches.
        if self.commands.first().is_some_and(|first| first == trimmed) {
            return;
        }
        self.commands.insert(0, trimmed.to_string());
    }

    fn cursor_down(&mut self) -> bool {
        if self.cursor + 1 < self.commands.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn cursor_up(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        self.commands
            .iter()
            .enumerate()
            .map(|(i, cmd)| {
                let line_num = format!("{:>4} ", i + 1);
                let is_selected = i == self.cursor;
                let style = if is_selected { "bold,white" } else { "white" };

                FrameLine {
                    // Prefix the dim line number into a separate FrameLine field
                    // is not available, so we embed it in the text.
                    text: format!("{line_num}{cmd}"),
                    icon: if is_selected {
                        Some("\u{f0da}".to_string()) // right-pointing triangle
                    } else {
                        None
                    },
                    style: Some(style.to_string()),
                    indent: None,
                    action: None,
                    expandable: None,
                    expanded: None,
                    selected: None,
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// History file loading
// ---------------------------------------------------------------------------

/// Load history from `~/.colony_history`, falling back to `~/.bash_history`.
///
/// Returns commands in reverse chronological order with consecutive
/// duplicates collapsed.
fn load_history() -> Vec<String> {
    let home = match home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let colony_history = home.join(".colony_history");
    let bash_history = home.join(".bash_history");

    let path = if colony_history.exists() {
        colony_history
    } else if bash_history.exists() {
        bash_history
    } else {
        return Vec::new();
    };

    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

    let mut commands: Vec<String> = Vec::new();
    for line in contents.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Collapse consecutive duplicates (consecutive in the reversed order).
        if commands.last().is_some_and(|last| last == trimmed) {
            continue;
        }
        commands.push(trimmed.to_string());
    }

    commands
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
