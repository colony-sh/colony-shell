//! serve-monitor: panel binary for live HTTP traffic monitoring.
//!
//! Subscribes to `http_request` tuples and displays recent requests,
//! server stats, and route information. Follows the standard colony
//! panel protocol (same as session-panel, history-panel, etc.).

use colony_panel::protocol::{FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME};
use std::io::{self, BufRead, Write};
const MAX_RECENT_REQUESTS: usize = 100;

fn main() {
    let mut state = MonitorState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["http_request".to_string()],
            help: vec![
                "Tab: toggle traffic/routes".to_string(),
                "j/k: scroll".to_string(),
                "c: clear".to_string(),
            ],
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
                if key == "http_request" {
                    if let Some(value) = data.get("value") {
                        state.add_request(value);
                    }
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Key { ref key, .. } => {
                if handle_key(&mut state, key) {
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Resize { .. } => emit_frame(&stdout, &state),
            InputEvent::Shutdown => break,
            InputEvent::Focus { .. } => {}
        }
    }
}

fn handle_key(state: &mut MonitorState, key: &str) -> bool {
    match key {
        "Tab" => {
            state.toggle_mode();
            true
        }
        "j" | "Down" => state.scroll_down(),
        "k" | "Up" => state.scroll_up(),
        "c" => {
            state.clear();
            true
        }
        _ => false,
    }
}

fn emit_frame(stdout: &io::Stdout, state: &MonitorState) {
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
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Traffic,
    Routes,
}

#[derive(Debug, Clone)]
struct RequestEntry {
    method: String,
    path: String,
    status: u16,
    duration_ms: u64,
    timestamp: String,
}

struct MonitorState {
    mode: Mode,
    requests: Vec<RequestEntry>,
    total_requests: u64,
    total_errors: u64,
    cursor: usize,
}

impl MonitorState {
    fn new() -> Self {
        Self {
            mode: Mode::Traffic,
            requests: Vec::new(),
            total_requests: 0,
            total_errors: 0,
            cursor: 0,
        }
    }

    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Traffic => Mode::Routes,
            Mode::Routes => Mode::Traffic,
        };
    }

    #[allow(clippy::cast_possible_truncation)]
    fn add_request(&mut self, value: &serde_json::Value) {
        // Parse tuple values from the JSON.
        // Tuple format: ("http_request", session_id, method, path, status, duration_ms)
        // The panel receives it as JSON with fields or array indices.
        let method = value
            .get("method")
            .or_else(|| value.get(0))
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let path = value
            .get("path")
            .or_else(|| value.get(1))
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let status = value
            .get("status")
            .or_else(|| value.get(2))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u16;
        let duration_ms = value
            .get("duration_ms")
            .or_else(|| value.get(3))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

        let now = chrono::Local::now();
        let timestamp = now.format("%H:%M:%S").to_string();

        self.total_requests += 1;
        if status >= 400 {
            self.total_errors += 1;
        }

        self.requests.insert(
            0,
            RequestEntry {
                method,
                path,
                status,
                duration_ms,
                timestamp,
            },
        );

        // Cap history
        if self.requests.len() > MAX_RECENT_REQUESTS {
            self.requests.truncate(MAX_RECENT_REQUESTS);
        }
    }

    fn scroll_down(&mut self) -> bool {
        if self.cursor + 1 < self.requests.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn scroll_up(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    fn clear(&mut self) {
        self.requests.clear();
        self.cursor = 0;
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        match self.mode {
            Mode::Traffic => self.render_traffic(),
            Mode::Routes => self.render_routes(),
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn render_traffic(&self) -> Vec<FrameLine> {
        let mut lines = Vec::new();

        // Status line
        let err_rate = if self.total_requests > 0 {
            (self.total_errors as f64 / self.total_requests as f64) * 100.0
        } else {
            0.0
        };

        lines.push(FrameLine {
            text: format!(
                "{} req  {} err  {:.1}% err rate",
                self.total_requests, self.total_errors, err_rate
            ),
            icon: None,
            style: Some("bold,cyan".to_string()),
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        });

        // Blank separator
        lines.push(FrameLine {
            text: String::new(),
            icon: None,
            style: None,
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        });

        // Recent requests
        for (i, req) in self.requests.iter().enumerate() {
            let status_style = if req.status >= 500 {
                "red"
            } else if req.status >= 400 {
                "yellow"
            } else {
                "green"
            };

            let selected = i == self.cursor;
            lines.push(FrameLine {
                text: format!(
                    "{:<4} {:<30} {}  {:>4}ms  {}",
                    req.method, req.path, req.status, req.duration_ms, req.timestamp
                ),
                icon: None,
                style: Some(if selected {
                    format!("bold,{status_style}")
                } else {
                    status_style.to_string()
                }),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: Some(selected),
            });
        }

        if self.requests.is_empty() {
            lines.push(FrameLine {
                text: "No requests yet".to_string(),
                icon: None,
                style: Some("dim".to_string()),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        }

        lines
    }

    #[allow(clippy::unused_self)]
    fn render_routes(&self) -> Vec<FrameLine> {
        // Routes mode shows a placeholder since the panel doesn't have
        // direct access to serve config. Route tuples could be added in
        // a future enhancement.
        vec![FrameLine {
            text: "Routes: use 'serve routes' to view registered routes".to_string(),
            icon: None,
            style: Some("dim".to_string()),
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        }]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_state_initially_empty() {
        let state = MonitorState::new();
        assert_eq!(state.mode, Mode::Traffic);
        assert!(state.requests.is_empty());
        assert_eq!(state.total_requests, 0);
        assert_eq!(state.total_errors, 0);
    }

    #[test]
    fn toggle_mode() {
        let mut state = MonitorState::new();
        assert_eq!(state.mode, Mode::Traffic);
        state.toggle_mode();
        assert_eq!(state.mode, Mode::Routes);
        state.toggle_mode();
        assert_eq!(state.mode, Mode::Traffic);
    }

    #[test]
    fn add_request_increments_counters() {
        let mut state = MonitorState::new();
        let value = serde_json::json!({
            "method": "GET",
            "path": "/api/test",
            "status": 200,
            "duration_ms": 42
        });
        state.add_request(&value);
        assert_eq!(state.total_requests, 1);
        assert_eq!(state.total_errors, 0);
        assert_eq!(state.requests.len(), 1);
        assert_eq!(state.requests[0].method, "GET");
        assert_eq!(state.requests[0].status, 200);
    }

    #[test]
    fn add_error_request_increments_errors() {
        let mut state = MonitorState::new();
        let value = serde_json::json!({
            "method": "POST",
            "path": "/api/fail",
            "status": 500,
            "duration_ms": 100
        });
        state.add_request(&value);
        assert_eq!(state.total_errors, 1);
    }

    #[test]
    fn clear_resets_state() {
        let mut state = MonitorState::new();
        let value = serde_json::json!({
            "method": "GET",
            "path": "/",
            "status": 200,
            "duration_ms": 1
        });
        state.add_request(&value);
        state.clear();
        assert!(state.requests.is_empty());
        assert_eq!(state.cursor, 0);
        // total_requests is not reset by clear (it's a lifetime counter)
        assert_eq!(state.total_requests, 1);
    }

    #[test]
    fn scroll_within_bounds() {
        let mut state = MonitorState::new();
        // Cannot scroll with no requests
        assert!(!state.scroll_down());
        assert!(!state.scroll_up());

        for i in 0..5 {
            let value = serde_json::json!({
                "method": "GET",
                "path": format!("/{i}"),
                "status": 200,
                "duration_ms": 1
            });
            state.add_request(&value);
        }

        assert!(state.scroll_down());
        assert_eq!(state.cursor, 1);
        assert!(state.scroll_up());
        assert_eq!(state.cursor, 0);
        assert!(!state.scroll_up()); // already at top
    }

    #[test]
    fn render_lines_not_empty() {
        let state = MonitorState::new();
        let lines = state.render_lines();
        assert!(!lines.is_empty());
    }

    #[test]
    fn max_recent_requests_cap() {
        let mut state = MonitorState::new();
        for i in 0..150 {
            let value = serde_json::json!({
                "method": "GET",
                "path": format!("/{i}"),
                "status": 200,
                "duration_ms": 1
            });
            state.add_request(&value);
        }
        assert_eq!(state.requests.len(), MAX_RECENT_REQUESTS);
        assert_eq!(state.total_requests, 150);
    }
}
