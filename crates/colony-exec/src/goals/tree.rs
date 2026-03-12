//! GoalTree — persistent goal tree stored per-project
//!
//! Goals are stored in `~/.colony/goals/<project-hash>.json` where
//! the project hash is SHA-256 of the absolute path of the project root.

use super::types::{Checkpoint, Goal, GoalId, GoalStatus, SessionSummary};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum GoalError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("goal not found: {0}")]
    NotFound(GoalId),

    #[error("invalid operation: {0}")]
    InvalidOperation(String),
}

type Result<T> = std::result::Result<T, GoalError>;

/// Persistent on-disk representation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoalFile {
    project_path: String,
    next_id: GoalId,
    goals: Vec<Goal>,
    checkpoints: Vec<Checkpoint>,
    sessions: Vec<SessionSummary>,
}

/// Goal tree with file-backed persistence
pub struct GoalTree {
    file_path: PathBuf,
    project_path: PathBuf,
    data: GoalFile,
}

impl GoalTree {
    /// Load or create a goal tree for the given project directory
    pub fn load(project_path: &Path) -> Result<Self> {
        let file_path = Self::storage_path(project_path)?;
        let data = if file_path.exists() {
            let content = std::fs::read_to_string(&file_path)?;
            serde_json::from_str(&content)?
        } else {
            GoalFile {
                project_path: project_path.display().to_string(),
                next_id: 1,
                goals: Vec::new(),
                checkpoints: Vec::new(),
                sessions: Vec::new(),
            }
        };

        Ok(Self {
            file_path,
            project_path: project_path.to_path_buf(),
            data,
        })
    }

    /// Get the storage file path for a project
    fn storage_path(project_path: &Path) -> Result<PathBuf> {
        let hash = Self::project_hash(project_path);
        let home = dirs::home_dir().ok_or_else(|| {
            GoalError::InvalidOperation("home directory not available".to_string())
        })?;
        let goals_dir = home.join(".colony").join("goals");
        Ok(goals_dir.join(format!("{}.json", hash)))
    }

    /// SHA-256 hash of the project path (first 16 hex chars)
    fn project_hash(project_path: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(project_path.display().to_string().as_bytes());
        let result = hasher.finalize();
        hex::encode(&result[..8])
    }

