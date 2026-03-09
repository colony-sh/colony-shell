use std::collections::VecDeque;
use std::path::Path;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use chrono::{Local, Timelike};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Categories of observations the reflection system can record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationCategory {
    Command,
    Error,
    AgentChat,
    FileActivity,
    ContextUsage,
}

impl ObservationCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Error => "error",
            Self::AgentChat => "agent_chat",
            Self::FileActivity => "file_activity",
            Self::ContextUsage => "context_usage",
        }
    }
}

/// Coarse time-of-day buckets for pattern detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    Night,
}

impl TimeOfDay {
    /// Determine the current time-of-day bucket based on local clock.
    pub fn now() -> Self {
        let hour = Local::now().hour();
        match hour {
            6..=11 => Self::Morning,
            12..=16 => Self::Afternoon,
            17..=20 => Self::Evening,
            _ => Self::Night,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Morning => "morning",
            Self::Afternoon => "afternoon",
            Self::Evening => "evening",
            Self::Night => "night",
        }
    }
}

/// Return the current day of the week as a lowercase string.
pub fn day_of_week_str() -> String {
    Local::now().format("%A").to_string().to_lowercase()
}

/// Contextual metadata attached to every observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub session_id: String,
    pub cwd: String,
    pub git_branch: Option<String>,
    pub command_count: u64,
    pub time_since_last_ms: u64,
    pub time_of_day: String,
    pub day_of_week: String,
    pub project: Option<String>,
    pub focus_duration_ms: u64,
    pub energy_level: f64,
}

/// Tracks session state and builds `SessionContext` snapshots.
pub struct SessionTracker {
    session_id: String,
    start_time: Instant,
    command_count: u64,
    last_observation: Option<Instant>,
    error_window: VecDeque<Instant>,
    inter_command_times: VecDeque<u64>,
}

const ERROR_WINDOW_SIZE: usize = 20;
const INTER_CMD_WINDOW_SIZE: usize = 20;

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            start_time: Instant::now(),
            command_count: 0,
            last_observation: None,
            error_window: VecDeque::with_capacity(ERROR_WINDOW_SIZE),
            inter_command_times: VecDeque::with_capacity(INTER_CMD_WINDOW_SIZE),
        }
    }

    /// Build a snapshot of the current session context.
    pub fn build_context(&mut self) -> SessionContext {
        let now = Instant::now();
        let time_since_last_ms = self
            .last_observation
            .map(|last| now.duration_since(last).as_millis() as u64)
            .unwrap_or(0);

        if let Some(last) = self.last_observation {
            let delta = now.duration_since(last).as_millis() as u64;
            if self.inter_command_times.len() >= INTER_CMD_WINDOW_SIZE {
                self.inter_command_times.pop_front();
            }
            self.inter_command_times.push_back(delta);
        }

        self.command_count += 1;
        self.last_observation = Some(now);

        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        SessionContext {
            session_id: self.session_id.clone(),
            cwd: cwd.clone(),
            git_branch: detect_git_branch(),
            command_count: self.command_count,
            time_since_last_ms,
            time_of_day: TimeOfDay::now().as_str().to_string(),
            day_of_week: day_of_week_str(),
            project: detect_project_name(&cwd),
            focus_duration_ms: now.duration_since(self.start_time).as_millis() as u64,
            energy_level: self.compute_energy(),
        }
    }

    /// Record an error occurrence for energy-level tracking.
    pub fn record_error(&mut self) {
        if self.error_window.len() >= ERROR_WINDOW_SIZE {
            self.error_window.pop_front();
        }
        self.error_window.push_back(Instant::now());
    }

    /// Compute an energy level in [0.0, 1.0] based on error rate and command cadence.
    ///
    /// Energy = 1.0 - (error_rate * 0.5 + slowness * 0.5), clamped to [0, 1].
    /// - error_rate = errors in window / window size
    /// - slowness = avg inter-command time / 30_000ms, capped at 1.0
    fn compute_energy(&self) -> f64 {
        let error_rate = self.error_window.len() as f64 / ERROR_WINDOW_SIZE as f64;

        let slowness = if self.inter_command_times.is_empty() {
            0.0
        } else {
            let avg: f64 =
                self.inter_command_times.iter().sum::<u64>() as f64
                    / self.inter_command_times.len() as f64;
            (avg / 30_000.0).min(1.0)
        };

        (1.0 - (error_rate * 0.5 + slowness * 0.5)).clamp(0.0, 1.0)
    }
}

impl Default for SessionTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Category-specific data structs
// ---------------------------------------------------------------------------

/// Data for a command observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandData {
    pub input: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub pipeline_length: usize,
}

/// Data for an error observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    pub message: String,
    pub kind: String,
    pub recoverable: bool,
}

/// Data for an agent chat observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentChatData {
    pub provider: String,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub duration_ms: u64,
}

/// Data for a file-activity observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileActivityData {
    pub path: String,
    pub action: String,
    pub language: Option<String>,
}

/// Data for a context-usage observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUsageData {
    pub channel: String,
    pub bytes: u64,
    pub direction: String,
}

// ---------------------------------------------------------------------------
// Observation
// ---------------------------------------------------------------------------

/// A single observation recorded by the reflection system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub timestamp: u64,
    pub category: ObservationCategory,
    pub data: serde_json::Value,
    pub context: SessionContext,
}

impl Observation {
    /// Create a new observation with the given category, data, and context.
    pub fn new(
        category: ObservationCategory,
        data: serde_json::Value,
        context: SessionContext,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX epoch")
            .as_millis() as u64;

        Self {
            id: Uuid::new_v4().to_string(),
            timestamp,
            category,
            data,
            context,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Detect the current git branch by running `git rev-parse`.
pub fn detect_git_branch() -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let branch = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if branch.is_empty() { None } else { Some(branch) }
        })
}

/// Detect a project name from the given working directory path.
///
/// Uses the name of the deepest directory component as the project name.
pub fn detect_project_name(cwd: &str) -> Option<String> {
    Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_of_day() {
        // Verify TimeOfDay::now() doesn't panic and returns a valid variant.
        let tod = TimeOfDay::now();
        assert!(!tod.as_str().is_empty());
    }

    #[test]
    fn test_session_tracker_builds_context() {
        let mut tracker = SessionTracker::new();
        let ctx = tracker.build_context();

        assert!(!ctx.session_id.is_empty());
        assert_eq!(ctx.command_count, 1);
        assert!((0.0..=1.0).contains(&ctx.energy_level));
    }

    #[test]
    fn test_session_tracker_energy_degrades_with_errors() {
        let mut tracker = SessionTracker::new();
        let baseline = tracker.build_context();
        let baseline_energy = baseline.energy_level;

        for _ in 0..10 {
            tracker.record_error();
        }

        let after = tracker.build_context();
        assert!(
            after.energy_level < baseline_energy,
            "energy should degrade after errors: {} vs {}",
            after.energy_level,
            baseline_energy,
        );
    }

    #[test]
    fn test_observation_new() {
        let mut tracker = SessionTracker::new();
        let ctx = tracker.build_context();
        let obs = Observation::new(
            ObservationCategory::Command,
            serde_json::json!({"input": "ls"}),
            ctx,
        );

        assert!(!obs.id.is_empty());
        assert_eq!(obs.category, ObservationCategory::Command);
    }
}
