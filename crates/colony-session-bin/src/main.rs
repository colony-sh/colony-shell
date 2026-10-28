//! session-panel: a panel binary that displays session statistics.
//!
//! Shows session duration, command count, token usage, and estimated cost.
//! Subscribes to `agent_tokens` and `cmd_complete` tuples.

use colony_panel::protocol::{FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME};
use std::io::{self, BufRead, Write};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Approximate pricing per token (USD)
// ---------------------------------------------------------------------------

const INPUT_TOKEN_COST: f64 = 0.000_003;
const OUTPUT_TOKEN_COST: f64 = 0.000_015;

fn main() {
    let mut state = SessionState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["agent_tokens".to_string(), "cmd_complete".to_string()],
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
            InputEvent::Tuple { ref data } => {
                let Some(key) = data.get("key").and_then(|k| k.as_str()) else {
                    continue;
                };
                match key {
                    "cmd_complete" => {
                        state.command_count += 1;
                        emit_frame(&stdout, &state);
                    }
                    "agent_tokens" => {
                        if let Some(value) = data.get("value") {
                            if let Some(input) = value.get("input_tokens").and_then(|v| v.as_u64())
                            {
                                state.input_tokens = input;
                            }
                            if let Some(output) =
                                value.get("output_tokens").and_then(|v| v.as_u64())
                            {
                                state.output_tokens = output;
                            }
                        }
                        emit_frame(&stdout, &state);
                    }
                    _ => {}
                }
            }
            InputEvent::Resize { .. } => {
                emit_frame(&stdout, &state);
            }
            InputEvent::Shutdown => break,
            InputEvent::Key { .. } | InputEvent::Focus { .. } => {}
        }
    }
}

fn emit_frame(stdout: &io::Stdout, state: &SessionState) {
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
// Session state
// ---------------------------------------------------------------------------

struct SessionState {
    start: Instant,
    command_count: u64,
    input_tokens: u64,
    output_tokens: u64,
}

impl SessionState {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            command_count: 0,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    fn estimated_cost(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let cost = (self.input_tokens as f64 * INPUT_TOKEN_COST)
            + (self.output_tokens as f64 * OUTPUT_TOKEN_COST);
        cost
    }

    fn elapsed_display(&self) -> String {
        let secs = self.start.elapsed().as_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let s = secs % 60;
        if hours > 0 {
            format!("{hours}h {mins:02}m {s:02}s")
        } else if mins > 0 {
            format!("{mins}m {s:02}s")
        } else {
            format!("{s}s")
        }
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        vec![
            stat_line("Duration", &self.elapsed_display()),
            stat_line("Commands", &format_number(self.command_count)),
            stat_line("Tokens", &format_number(self.total_tokens())),
            stat_line("  Input", &format_number(self.input_tokens)),
            stat_line("  Output", &format_number(self.output_tokens)),
            stat_line("Est. Cost", &format!("${:.4}", self.estimated_cost())),
        ]
    }
}

/// Build a single stat line with a bold-cyan label and default-styled value.
fn stat_line(label: &str, value: &str) -> FrameLine {
    FrameLine {
        text: format!("{label}: {value}"),
        icon: None,
        style: Some("bold,cyan".to_string()),
        indent: None,
        action: None,
        expandable: None,
        expanded: None,
        selected: None,
    }
}

/// Format a number with comma separators (e.g. 1234 -> "1,234").
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_number_zero() {
        assert_eq!(format_number(0), "0");
    }

    #[test]
    fn format_number_small() {
        assert_eq!(format_number(42), "42");
    }

    #[test]
    fn format_number_thousands() {
        assert_eq!(format_number(1234), "1,234");
    }

    #[test]
    fn format_number_millions() {
        assert_eq!(format_number(1_234_567), "1,234,567");
    }

    #[test]
    fn elapsed_display_seconds() {
        let state = SessionState::new();
        // Just check it returns something reasonable
        let display = state.elapsed_display();
        assert!(display.ends_with('s'));
    }

    #[test]
    fn estimated_cost_zero_tokens() {
        let state = SessionState::new();
        assert!((state.estimated_cost() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn estimated_cost_with_tokens() {
        let mut state = SessionState::new();
        state.input_tokens = 1000;
        state.output_tokens = 500;
        let expected = 1000.0 * INPUT_TOKEN_COST + 500.0 * OUTPUT_TOKEN_COST;
        assert!((state.estimated_cost() - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn render_lines_count() {
        let state = SessionState::new();
        assert_eq!(state.render_lines().len(), 6);
    }

    #[test]
    fn stat_line_style_is_bold_cyan() {
        let line = stat_line("Test", "123");
        assert_eq!(line.style.as_deref(), Some("bold,cyan"));
        assert_eq!(line.text, "Test: 123");
    }
}
