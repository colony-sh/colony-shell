//! Core types for the goal tracking system

use serde::{Deserialize, Serialize};

/// Unique identifier for a goal (auto-incrementing within a tree)
pub type GoalId = u64;

/// Status of a goal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Cancelled,
}

impl GoalStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for GoalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single goal in the tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: GoalId,
    pub title: String,
    pub status: GoalStatus,
    pub children: Vec<Goal>,
    /// Progress percentage (0-100), derived from children or set manually
    pub progress: u8,
    /// Blocker descriptions
    pub blockers: Vec<String>,
    /// Timestamp when created (ISO 8601)
    pub created_at: String,
    /// Timestamp when last updated (ISO 8601)
    pub updated_at: String,
    /// Optional notes or description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Goal {
    /// Create a new goal with the given title
    pub fn new(id: GoalId, title: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            title: title.into(),
            status: GoalStatus::Pending,
            children: Vec::new(),
            progress: 0,
            blockers: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            notes: None,
        }
    }

    /// Recalculate progress from children
    pub fn recalculate_progress(&mut self) {
        if self.children.is_empty() {
            return;
        }

        let total = self.children.len() as f64;
        let completed = self
            .children
            .iter()
            .filter(|c| c.status == GoalStatus::Completed)
            .count() as f64;
        let partial: f64 = self
            .children
            .iter()
            .filter(|c| !c.status.is_terminal())
            .map(|c| c.progress as f64 / 100.0)
            .sum();

        self.progress = ((completed + partial) / total * 100.0).round() as u8;
    }

    /// Find a child goal by ID (recursive)
    pub fn find_mut(&mut self, id: GoalId) -> Option<&mut Goal> {
        if self.id == id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_mut(id) {
                return Some(found);
            }
        }
        None
    }

    /// Find a goal by ID (recursive, immutable)
    pub fn find(&self, id: GoalId) -> Option<&Goal> {
        if self.id == id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find(id) {
                return Some(found);
            }
        }
        None
    }

    /// Collect all goals in the tree (flattened, DFS order)
    pub fn flatten(&self) -> Vec<&Goal> {
        let mut result = vec![self];
        for child in &self.children {
            result.extend(child.flatten());
        }
        result
    }
}

/// A snapshot of goal state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub timestamp: String,
    pub label: String,
    pub goals: Vec<Goal>,
}

/// Summary of a session's accomplishments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub started_at: String,
    pub ended_at: String,
    pub goals_completed: Vec<GoalId>,
    pub goals_progressed: Vec<GoalId>,
    pub notes: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goal_creation() {
        let goal = Goal::new(1, "Implement feature X");
        assert_eq!(goal.id, 1);
        assert_eq!(goal.title, "Implement feature X");
        assert_eq!(goal.status, GoalStatus::Pending);
        assert_eq!(goal.progress, 0);
        assert!(goal.children.is_empty());
        assert!(goal.blockers.is_empty());
    }

    #[test]
    fn test_status_display() {
        assert_eq!(GoalStatus::Pending.to_string(), "pending");
        assert_eq!(GoalStatus::InProgress.to_string(), "in_progress");
        assert_eq!(GoalStatus::Completed.to_string(), "completed");
        assert_eq!(GoalStatus::Blocked.to_string(), "blocked");
        assert_eq!(GoalStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_status_terminal() {
        assert!(!GoalStatus::Pending.is_terminal());
        assert!(!GoalStatus::InProgress.is_terminal());
        assert!(GoalStatus::Completed.is_terminal());
        assert!(GoalStatus::Cancelled.is_terminal());
        assert!(!GoalStatus::Blocked.is_terminal());
    }

    #[test]
    fn test_recalculate_progress() {
        let mut parent = Goal::new(1, "Parent");
        let mut child_a = Goal::new(2, "Child A");
        child_a.status = GoalStatus::Completed;
        child_a.progress = 100;

        let child_b = Goal::new(3, "Child B");

        parent.children.push(child_a);
        parent.children.push(child_b);
        parent.recalculate_progress();

        assert_eq!(parent.progress, 50);
    }

    #[test]
    fn test_find_recursive() {
        let mut root = Goal::new(1, "Root");
        let mut sub = Goal::new(2, "Sub");
        sub.children.push(Goal::new(3, "Leaf"));
        root.children.push(sub);

        assert!(root.find(1).is_some());
        assert!(root.find(2).is_some());
        assert!(root.find(3).is_some());
        assert!(root.find(99).is_none());
    }

    #[test]
    fn test_flatten() {
        let mut root = Goal::new(1, "Root");
        root.children.push(Goal::new(2, "A"));
        root.children.push(Goal::new(3, "B"));

        let flat = root.flatten();
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].id, 1);
        assert_eq!(flat[1].id, 2);
        assert_eq!(flat[2].id, 3);
    }
}
