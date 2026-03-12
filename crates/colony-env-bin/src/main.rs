//! env-panel: a panel binary that displays filterable environment variables.
//!
//! Emits frames to stdout following the panel protocol.
//! Subscribes to `env_change` tuples to refresh the variable list.
//! Reads key events from stdin for navigation and filtering.

use colony_panel::protocol::{
    FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME,
};
use std::io::{self, BufRead, Write};

fn main() {
    let mut state = EnvState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["env_change".to_string()],
            help: Vec::new(),
        };
        writeln!(out, "{}", serde_json::to_string(&config).expect("config must serialize")).ok();
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
                ref modifiers,
            } => {
                let changed = handle_key(&mut state, key, modifiers);
                if changed {
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Tuple { ref data } => {
                // React to env_change: refresh the environment variable list
                if let Some(key) = data.get("key").and_then(|k| k.as_str()) {
                    if key == "env_change" {
                        state.refresh();
                        emit_frame(&stdout, &state);
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

fn handle_key(state: &mut EnvState, key: &str, _modifiers: &[String]) -> bool {
    if state.filter_mode {
        match key {
            "Escape" => {
                state.filter_mode = false;
                state.filter.clear();
                state.rebuild_visible();
                true
            }
            "Enter" => {
                state.filter_mode = false;
                true
            }
            "Backspace" => {
                state.filter.pop();
                state.rebuild_visible();
                true
            }
            ch if ch.len() == 1 => {
                state.filter.push_str(ch);
                state.rebuild_visible();
                true
            }
            _ => false,
        }
    } else {
        match key {
            "j" | "Down" => state.cursor_down(),
            "k" | "Up" => state.cursor_up(),
            "/" => {
                state.filter_mode = true;
                state.filter.clear();
                true
            }
            _ => false,
        }
    }
}

fn emit_frame(stdout: &io::Stdout, state: &EnvState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in state.render_lines() {
        writeln!(out, "{}", serde_json::to_string(&line).expect("frame line must serialize")).ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

// ---------------------------------------------------------------------------
// Environment state
// ---------------------------------------------------------------------------

struct EnvState {
    /// All environment variables, sorted by name.
    all_vars: Vec<(String, String)>,
    /// Indices into `all_vars` matching the current filter.
    visible: Vec<usize>,
    cursor: usize,
    filter: String,
    filter_mode: bool,
    width: u16,
    _height: u16,
}

impl EnvState {
    fn new() -> Self {
        let mut state = Self {
            all_vars: Vec::new(),
            visible: Vec::new(),
            cursor: 0,
            filter: String::new(),
            filter_mode: false,
            width: 60,
            _height: 40,
        };
        state.refresh();
        state
    }

    /// Re-read all environment variables from the process.
    fn refresh(&mut self) {
        let mut vars: Vec<(String, String)> = std::env::vars().collect();
        vars.sort_by(|a, b| a.0.cmp(&b.0));
        self.all_vars = vars;
        self.rebuild_visible();
    }

    /// Rebuild the visible list based on the current filter.
    fn rebuild_visible(&mut self) {
        let filter_upper = self.filter.to_uppercase();
        self.visible = self
            .all_vars
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| {
                self.filter.is_empty() || name.to_uppercase().contains(&filter_upper)
            })
            .map(|(i, _)| i)
            .collect();

        // Clamp cursor
        if self.visible.is_empty() {
            self.cursor = 0;
        } else if self.cursor >= self.visible.len() {
            self.cursor = self.visible.len() - 1;
        }
    }

    fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self._height = height;
    }

    fn cursor_down(&mut self) -> bool {
        if !self.visible.is_empty() && self.cursor < self.visible.len() - 1 {
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
        let mut lines = Vec::with_capacity(self.visible.len() + 1);

        // Filter status line
        if self.filter_mode {
            lines.push(FrameLine {
                text: format!("/{}", self.filter),
                icon: None,
                style: Some("bold,yellow".to_string()),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        } else if !self.filter.is_empty() {
            lines.push(FrameLine {
                text: format!("filter: {} ({} matches)", self.filter, self.visible.len()),
                icon: None,
                style: Some("dim".to_string()),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        }

        for (vi, &idx) in self.visible.iter().enumerate() {
            let (name, value) = &self.all_vars[idx];
            let truncated_value = truncate_value(value, self.width, name.len());
            let text = format!("{name}={truncated_value}");

            let style = if vi == self.cursor {
                "bold,yellow"
            } else {
                "bold,cyan"
            };

            lines.push(FrameLine {
                text,
                icon: None,
                style: Some(style.to_string()),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        }

        lines
    }
}

/// Truncate a value to fit within the panel width.
///
/// Reserves space for `NAME=` plus an ellipsis marker.
fn truncate_value(value: &str, panel_width: u16, name_len: usize) -> String {
    let width = panel_width as usize;
    // "NAME=" takes name_len + 1 chars; leave room for the text
    let available = width.saturating_sub(name_len + 1);
    if available == 0 {
        return String::new();
    }

    if value.len() <= available {
        value.to_string()
    } else {
        let end = available.saturating_sub(1);
        let mut truncated: String = value.chars().take(end).collect();
        truncated.push('\u{2026}'); // ellipsis
        truncated
    }
}
