//! File tree protocol messages for shell ↔ terminal communication
//!
//! This module defines the protocol for syncing file tree state between
//! the shell (which owns the tree state) and the terminal (which renders it).
//!
//! ## Message Flow
//!
//! Shell → Terminal:
//! - `TreeSnapshot`: Full tree state (initial load or after major changes)
//! - `TreeUpdate`: Incremental updates (node added/removed/modified)
//! - `TreeGitStatus`: Git status updates for files
//!
//! Terminal → Shell:
//! - `TreeAction`: User interactions (expand, collapse, open, create, etc.)
//!
//! ## Size Considerations
//!
//! - Small updates (< 4KB): Send via OSC 2337 directly
//! - Large snapshots: Use base64 encoding or side channel (future)

use serde::{Deserialize, Serialize};

/// Unique identifier for a tree node (hash of path)
pub type NodeId = u64;

/// Compact node representation for protocol transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    /// Unique identifier
    pub id: NodeId,
    /// File/directory name
    pub name: String,
    /// Relative path from tree root
    pub path: String,
    /// Type of node
    pub node_type: TreeNodeType,
    /// Whether expanded (directories only)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub expanded: bool,
    /// Children node IDs (directories only)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
    /// Depth in tree (0 = root)
    pub depth: u16,
    /// File size in bytes (files only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Git status indicator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_status: Option<GitStatus>,
}

/// Type of tree node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeNodeType {
    File,
    Directory,
    Symlink,
    Other,
}

/// Git status for a file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitStatus {
    Untracked,
    New,
    Modified,
    Staged,
    Deleted,
    Renamed,
    Conflicted,
    Ignored,
}

/// Full tree snapshot (shell → terminal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeSnapshot {
    /// Root path of the tree
    pub root: String,
    /// All visible nodes (flattened, respecting expansion)
    pub nodes: Vec<TreeNode>,
    /// Git branch name (if in a git repo)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    /// Whether repo has uncommitted changes
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub git_dirty: bool,
    /// Commits ahead of upstream
    #[serde(default, skip_serializing_if = "is_zero")]
    pub git_ahead: u32,
    /// Commits behind upstream
    #[serde(default, skip_serializing_if = "is_zero")]
    pub git_behind: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Incremental tree update (shell → terminal)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TreeUpdate {
    /// Node was added
    NodeAdded {
        node: TreeNode,
        /// Parent node ID (None if root)
        parent_id: Option<NodeId>,
        /// Index within parent's children
        index: usize,
    },
    /// Node was removed
    NodeRemoved { id: NodeId },
    /// Node was modified (content changed)
    NodeModified {
        id: NodeId,
        /// New size
        size: Option<u64>,
        /// New git status
        git_status: Option<GitStatus>,
    },
    /// Node expansion state toggled
    NodeToggled {
        id: NodeId,
        expanded: bool,
        /// Children to show (if expanding)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        children: Vec<TreeNode>,
    },
    /// Git status batch update
    GitStatusUpdate {
        /// Map of node ID to new status
        updates: Vec<(NodeId, Option<GitStatus>)>,
    },
    /// Full tree reload needed
    Reload,
}

/// User action on the tree (terminal → shell)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TreeAction {
    /// Toggle expand/collapse of a directory
    Toggle { id: NodeId },
    /// Expand a directory (load children if needed)
    Expand { id: NodeId },
    /// Collapse a directory
    Collapse { id: NodeId },
    /// Open a file (in editor or default app)
    Open { path: String },
    /// Select a node (for context, not open)
    Select { id: NodeId },
    /// Request to create a new file
    CreateFile { parent_id: NodeId, name: String },
    /// Request to create a new directory
    CreateDirectory { parent_id: NodeId, name: String },
    /// Request to rename a node
    Rename { id: NodeId, new_name: String },
    /// Request to delete a node
    Delete {
        id: NodeId,
        /// If true, delete without confirmation
        force: bool,
    },
    /// Request to copy a node
    Copy { id: NodeId, dest_parent_id: NodeId },
    /// Request to move a node
    Move { id: NodeId, dest_parent_id: NodeId },
    /// Reveal a path in the tree (expand parents, scroll to view)
    Reveal { path: String },
    /// Refresh the tree (reload from disk)
    Refresh,
    /// Request full snapshot
    RequestSnapshot,
}