    /// Save current state to disk
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.data)?;
        std::fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// Get all top-level goals
    pub fn goals(&self) -> &[Goal] {
        &self.data.goals
    }

    /// Get the project path
    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    /// Add a new top-level goal
    pub fn add(&mut self, title: impl Into<String>) -> GoalId {
        let id = self.data.next_id;
        self.data.next_id += 1;
        self.data.goals.push(Goal::new(id, title));
        id
    }

    /// Add a child goal under a parent
    pub fn add_child(&mut self, parent_id: GoalId, title: impl Into<String>) -> Result<GoalId> {
        let id = self.data.next_id;
        self.data.next_id += 1;
        let child = Goal::new(id, title);

        let parent = self
            .find_goal_mut(parent_id)
            .ok_or(GoalError::NotFound(parent_id))?;

        parent.children.push(child);
        Ok(id)
    }

    /// Update a goal's status
    pub fn update_status(&mut self, id: GoalId, status: GoalStatus) -> Result<()> {
        let goal = self
            .find_goal_mut(id)
            .ok_or(GoalError::NotFound(id))?;

        goal.status = status;
        goal.updated_at = chrono::Utc::now().to_rfc3339();

        if status == GoalStatus::Completed {
            goal.progress = 100;
        }

        Ok(())
    }

    /// Mark a goal as complete
    pub fn complete(&mut self, id: GoalId) -> Result<()> {
        self.update_status(id, GoalStatus::Completed)
    }

    /// Add a blocker to a goal.
    ///
    /// **Side-effect:** This method unconditionally sets `goal.status` to
    /// [`GoalStatus::Blocked`], overwriting whatever status the goal previously
    /// held (e.g. `InProgress`). Callers that want to preserve the existing
    /// status must save and restore it themselves.
    pub fn add_blocker(&mut self, id: GoalId, blocker: impl Into<String>) -> Result<()> {
        let goal = self
            .find_goal_mut(id)
            .ok_or(GoalError::NotFound(id))?;

        goal.blockers.push(blocker.into());
        goal.status = GoalStatus::Blocked;
        goal.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(())
    }

    /// Create a checkpoint (snapshot of current state)
    pub fn checkpoint(&mut self, label: impl Into<String>) {
        let cp = Checkpoint {
            timestamp: chrono::Utc::now().to_rfc3339(),
            label: label.into(),
            goals: self.data.goals.clone(),
        };
        self.data.checkpoints.push(cp);
    }

    /// Get all checkpoints
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.data.checkpoints
    }

    /// Record a session summary
    pub fn record_session(
        &mut self,
        started_at: impl Into<String>,
        notes: impl Into<String>,
        completed: Vec<GoalId>,
        progressed: Vec<GoalId>,
    ) {
        let summary = SessionSummary {
            started_at: started_at.into(),
            ended_at: chrono::Utc::now().to_rfc3339(),
            goals_completed: completed,
            goals_progressed: progressed,
            notes: notes.into(),
        };
        self.data.sessions.push(summary);
    }

    /// Get all session summaries
    pub fn sessions(&self) -> &[SessionSummary] {
        &self.data.sessions
    }

    /// Find a goal by ID across all trees
    pub fn find_goal(&self, id: GoalId) -> Option<&Goal> {
        for goal in &self.data.goals {
            if let Some(found) = goal.find(id) {
                return Some(found);
            }
        }
        None
    }

    /// Find a goal by ID (mutable) across all trees
    fn find_goal_mut(&mut self, id: GoalId) -> Option<&mut Goal> {
        for goal in &mut self.data.goals {
            if let Some(found) = goal.find_mut(id) {
                return Some(found);
            }
        }
        None
    }

    /// List all goals flattened (DFS order)
    pub fn all_goals(&self) -> Vec<&Goal> {
        let mut result = Vec::new();
        for goal in &self.data.goals {
            result.extend(goal.flatten());
        }
        result
    }

    /// Format goals as a human-readable tree
    pub fn format_tree(&self) -> String {
        let mut output = String::new();
        for goal in &self.data.goals {
            Self::format_goal(&mut output, goal, 0);
        }
        output
    }

    fn format_goal(output: &mut String, goal: &Goal, depth: usize) {
        let indent = "  ".repeat(depth);
        let status_icon = match goal.status {
            GoalStatus::Pending => "[ ]",
            GoalStatus::InProgress => "[~]",
            GoalStatus::Completed => "[x]",
            GoalStatus::Blocked => "[!]",
            GoalStatus::Cancelled => "[-]",
        };

        output.push_str(&format!(
            "{}{} #{}: {} ({}%)\n",
            indent, status_icon, goal.id, goal.title, goal.progress
        ));

        for blocker in &goal.blockers {
            output.push_str(&format!("{}  BLOCKED: {}\n", indent, blocker));
        }

        for child in &goal.children {
            Self::format_goal(output, child, depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project() -> (tempfile::TempDir, GoalTree) {
        let dir = tempfile::tempdir().unwrap();
        let tree = GoalTree::load(dir.path()).unwrap();
        (dir, tree)
    }

    #[test]
    fn test_add_goal() {
        let (_dir, mut tree) = temp_project();
        let id = tree.add("Build feature A");
        assert_eq!(id, 1);
        assert_eq!(tree.goals().len(), 1);
        assert_eq!(tree.goals()[0].title, "Build feature A");
    }

    #[test]
    fn test_add_child() {
        let (_dir, mut tree) = temp_project();
        let parent = tree.add("Parent");
        let child = tree.add_child(parent, "Child").unwrap();

        assert_eq!(child, 2);
        let parent_goal = tree.find_goal(parent).unwrap();
        assert_eq!(parent_goal.children.len(), 1);
        assert_eq!(parent_goal.children[0].title, "Child");
    }

    #[test]
    fn test_complete_goal() {
        let (_dir, mut tree) = temp_project();
        let id = tree.add("Task");
        tree.complete(id).unwrap();

        let goal = tree.find_goal(id).unwrap();
        assert_eq!(goal.status, GoalStatus::Completed);
        assert_eq!(goal.progress, 100);
    }

    #[test]
    fn test_add_blocker() {
        let (_dir, mut tree) = temp_project();
        let id = tree.add("Blocked task");
        tree.add_blocker(id, "Waiting on API docs").unwrap();

        let goal = tree.find_goal(id).unwrap();
        assert_eq!(goal.status, GoalStatus::Blocked);
        assert_eq!(goal.blockers, vec!["Waiting on API docs"]);
    }

    #[test]
    fn test_checkpoint() {
        let (_dir, mut tree) = temp_project();
        tree.add("Goal A");
        tree.checkpoint("Before refactor");

        assert_eq!(tree.checkpoints().len(), 1);
        assert_eq!(tree.checkpoints()[0].label, "Before refactor");
        assert_eq!(tree.checkpoints()[0].goals.len(), 1);
    }

    #[test]
    fn test_save_and_reload() {
        let dir = tempfile::tempdir().unwrap();

        {
            let mut tree = GoalTree::load(dir.path()).unwrap();
            tree.add("Persistent goal");
            tree.save().unwrap();
        }

        {
            let tree = GoalTree::load(dir.path()).unwrap();
            assert_eq!(tree.goals().len(), 1);
            assert_eq!(tree.goals()[0].title, "Persistent goal");
        }
    }

    #[test]
    fn test_format_tree() {
        let (_dir, mut tree) = temp_project();
        let root = tree.add("Root goal");
        tree.add_child(root, "Sub-goal A").unwrap();
        tree.add_child(root, "Sub-goal B").unwrap();

        let output = tree.format_tree();
        assert!(output.contains("Root goal"));
        assert!(output.contains("Sub-goal A"));
        assert!(output.contains("Sub-goal B"));
    }

    #[test]
    fn test_all_goals_flattened() {
        let (_dir, mut tree) = temp_project();
        let root = tree.add("Root");
        tree.add_child(root, "Child").unwrap();
        tree.add("Another root");

        let all = tree.all_goals();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_not_found() {
        let (_dir, mut tree) = temp_project();
        let result = tree.complete(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_recording() {
        let (_dir, mut tree) = temp_project();
        let id = tree.add("Done");
        tree.complete(id).unwrap();
        tree.record_session("2025-01-01T00:00:00Z", "Completed goal", vec![id], vec![]);

        assert_eq!(tree.sessions().len(), 1);
        assert_eq!(tree.sessions()[0].goals_completed, vec![id]);
    }
}
