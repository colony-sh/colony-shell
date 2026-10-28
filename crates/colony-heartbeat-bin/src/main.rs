//! heartbeat-monitor: a panel binary showing heartbeat status.
//!
//! Subscribes to `heartbeat_complete` and `heartbeat_finding` tuples.
//! Shows status, interval, last run time, next run, and recent findings.
//!
//! Key bindings:
//! - Enter: trigger immediate run (emits tuple)
//! - l: show log view

use colony_panel::protocol::{FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME};
use std::io::{self, BufRead, Write};

fn main() {
    let mut state = HeartbeatState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec![
                "heartbeat_complete".to_string(),
                "heartbeat_finding".to_string(),
            ],
            help: vec!["Enter: trigger run".to_string(), "l: show log".to_string()],
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
                    match key {
                        "heartbeat_complete" => {
                            // Extract: path, run_id, status, findings_count, duration_ms
                            let status = data
                                .get("value")
                                .and_then(|v| v.get("status"))
                                .and_then(|s| s.as_str())
                                .unwrap_or("unknown");
                            let duration_ms = data
                                .get("value")
                                .and_then(|v| v.get("duration_ms"))
                                .and_then(|d| d.as_i64())
                                .unwrap_or(0);
                            let findings_count = data
                                .get("value")
                                .and_then(|v| v.get("findings_count"))
                                .and_then(|f| f.as_i64())
                                .unwrap_or(0);

                            state.update_status(status, duration_ms, findings_count);
                            emit_frame(&stdout, &state);
                        }
                        "heartbeat_finding" => {
                            let severity = data
                                .get("value")
                                .and_then(|v| v.get("severity"))
                                .and_then(|s| s.as_str())
                                .unwrap_or("PASS");
                            let message = data
                                .get("value")
                                .and_then(|v| v.get("message"))
                                .and_then(|m| m.as_str())
                                .unwrap_or("");

                            state.add_finding(severity, message);
                            emit_frame(&stdout, &state);
                        }
                        _ => {}
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

fn handle_key(state: &mut HeartbeatState, key: &str) -> bool {
    match key {
        "Enter" => {
            state.trigger_requested = true;
            true
        }
        "l" => {
            state.show_log = !state.show_log;
            true
        }
        _ => false,
    }
}

fn emit_frame(stdout: &io::Stdout, state: &HeartbeatState) {
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
// Heartbeat state
// ---------------------------------------------------------------------------

struct FindingEntry {
    severity: String,
    message: String,
}

struct HeartbeatState {
    status: String,
    last_duration_ms: i64,
    last_findings_count: i64,
    last_run_time: Option<chrono::DateTime<chrono::Utc>>,
    findings: Vec<FindingEntry>,
    trigger_requested: bool,
    show_log: bool,
    _width: u16,
    _height: u16,
}

impl HeartbeatState {
    fn new() -> Self {
        Self {
            status: "idle".to_string(),
            last_duration_ms: 0,
            last_findings_count: 0,
            last_run_time: None,
            findings: Vec::new(),
            trigger_requested: false,
            show_log: false,
            _width: 40,
            _height: 20,
        }
    }

    fn set_size(&mut self, width: u16, height: u16) {
        self._width = width;
        self._height = height;
    }

    fn update_status(&mut self, status: &str, duration_ms: i64, findings_count: i64) {
        self.status = status.to_string();
        self.last_duration_ms = duration_ms;
        self.last_findings_count = findings_count;
        self.last_run_time = Some(chrono::Utc::now());
        self.findings.clear();
    }

    fn add_finding(&mut self, severity: &str, message: &str) {
        self.findings.push(FindingEntry {
            severity: severity.to_string(),
            message: message.to_string(),
        });
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        let mut lines = Vec::new();

        // Status line
        let status_icon = match self.status.as_str() {
            "pass" => "\u{2764}",  // heart
            "warn" => "\u{26a0}",  // warning
            "alert" => "\u{2757}", // exclamation
            _ => "\u{2764}",
        };

        let last_time = self
            .last_run_time
            .map(|t| t.format("%H:%M").to_string())
            .unwrap_or_else(|| "never".to_string());

        let status_style = match self.status.as_str() {
            "pass" => "green",
            "warn" => "yellow",
            "alert" => "red",
            _ => "dim",
        };

        lines.push(FrameLine {
            text: format!(
                "{status_icon} Heartbeat  Last: {last_time} ({status})  {dur}ms",
                status = self.status.to_uppercase(),
                dur = self.last_duration_ms,
            ),
            icon: None,
            style: Some(status_style.to_string()),
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        });

        // Recent findings
        for finding in &self.findings {
            let style = match finding.severity.to_uppercase().as_str() {
                "PASS" => "green",
                "WARN" => "yellow",
                "ALERT" => "red",
                _ => "white",
            };
            lines.push(FrameLine {
                text: format!("  {}  {}", finding.severity.to_uppercase(), finding.message),
                icon: None,
                style: Some(style.to_string()),
                indent: Some(2),
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        }

        if self.findings.is_empty() && self.last_run_time.is_some() {
            lines.push(FrameLine {
                text: format!("  No findings ({} total)", self.last_findings_count),
                icon: None,
                style: Some("dim".to_string()),
                indent: Some(2),
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        }

        if self.last_run_time.is_none() {
            lines.push(FrameLine {
                text: "  Waiting for first heartbeat run...".to_string(),
                icon: None,
                style: Some("dim".to_string()),
                indent: Some(2),
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        }

        lines
    }
}
