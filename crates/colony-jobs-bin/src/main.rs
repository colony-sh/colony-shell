//! jobs-panel: a panel binary that displays background job status.
//!
//! Emits frames to stdout following the panel protocol.
//! Subscribes to `job_change` tuples to track job state.

use colony_panel::protocol::{
    FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME,
};
use std::io::{self, BufRead, Write};

fn main() {
    let mut state = JobsState::default();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["job_change".to_string()],
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
            InputEvent::Tuple { ref data } => {
                if let Some(key) = data.get("key").and_then(|k| k.as_str()) {
                    if key == "job_change" {
                        state.apply_tuple(data);
                        emit_frame(&stdout, &state);
                    }
                }
            }
            InputEvent::Resize { width, height } => {
                state.set_size(width, height);
                emit_frame(&stdout, &state);
            }
            InputEvent::Shutdown => break,
            InputEvent::Key { .. } | InputEvent::Focus { .. } => {}
        }
    }
}

fn emit_frame(stdout: &io::Stdout, state: &JobsState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in state.render_lines() {
        writeln!(out, "{}", serde_json::to_string(&line).expect("frame line must serialize")).ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

// ---------------------------------------------------------------------------
// Job tracking state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct JobEntry {
    number: u32,
    command: String,
    status: JobStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobStatus {
    Running,
    Stopped,
}

impl JobStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Stopped => "Stopped",
        }
    }

    fn style(self) -> &'static str {
        match self {
            Self::Running => "green",
            Self::Stopped => "yellow",
        }
    }
}

#[derive(Debug, Default)]
struct JobsState {
    jobs: Vec<JobEntry>,
    _width: u16,
    _height: u16,
}

impl JobsState {
    fn set_size(&mut self, width: u16, height: u16) {
        self._width = width;
        self._height = height;
    }

    /// Apply a `job_change` tuple event to update tracked jobs.
    ///
    /// Expected tuple shape:
    /// ```json
    /// {
    ///   "key": "job_change",
    ///   "value": {
    ///     "number": 1,
    ///     "command": "sleep 100",
    ///     "status": "running" | "stopped" | "done"
    ///   }
    /// }
    /// ```
    fn apply_tuple(&mut self, data: &serde_json::Value) {
        let Some(value) = data.get("value") else {
            return;
        };
        let Some(number) = value.get("number").and_then(|n| n.as_u64()) else {
            return;
        };
        let number = number as u32;
        let status_str = value
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("running");
        let command = value
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("???")
            .to_string();

        match status_str {
            "done" | "exited" => {
                self.jobs.retain(|j| j.number != number);
            }
            _ => {
                let status = if status_str == "stopped" {
                    JobStatus::Stopped
                } else {
                    JobStatus::Running
                };

                if let Some(existing) = self.jobs.iter_mut().find(|j| j.number == number) {
                    existing.status = status;
                    existing.command = command;
                } else {
                    self.jobs.push(JobEntry {
                        number,
                        command,
                        status,
                    });
                }
            }
        }

        self.jobs.sort_by_key(|j| j.number);
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        let mut lines = Vec::new();

        // Header
        lines.push(FrameLine {
            text: "Jobs".to_string(),
            icon: None,
            style: Some("bold".to_string()),
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        });

        if self.jobs.is_empty() {
            lines.push(FrameLine {
                text: "[no jobs]".to_string(),
                icon: None,
                style: Some("dim".to_string()),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        } else {
            for job in &self.jobs {
                lines.push(FrameLine {
                    text: format!("[{}] {} {}", job.number, job.status.label(), job.command),
                    icon: None,
                    style: Some(job.status.style().to_string()),
                    indent: None,
                    action: None,
                    expandable: None,
                    expanded: None,
                    selected: None,
                });
            }
        }

        lines
    }
}
