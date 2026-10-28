//! logs-panel: a panel binary that displays scrolling log output from commands.
//!
//! Subscribes to `cmd_output` tuples and maintains a scrollback buffer.
//! Emits frames to stdout following the panel protocol.

use colony_panel::protocol::{FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME};
use std::collections::VecDeque;
use std::io::{self, BufRead, Write};

/// Maximum number of log lines retained in the scrollback buffer.
const MAX_SCROLLBACK: usize = 1000;

fn main() {
    let mut state = LogState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["cmd_output".to_string()],
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
                    if key == "cmd_output" {
                        handle_cmd_output(&mut state, data);
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

fn handle_key(state: &mut LogState, key: &str) -> bool {
    match key {
        "j" | "Down" => state.scroll_down(),
        "k" | "Up" => state.scroll_up(),
        _ => false,
    }
}

fn handle_cmd_output(state: &mut LogState, data: &serde_json::Value) {
    let value = data.get("value");

    let text = value
        .and_then(|v| v.get("text").or(Some(v)))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let timestamp = value
        .and_then(|v| v.get("timestamp"))
        .and_then(|t| t.as_str())
        .map(String::from);

    // Split on newlines so each line becomes a separate scrollback entry
    for line in text.split('\n') {
        state.push_line(line.to_string(), timestamp.clone());
    }
}

fn emit_frame(stdout: &io::Stdout, state: &LogState) {
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
// Log state
// ---------------------------------------------------------------------------

struct LogLine {
    text: String,
    timestamp: Option<String>,
}

struct LogState {
    lines: VecDeque<LogLine>,
    /// Manual scroll offset from the bottom. 0 means "at the bottom" (auto-scroll).
    scroll_offset: usize,
    height: u16,
    _width: u16,
}

impl LogState {
    fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            scroll_offset: 0,
            height: 24,
            _width: 80,
        }
    }

    fn set_size(&mut self, width: u16, height: u16) {
        self._width = width;
        self.height = height;
    }

    fn push_line(&mut self, text: String, timestamp: Option<String>) {
        self.lines.push_back(LogLine { text, timestamp });
        if self.lines.len() > MAX_SCROLLBACK {
            self.lines.pop_front();
            // Adjust scroll offset if we dropped a line above the view
            if self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        // Auto-scroll to bottom on new content (reset manual scroll)
        self.scroll_offset = 0;
    }

    fn scroll_up(&mut self) -> bool {
        let total = self.lines.len();
        let visible = self.height as usize;
        let max_offset = total.saturating_sub(visible);
        if self.scroll_offset < max_offset {
            self.scroll_offset += 1;
            true
        } else {
            false
        }
    }

    fn scroll_down(&mut self) -> bool {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            true
        } else {
            false
        }
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        if self.lines.is_empty() {
            return vec![FrameLine {
                text: "[waiting for output...]".to_string(),
                icon: None,
                style: Some("dim".to_string()),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            }];
        }

        let visible = self.height as usize;
        let total = self.lines.len();

        // Calculate the range of lines to show.
        // scroll_offset=0 means the bottom of the buffer is visible.
        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(visible);

        self.lines
            .iter()
            .skip(start)
            .take(end - start)
            .map(|log_line| {
                let text = match log_line.timestamp {
                    Some(ref ts) => format!("\x1b[2m{ts}\x1b[0m {}", log_line.text),
                    None => log_line.text.clone(),
                };
                FrameLine {
                    text,
                    icon: None,
                    style: None,
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_shows_waiting_message() {
        let state = LogState::new();
        let lines = state.render_lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "[waiting for output...]");
        assert_eq!(lines[0].style.as_deref(), Some("dim"));
    }

    #[test]
    fn push_line_adds_to_buffer() {
        let mut state = LogState::new();
        state.push_line("hello".to_string(), None);
        assert_eq!(state.lines.len(), 1);
        let rendered = state.render_lines();
        assert_eq!(rendered[0].text, "hello");
    }

    #[test]
    fn scrollback_limit_enforced() {
        let mut state = LogState::new();
        for i in 0..=MAX_SCROLLBACK {
            state.push_line(format!("line {i}"), None);
        }
        assert_eq!(state.lines.len(), MAX_SCROLLBACK);
    }

    #[test]
    fn timestamp_rendered_dim() {
        let mut state = LogState::new();
        state.push_line("msg".to_string(), Some("12:00:00".to_string()));
        let rendered = state.render_lines();
        assert!(rendered[0].text.contains("12:00:00"));
        assert!(rendered[0].text.contains("msg"));
    }

    #[test]
    fn scroll_up_and_down() {
        let mut state = LogState::new();
        state.height = 5;
        for i in 0..20 {
            state.push_line(format!("line {i}"), None);
        }
        // At bottom, scroll_offset = 0
        assert!(!state.scroll_down());
        assert!(state.scroll_up());
        assert_eq!(state.scroll_offset, 1);
        assert!(state.scroll_down());
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn auto_scroll_on_new_content() {
        let mut state = LogState::new();
        state.height = 5;
        for i in 0..20 {
            state.push_line(format!("line {i}"), None);
        }
        state.scroll_up();
        state.scroll_up();
        assert_eq!(state.scroll_offset, 2);
        // New line resets scroll to bottom
        state.push_line("new".to_string(), None);
        assert_eq!(state.scroll_offset, 0);
    }
}
