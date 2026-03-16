//! History record data types and builder.

use chrono::Utc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

/// Origin of a command execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Executor {
    /// Interactive user at the terminal.
    User,
    /// The colony-agent subsystem.
    ColonyAgent,
    /// An external AI agent detected via environment variables.
    External(String),
}

impl Executor {
    /// Return a stable string representation for storage.
    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::ColonyAgent => "colony-agent",
            Self::External(name) => name,
        }
    }
}

// ---------------------------------------------------------------------------
// RiskLevel
// ---------------------------------------------------------------------------

/// Risk classification for agent-originated commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    Critical,
    High,
    Medium,
    Low,
    Safe,
}

impl RiskLevel {
    /// Return a stable string representation for storage.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Safe => "safe",
        }
    }
}

// ---------------------------------------------------------------------------
// HistoryRecord
// ---------------------------------------------------------------------------

/// A single command history entry with full metadata.
#[derive(Debug, Clone)]
pub struct HistoryRecord {
    pub id: String,
    pub command: String,
    pub command_redacted: String,
    pub cwd: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub executor: Executor,
    pub agent_session_id: Option<String>,
    pub agent_goal: Option<String>,
    pub risk_level: Option<RiskLevel>,
    pub session_id: String,
    pub git_branch: Option<String>,
    pub hostname: String,
    pub timestamp: i64,
    pub hidden: bool,
    pub bookmarked: bool,
    pub note: Option<String>,
    pub tags: Option<String>,
}

impl HistoryRecord {
    /// Create a new builder for constructing a `HistoryRecord`.
    pub fn builder() -> HistoryRecordBuilder {
        HistoryRecordBuilder::default()
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Error returned when a required builder field is missing.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),
}

/// Builder for `HistoryRecord` with required-field validation.
#[derive(Debug, Default)]
pub struct HistoryRecordBuilder {
    command: Option<String>,
    command_redacted: Option<String>,
    cwd: Option<String>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    executor: Option<Executor>,
    agent_session_id: Option<String>,
    agent_goal: Option<String>,
    risk_level: Option<RiskLevel>,
    session_id: Option<String>,
    git_branch: Option<String>,
    hostname: Option<String>,
}

impl HistoryRecordBuilder {
    pub fn command(mut self, cmd: impl Into<String>) -> Self {
        self.command = Some(cmd.into());
        self
    }

    pub fn command_redacted(mut self, cmd: impl Into<String>) -> Self {
        self.command_redacted = Some(cmd.into());
        self
    }

    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    pub fn duration_ms(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    pub fn executor(mut self, executor: Executor) -> Self {
        self.executor = Some(executor);
        self
    }

    pub fn agent_session_id(mut self, id: impl Into<String>) -> Self {
        self.agent_session_id = Some(id.into());
        self
    }

    pub fn agent_goal(mut self, goal: impl Into<String>) -> Self {
        self.agent_goal = Some(goal.into());
        self
    }

    pub fn risk_level(mut self, level: RiskLevel) -> Self {
        self.risk_level = Some(level);
        self
    }

    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    pub fn git_branch(mut self, branch: impl Into<String>) -> Self {
        self.git_branch = Some(branch.into());
        self
    }

    pub fn hostname(mut self, host: impl Into<String>) -> Self {
        self.hostname = Some(host.into());
        self
    }

    /// Build the `HistoryRecord`, returning an error if required fields are
    /// missing.
    pub fn build(self) -> Result<HistoryRecord, RecordError> {
        let command = self.command.ok_or(RecordError::MissingField("command"))?;
        let cwd = self.cwd.ok_or(RecordError::MissingField("cwd"))?;
        let exit_code = self
            .exit_code
            .ok_or(RecordError::MissingField("exit_code"))?;
        let duration_ms = self
            .duration_ms
            .ok_or(RecordError::MissingField("duration_ms"))?;
        let session_id = self
            .session_id
            .ok_or(RecordError::MissingField("session_id"))?;

        let command_redacted = self.command_redacted.unwrap_or_else(|| command.clone());
        let hostname = self.hostname.unwrap_or_else(|| {
            nix::unistd::gethostname()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string())
        });

        Ok(HistoryRecord {
            id: Uuid::new_v4().to_string(),
            command,
            command_redacted,
            cwd,
            exit_code,
            duration_ms,
            executor: self.executor.unwrap_or(Executor::User),
            agent_session_id: self.agent_session_id,
            agent_goal: self.agent_goal,
            risk_level: self.risk_level,
            session_id,
            git_branch: self.git_branch,
            hostname,
            timestamp: Utc::now().timestamp_millis(),
            hidden: false,
            bookmarked: false,
            note: None,
            tags: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_builder_minimal() {
        let record = HistoryRecord::builder()
            .command("ls -la")
            .cwd("/home/user")
            .exit_code(0)
            .duration_ms(42)
            .session_id("sess-123")
            .build()
            .unwrap();

        assert_eq!(record.command, "ls -la");
        assert_eq!(record.cwd, "/home/user");
        assert_eq!(record.exit_code, 0);
        assert_eq!(record.executor, Executor::User);
        assert!(!record.id.is_empty());
    }

    #[test]
    fn test_record_builder_with_agent() {
        let record = HistoryRecord::builder()
            .command("rm -rf build/")
            .cwd("/project")
            .exit_code(0)
            .duration_ms(100)
            .session_id("sess-456")
            .executor(Executor::ColonyAgent)
            .agent_session_id("agent-789")
            .agent_goal("clean build artifacts")
            .build()
            .unwrap();

        assert_eq!(record.executor, Executor::ColonyAgent);
        assert_eq!(record.agent_session_id.as_deref(), Some("agent-789"));
    }

    #[test]
    fn test_record_builder_missing_required_field() {
        let result = HistoryRecord::builder().command("ls").build();
        assert!(result.is_err());
    }

    #[test]
    fn test_executor_as_str() {
        assert_eq!(Executor::User.as_str(), "user");
        assert_eq!(Executor::ColonyAgent.as_str(), "colony-agent");
        assert_eq!(
            Executor::External("claude-code".to_string()).as_str(),
            "claude-code"
        );
    }

    #[test]
    fn test_risk_level_as_str() {
        assert_eq!(RiskLevel::Critical.as_str(), "critical");
        assert_eq!(RiskLevel::High.as_str(), "high");
        assert_eq!(RiskLevel::Medium.as_str(), "medium");
        assert_eq!(RiskLevel::Low.as_str(), "low");
        assert_eq!(RiskLevel::Safe.as_str(), "safe");
    }

    #[test]
    fn test_record_builder_with_redacted() {
        let record = HistoryRecord::builder()
            .command("curl -H 'Auth: secret' https://api.com")
            .command_redacted("curl -H 'Auth: ***' https://api.com")
            .cwd("/tmp")
            .exit_code(0)
            .duration_ms(500)
            .session_id("sess")
            .build()
            .unwrap();

        assert_eq!(
            record.command_redacted,
            "curl -H 'Auth: ***' https://api.com"
        );
        assert_ne!(record.command, record.command_redacted);
    }

    #[test]
    fn test_record_builder_defaults_redacted_to_command() {
        let record = HistoryRecord::builder()
            .command("ls -la")
            .cwd("/tmp")
            .exit_code(0)
            .duration_ms(10)
            .session_id("sess")
            .build()
            .unwrap();

        assert_eq!(record.command, record.command_redacted);
    }
}