/// Result of a tree action (shell → terminal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeActionResult {
    /// Whether the action succeeded
    pub success: bool,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Associated data (action-specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl TreeNode {
    /// Create a new file node
    pub fn file(id: NodeId, name: String, path: String, depth: u16) -> Self {
        Self {
            id,
            name,
            path,
            node_type: TreeNodeType::File,
            expanded: false,
            children: Vec::new(),
            depth,
            size: None,
            git_status: None,
        }
    }

    /// Create a new directory node
    pub fn directory(id: NodeId, name: String, path: String, depth: u16) -> Self {
        Self {
            id,
            name,
            path,
            node_type: TreeNodeType::Directory,
            expanded: false,
            children: Vec::new(),
            depth,
            size: None,
            git_status: None,
        }
    }

    /// Set the size
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Set git status
    pub fn with_git_status(mut self, status: GitStatus) -> Self {
        self.git_status = Some(status);
        self
    }

    /// Set expanded state
    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Set children
    pub fn with_children(mut self, children: Vec<NodeId>) -> Self {
        self.children = children;
        self
    }
}

impl TreeSnapshot {
    /// Create an empty snapshot
    pub fn empty(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            nodes: Vec::new(),
            git_branch: None,
            git_dirty: false,
            git_ahead: 0,
            git_behind: 0,
        }
    }

    /// Estimate serialized size (for deciding OSC vs side channel)
    pub fn estimated_size(&self) -> usize {
        // Rough estimate: ~100 bytes per node + base overhead
        200 + self.nodes.len() * 100
    }

    /// Check if small enough for OSC (< 4KB recommended)
    pub fn fits_in_osc(&self) -> bool {
        self.estimated_size() < 4096
    }
}

impl TreeActionResult {
    /// Create a success result
    pub fn success() -> Self {
        Self {
            success: true,
            error: None,
            data: None,
        }
    }

    /// Create a success result with data
    pub fn success_with_data(data: serde_json::Value) -> Self {
        Self {
            success: true,
            error: None,
            data: Some(data),
        }
    }

    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(message.into()),
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_node_serialization() {
        let node = TreeNode::directory(123, "src".to_string(), "src".to_string(), 1)
            .with_expanded(true)
            .with_children(vec![456, 789]);

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"id\":123"));
        assert!(json.contains("\"name\":\"src\""));
        assert!(json.contains("\"node_type\":\"directory\""));
        assert!(json.contains("\"expanded\":true"));

        let parsed: TreeNode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, 123);
        assert_eq!(parsed.children.len(), 2);
    }

    #[test]
    fn test_tree_snapshot_serialization() {
        let snapshot = TreeSnapshot {
            root: "/project".to_string(),
            nodes: vec![
                TreeNode::directory(1, "project".to_string(), "".to_string(), 0)
                    .with_expanded(true),
                TreeNode::file(2, "main.rs".to_string(), "main.rs".to_string(), 1),
            ],
            git_branch: Some("main".to_string()),
            git_dirty: true,
            git_ahead: 2,
            git_behind: 0,
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("\"root\":\"/project\""));
        assert!(json.contains("\"git_branch\":\"main\""));
        assert!(json.contains("\"git_dirty\":true"));
        assert!(json.contains("\"git_ahead\":2"));
        // git_behind should be skipped (is_zero)
        assert!(!json.contains("git_behind"));
    }

    #[test]
    fn test_tree_update_node_added() {
        let update = TreeUpdate::NodeAdded {
            node: TreeNode::file(42, "new.rs".to_string(), "src/new.rs".to_string(), 2),
            parent_id: Some(10),
            index: 3,
        };

        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("\"kind\":\"node_added\""));
        assert!(json.contains("\"parent_id\":10"));
        assert!(json.contains("\"index\":3"));
    }

    #[test]
    fn test_tree_update_node_toggled() {
        let update = TreeUpdate::NodeToggled {
            id: 100,
            expanded: true,
            children: vec![
                TreeNode::file(101, "a.rs".to_string(), "src/a.rs".to_string(), 2),
                TreeNode::file(102, "b.rs".to_string(), "src/b.rs".to_string(), 2),
            ],
        };

        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("\"kind\":\"node_toggled\""));
        assert!(json.contains("\"expanded\":true"));
        assert!(json.contains("\"children\""));
    }

    #[test]
    fn test_tree_action_toggle() {
        let action = TreeAction::Toggle { id: 42 };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"action\":\"toggle\""));
        assert!(json.contains("\"id\":42"));
    }

    #[test]
    fn test_tree_action_create_file() {
        let action = TreeAction::CreateFile {
            parent_id: 10,
            name: "new_file.rs".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"action\":\"create_file\""));
        assert!(json.contains("\"parent_id\":10"));
        assert!(json.contains("\"name\":\"new_file.rs\""));
    }

    #[test]
    fn test_tree_action_result() {
        let success = TreeActionResult::success();
        let json = serde_json::to_string(&success).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(!json.contains("error"));

        let error = TreeActionResult::error("File not found");
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"File not found\""));
    }

    #[test]
    fn test_git_status_serialization() {
        let node = TreeNode::file(1, "test.rs".to_string(), "test.rs".to_string(), 0)
            .with_git_status(GitStatus::Modified);

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"git_status\":\"modified\""));
    }

    #[test]
    fn test_snapshot_size_estimation() {
        let mut snapshot = TreeSnapshot::empty("/test");
        assert!(snapshot.fits_in_osc());

        // Add 50 nodes - should still fit
        for i in 0..50 {
            snapshot.nodes.push(TreeNode::file(
                i,
                format!("file{}.rs", i),
                format!("file{}.rs", i),
                1,
            ));
        }
        assert!(snapshot.estimated_size() < 8000);
    }
}
