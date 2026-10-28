//! Goal data types and state machine
//!
//! Goals form a tree structure with parent/child relationships.
//! Each goal has a status with enforced valid transitions.

use serde::{Deserialize, Serialize};

use crate::error::{FocusError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoalStatus {
    Active,
    Completed,
    Blocked,
    Cancelled,
}

impl GoalStatus {
    pub fn can_transition_to(&self, target: GoalStatus) -> bool {
        matches!(
            (self, target),
            (GoalStatus::Active, GoalStatus::Completed)
                | (GoalStatus::Active, GoalStatus::Blocked)
                | (GoalStatus::Active, GoalStatus::Cancelled)
                | (GoalStatus::Blocked, GoalStatus::Active)
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, GoalStatus::Completed | GoalStatus::Cancelled)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            GoalStatus::Active => "active",
            GoalStatus::Completed => "completed",
            GoalStatus::Blocked => "blocked",
            GoalStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "active" => Ok(GoalStatus::Active),
            "completed" => Ok(GoalStatus::Completed),
            "blocked" => Ok(GoalStatus::Blocked),
            "cancelled" => Ok(GoalStatus::Cancelled),
            _ => Err(FocusError::InvalidTransition {
                from: "unknown".to_string(),
                to: s.to_string(),
            }),
        }
    }
}

impl std::str::FromStr for GoalStatus {
    type Err = FocusError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl std::fmt::Display for GoalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: GoalStatus,
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Goal {
    pub fn new(title: impl Into<String>) -> Self {
        let now = now_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            description: None,
            status: GoalStatus::Active,
            parent_id: None,
            children: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn transition(&mut self, target: GoalStatus) -> Result<()> {
        if !self.status.can_transition_to(target) {
            return Err(FocusError::InvalidTransition {
                from: self.status.to_string(),
                to: target.to_string(),
            });
        }
        self.status = target;
        self.updated_at = now_rfc3339();
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.status == GoalStatus::Active
    }

    pub fn is_completed(&self) -> bool {
        self.status == GoalStatus::Completed
    }

    pub fn is_blocked(&self) -> bool {
        self.status == GoalStatus::Blocked
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1970 + now.as_secs() / 31_536_000,
        (now.as_secs() % 31_536_000) / 2_592_000 + 1,
        (now.as_secs() % 2_592_000) / 86400 + 1,
        (now.as_secs() % 86400) / 3600,
        (now.as_secs() % 3600) / 60,
        now.as_secs() % 60,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goal_new() {
        let goal = Goal::new("Test goal");
        assert_eq!(goal.title, "Test goal");
        assert_eq!(goal.status, GoalStatus::Active);
        assert!(goal.parent_id.is_none());
        assert!(goal.children.is_empty());
        assert!(goal.description.is_none());
        assert!(!goal.id.is_empty());
    }

    #[test]
    fn test_goal_builders() {
        let goal = Goal::new("Test")
            .with_id("g-1")
            .with_description("A test goal")
            .with_parent("g-0");

        assert_eq!(goal.id, "g-1");
        assert_eq!(goal.description.as_deref(), Some("A test goal"));
        assert_eq!(goal.parent_id.as_deref(), Some("g-0"));
    }

    #[test]
    fn test_valid_transitions() {
        assert!(GoalStatus::Active.can_transition_to(GoalStatus::Completed));
        assert!(GoalStatus::Active.can_transition_to(GoalStatus::Blocked));
        assert!(GoalStatus::Active.can_transition_to(GoalStatus::Cancelled));
        assert!(GoalStatus::Blocked.can_transition_to(GoalStatus::Active));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!GoalStatus::Completed.can_transition_to(GoalStatus::Active));
        assert!(!GoalStatus::Completed.can_transition_to(GoalStatus::Blocked));
        assert!(!GoalStatus::Cancelled.can_transition_to(GoalStatus::Active));
        assert!(!GoalStatus::Blocked.can_transition_to(GoalStatus::Completed));
        assert!(!GoalStatus::Blocked.can_transition_to(GoalStatus::Cancelled));
        assert!(!GoalStatus::Active.can_transition_to(GoalStatus::Active));
    }

    #[test]
    fn test_goal_transition_success() {
        let mut goal = Goal::new("Test").with_id("g-1");
        assert!(goal.transition(GoalStatus::Blocked).is_ok());
        assert_eq!(goal.status, GoalStatus::Blocked);

        assert!(goal.transition(GoalStatus::Active).is_ok());
        assert_eq!(goal.status, GoalStatus::Active);

        assert!(goal.transition(GoalStatus::Completed).is_ok());
        assert_eq!(goal.status, GoalStatus::Completed);
    }

    #[test]
    fn test_goal_transition_error() {
        let mut goal = Goal::new("Test").with_id("g-1");
        goal.status = GoalStatus::Completed;

        let result = goal.transition(GoalStatus::Active);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid transition"));
    }

    #[test]
    fn test_status_terminal() {
        assert!(GoalStatus::Completed.is_terminal());
        assert!(GoalStatus::Cancelled.is_terminal());
        assert!(!GoalStatus::Active.is_terminal());
        assert!(!GoalStatus::Blocked.is_terminal());
    }

    #[test]
    fn test_status_as_str() {
        assert_eq!(GoalStatus::Active.as_str(), "active");
        assert_eq!(GoalStatus::Completed.as_str(), "completed");
        assert_eq!(GoalStatus::Blocked.as_str(), "blocked");
        assert_eq!(GoalStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn test_status_from_str() {
        assert_eq!(GoalStatus::parse("active").unwrap(), GoalStatus::Active);
        assert_eq!(
            GoalStatus::parse("completed").unwrap(),
            GoalStatus::Completed
        );
        assert_eq!(GoalStatus::parse("blocked").unwrap(), GoalStatus::Blocked);
        assert_eq!(
            GoalStatus::parse("cancelled").unwrap(),
            GoalStatus::Cancelled
        );
        assert!(GoalStatus::parse("invalid").is_err());
    }

    #[test]
    fn test_status_display() {
        assert_eq!(GoalStatus::Active.to_string(), "active");
        assert_eq!(GoalStatus::Completed.to_string(), "completed");
    }

    #[test]
    fn test_goal_predicates() {
        let mut goal = Goal::new("Test");
        assert!(goal.is_active());
        assert!(!goal.is_completed());
        assert!(!goal.is_blocked());
        assert!(goal.is_leaf());

        goal.children.push("child-1".to_string());
        assert!(!goal.is_leaf());
    }

    #[test]
    fn test_status_serde_roundtrip() {
        let status = GoalStatus::Blocked;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"blocked\"");

        let parsed: GoalStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn test_goal_serde_roundtrip() {
        let goal = Goal::new("Test goal")
            .with_id("g-1")
            .with_description("A test");

        let json = serde_json::to_string(&goal).unwrap();
        let parsed: Goal = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "g-1");
        assert_eq!(parsed.title, "Test goal");
        assert_eq!(parsed.status, GoalStatus::Active);
    }
}
