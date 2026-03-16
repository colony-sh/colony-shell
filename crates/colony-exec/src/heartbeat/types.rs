//! Type definitions for the Heartbeat Scheduler (F080).
//!
//! These types represent the parsed HEARTBEAT.md configuration, checklist
//! items, findings from agent runs, and the overall run record stored in
//! the shared reflection database.

use serde::{Deserialize, Serialize};

/// Security mode controlling which tools the heartbeat agent can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecurityMode {
    /// Read-only: file_read, shell_exec (stdout capture), search
    #[default]
    Notify,
    /// Read + write: adds file_write
    Active,
    /// Unrestricted: full shell_exec
    Full,
}

impl SecurityMode {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "active" => Self::Active,
            "full" => Self::Full,
            _ => Self::Notify,
        }
    }

    /// Return the tool names allowed for this security mode.
    pub fn allowed_tools(&self) -> &[&str] {
        match self {
            Self::Notify => &["file_read", "shell_exec", "search"],
            Self::Active => &["file_read", "shell_exec", "search", "file_write"],
            Self::Full => &["file_read", "shell_exec", "search", "file_write"],
        }
    }
}

impl std::fmt::Display for SecurityMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Notify => "notify",
            Self::Active => "active",
            Self::Full => "full",
        };
        f.write_str(s)
    }
}

/// Parsed HEARTBEAT.md frontmatter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    pub interval_secs: u64,
    pub mode: SecurityMode,
    pub notify: bool,
    pub timeout_secs: u64,
    pub max_tool_calls: u32,
    pub token_budget: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_secs: 1800, // 30m
            mode: SecurityMode::Notify,
            notify: true,
            timeout_secs: 60,
            max_tool_calls: 20,
            token_budget: 10_000,
        }
    }
}

/// A single checklist item from HEARTBEAT.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub text: String,
    pub checked: bool,
}

/// Severity of a heartbeat finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Pass,
    Warn,
    Alert,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Alert => "ALERT",
        };
        f.write_str(s)
    }
}

/// A single finding extracted from agent output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

/// Overall status of a heartbeat run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeartbeatStatus {
    Pass,
    Warn,
    Alert,
    Unstructured,
    Timeout,
    Error,
}

impl std::fmt::Display for HeartbeatStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Alert => "alert",
            Self::Unstructured => "unstructured",
            Self::Timeout => "timeout",
            Self::Error => "error",
        };
        f.write_str(s)
    }
}

impl HeartbeatStatus {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "pass" => Self::Pass,
            "warn" => Self::Warn,
            "alert" => Self::Alert,
            "unstructured" => Self::Unstructured,
            "timeout" => Self::Timeout,
            "error" => Self::Error,
            _ => Self::Unstructured,
        }
    }
}

/// Record of a single heartbeat run, stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRun {
    pub id: String,
    pub timestamp: i64,
    pub duration_ms: u64,
    pub status: HeartbeatStatus,
    pub findings: Vec<Finding>,
    pub raw_output: String,
    pub checklist_path: String,
    pub interval_secs: u64,
}

/// Errors that can occur during heartbeat operations.
#[derive(Debug, thiserror::Error)]
pub enum HeartbeatError {
    #[error("no HEARTBEAT.md found")]
    NotFound,

    #[error("no LLM provider available: {0}")]
    NoProvider(String),

    #[error("scheduler already running")]
    AlreadyRunning,

    #[error("scheduler not running")]
    NotRunning,

    #[error("database error: {0}")]
    Database(String),

    #[error("agent error: {0}")]
    Agent(String),

    #[error("timeout after {0}s")]
    Timeout(u64),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
