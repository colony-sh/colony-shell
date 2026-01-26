//! Protocol bridge between TreeState and colony-protocol messages
//!
//! This module converts between the internal TreeState/FileNode types
//! and the protocol TreeNode/TreeSnapshot types for terminal communication.

use crate::git::GitFileStatus;
use crate::node::{FileNode, FileType, NodeId};
use crate::state::{TreeEvent, TreeState};
use colony_protocol::tree::{
    GitStatus as ProtoGitStatus, TreeAction, TreeActionResult, TreeNode as ProtoTreeNode,
    TreeNodeType, TreeSnapshot, TreeUpdate,
};
use colony_protocol::{emit_tree_action_result, emit_tree_snapshot, emit_tree_update};
use std::path::Path;

/// Bridge for converting and emitting tree protocol messages
pub struct TreeProtocol {
    tree: TreeState,
}

impl TreeProtocol {
    /// Create a new protocol bridge for a tree state
    pub fn new(tree: TreeState) -> Self {
        Self { tree }
    }

    /// Get a reference to the underlying tree state
    pub fn tree(&self) -> &TreeState {
        &self.tree
    }

    /// Get a mutable reference to the underlying tree state
    pub fn tree_mut(&mut self) -> &mut TreeState {
        &mut self.tree
    }

    /// Emit a full tree snapshot to the terminal
    pub fn emit_snapshot(&self) {
        let snapshot = self.build_snapshot();
        emit_tree_snapshot(&snapshot);
    }

    /// Build a snapshot without emitting
    pub fn build_snapshot(&self) -> TreeSnapshot {
        let visible = self.tree.visible_nodes();
        let root_path = self.tree.root_path().to_string_lossy().to_string();

        let nodes: Vec<ProtoTreeNode> =
            visible.iter().map(|node| self.convert_node(node)).collect();

        // Get git info from the tree's git status cache
        let (git_branch, git_dirty, git_ahead, git_behind) = self.get_git_info();

        TreeSnapshot {
            root: root_path,
            nodes,
            git_branch,
            git_dirty,
            git_ahead,
            git_behind,
        }
    }

    /// Emit a tree update based on a TreeEvent
    pub fn emit_update(&self, event: &TreeEvent) {
        if let Some(update) = self.build_update(event) {
            emit_tree_update(&update);
        }
    }

    /// Build an update without emitting
    pub fn build_update(&self, event: &TreeEvent) -> Option<TreeUpdate> {
        match event {
            TreeEvent::NodeAdded(id) => {
                let node = self.tree.get_node(*id)?;
                let proto_node = self.convert_node(&node);
                let parent_id = node.parent;
                // Find index within parent's children
                let index = if let Some(pid) = parent_id {
                    self.tree
                        .get_node(pid)
                        .map(|p| p.children.iter().position(|c| c == id).unwrap_or(0))
                        .unwrap_or(0)
                } else {
                    0
                };
                Some(TreeUpdate::NodeAdded {
                    node: proto_node,
                    parent_id,
                    index,
                })
            }
            TreeEvent::NodeRemoved(id) => Some(TreeUpdate::NodeRemoved { id: *id }),
            TreeEvent::NodeModified(id) => {
                let node = self.tree.get_node(*id)?;
                let git_status = self.tree.git_status(&node.path).map(convert_git_status);
                Some(TreeUpdate::NodeModified {
                    id: *id,
                    size: node.size,
                    git_status,
                })
            }
            TreeEvent::NodeToggled(id) => {
                let node = self.tree.get_node(*id)?;
                let children = if node.expanded {
                    self.tree
                        .children(*id)
                        .iter()
                        .map(|n| self.convert_node(n))
                        .collect()
                } else {
                    Vec::new()
                };
                Some(TreeUpdate::NodeToggled {
                    id: *id,
                    expanded: node.expanded,
                    children,
                })
            }
            TreeEvent::GitStatusUpdated => {
                // Collect all git status updates
                let updates: Vec<(NodeId, Option<ProtoGitStatus>)> = self
                    .tree
                    .visible_nodes()
                    .iter()
                    .filter_map(|node| {
                        let status = self.tree.git_status(&node.path).map(convert_git_status);
                        if status.is_some() {
                            Some((node.id, status))
                        } else {
                            None
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    None
                } else {
                    Some(TreeUpdate::GitStatusUpdate { updates })
                }
            }
            TreeEvent::TreeReloaded => Some(TreeUpdate::Reload),
        }
    }

    /// Handle a tree action from the terminal
    pub fn handle_action(&self, action: &TreeAction) -> TreeActionResult {
        match action {
            TreeAction::Toggle { id } => {
                if self.tree.toggle_expanded(*id) {
                    TreeActionResult::success()
                } else {
                    TreeActionResult::error("Node not found or not a directory")
                }
            }
            TreeAction::Expand { id } => {
                if let Some(node) = self.tree.get_node(*id) {
                    if node.is_dir() && !node.expanded {
                        self.tree.toggle_expanded(*id);
                    }
                    TreeActionResult::success()
                } else {
                    TreeActionResult::error("Node not found")
                }
            }
            TreeAction::Collapse { id } => {
                if let Some(node) = self.tree.get_node(*id) {
                    if node.is_dir() && node.expanded {
                        self.tree.toggle_expanded(*id);
                    }
                    TreeActionResult::success()
                } else {
                    TreeActionResult::error("Node not found")
                }
            }
            TreeAction::Select { id } => {
                // Selection is primarily a terminal-side state, just acknowledge
                if self.tree.get_node(*id).is_some() {
                    TreeActionResult::success()
                } else {
                    TreeActionResult::error("Node not found")
                }
            }
            TreeAction::Open { path } => {
                // Opening files is handled by the terminal/shell
                // We just verify the path exists in our tree
                let path = Path::new(path);
                let id = FileNode::hash_path(path);
                if self.tree.get_node(id).is_some() {
                    TreeActionResult::success_with_data(serde_json::json!({
                        "path": path.display().to_string()
                    }))
                } else {
                    TreeActionResult::error("File not in tree")
                }
            }
            TreeAction::Reveal { path } => {
                let path = Path::new(path);
                if self.tree.expand_to(path) {
                    let id = FileNode::hash_path(path);
                    TreeActionResult::success_with_data(serde_json::json!({
                        "node_id": id
                    }))
                } else {
                    TreeActionResult::error("Path not found in tree")
                }
            }
            TreeAction::Refresh => match self.tree.load() {
                Ok(()) => TreeActionResult::success(),
                Err(e) => TreeActionResult::error(format!("Failed to refresh: {}", e)),
            },
            TreeAction::RequestSnapshot => {
                // Emit snapshot and return success
                self.emit_snapshot();
                TreeActionResult::success()
            }
            // File operations - these need to be implemented with actual fs operations
            TreeAction::CreateFile { parent_id, name } => {
                if let Some(parent) = self.tree.get_node(*parent_id) {
                    let path = parent.path.join(name);
                    match std::fs::write(&path, "") {
                        Ok(()) => {
                            // Add to tree
                            match self.tree.add_node(&path) {
                                Ok(id) => TreeActionResult::success_with_data(serde_json::json!({
                                    "id": id,
                                    "path": path.display().to_string()
                                })),
                                Err(e) => {
                                    TreeActionResult::error(format!("Failed to add to tree: {}", e))
                                }
                            }
                        }
                        Err(e) => TreeActionResult::error(format!("Failed to create file: {}", e)),
                    }
                } else {
                    TreeActionResult::error("Parent directory not found")
                }
            }
            TreeAction::CreateDirectory { parent_id, name } => {
                if let Some(parent) = self.tree.get_node(*parent_id) {
                    let path = parent.path.join(name);
                    match std::fs::create_dir(&path) {
                        Ok(()) => match self.tree.add_node(&path) {
                            Ok(id) => TreeActionResult::success_with_data(serde_json::json!({
                                "id": id,
                                "path": path.display().to_string()
                            })),
                            Err(e) => {
                                TreeActionResult::error(format!("Failed to add to tree: {}", e))
                            }
                        },
                        Err(e) => {
                            TreeActionResult::error(format!("Failed to create directory: {}", e))
                        }
                    }
                } else {
                    TreeActionResult::error("Parent directory not found")
                }
            }
            TreeAction::Rename { id, new_name } => {
                if let Some(node) = self.tree.get_node(*id) {
                    let new_path = node
                        .path
                        .parent()
                        .map(|p| p.join(new_name))
                        .unwrap_or_else(|| Path::new(new_name).to_path_buf());

                    match std::fs::rename(&node.path, &new_path) {
                        Ok(()) => {
                            // Remove old, add new
                            self.tree.remove_node(*id);
                            match self.tree.add_node(&new_path) {
                                Ok(new_id) => {
                                    TreeActionResult::success_with_data(serde_json::json!({
                                        "id": new_id,
                                        "path": new_path.display().to_string()
                                    }))
                                }
                                Err(e) => TreeActionResult::error(format!(
                                    "Renamed but failed to update tree: {}",
                                    e
                                )),
                            }
                        }
                        Err(e) => TreeActionResult::error(format!("Failed to rename: {}", e)),
                    }
                } else {
                    TreeActionResult::error("Node not found")
                }
            }
            TreeAction::Delete { id, force } => {
                if let Some(node) = self.tree.get_node(*id) {
                    let result = if node.is_dir() {
                        if *force {
                            std::fs::remove_dir_all(&node.path)
                        } else {
                            std::fs::remove_dir(&node.path)
                        }
                    } else {
                        std::fs::remove_file(&node.path)
                    };

                    match result {
                        Ok(()) => {
                            self.tree.remove_node(*id);
                            TreeActionResult::success()
                        }
                        Err(e) => TreeActionResult::error(format!("Failed to delete: {}", e)),
                    }
                } else {
                    TreeActionResult::error("Node not found")
                }
            }
            TreeAction::Copy { id, dest_parent_id } => {
                if let (Some(node), Some(dest)) =
                    (self.tree.get_node(*id), self.tree.get_node(*dest_parent_id))
                {
                    let dest_path = dest.path.join(&node.name);

                    let result = if node.is_dir() {
                        copy_dir_recursive(&node.path, &dest_path)
                    } else {
                        std::fs::copy(&node.path, &dest_path).map(|_| ())
                    };

                    match result {
                        Ok(()) => match self.tree.add_node(&dest_path) {
                            Ok(new_id) => TreeActionResult::success_with_data(serde_json::json!({
                                "id": new_id,
                                "path": dest_path.display().to_string()
                            })),
                            Err(e) => TreeActionResult::error(format!(
                                "Copied but failed to update tree: {}",
                                e
                            )),
                        },
                        Err(e) => TreeActionResult::error(format!("Failed to copy: {}", e)),
                    }
                } else {
                    TreeActionResult::error("Source or destination not found")
                }
            }
            TreeAction::Move { id, dest_parent_id } => {
                if let (Some(node), Some(dest)) =
                    (self.tree.get_node(*id), self.tree.get_node(*dest_parent_id))
                {
                    let dest_path = dest.path.join(&node.name);

                    match std::fs::rename(&node.path, &dest_path) {
                        Ok(()) => {
                            self.tree.remove_node(*id);
                            match self.tree.add_node(&dest_path) {
                                Ok(new_id) => {
                                    TreeActionResult::success_with_data(serde_json::json!({
                                        "id": new_id,
                                        "path": dest_path.display().to_string()
                                    }))
                                }
                                Err(e) => TreeActionResult::error(format!(
                                    "Moved but failed to update tree: {}",
                                    e
                                )),
                            }
                        }
                        Err(e) => TreeActionResult::error(format!("Failed to move: {}", e)),
                    }
                } else {
                    TreeActionResult::error("Source or destination not found")
                }
            }
        }
    }

    /// Handle action and emit result
    pub fn handle_and_emit(&self, action: &TreeAction) {
        let result = self.handle_action(action);
        emit_tree_action_result(&result);
    }

    /// Convert internal FileNode to protocol TreeNode
    fn convert_node(&self, node: &FileNode) -> ProtoTreeNode {
        let relative_path = pathdiff::diff_paths(&node.path, self.tree.root_path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| node.path.to_string_lossy().to_string());

        let git_status = self.tree.git_status(&node.path).map(convert_git_status);

        ProtoTreeNode {
            id: node.id,
            name: node.name.clone(),
            path: relative_path,
            node_type: convert_file_type(node.file_type),
            expanded: node.expanded,
            children: node.children.clone(),
            depth: node.depth as u16,
            size: node.size,
            git_status,
        }
    }

    /// Get git info from tree state
    fn get_git_info(&self) -> (Option<String>, bool, u32, u32) {
        // This would ideally come from GitStatusCache, but we need to expose it
        // For now, return defaults - the tree.git_status reads from the cache
        (None, false, 0, 0)
    }
}

/// Convert internal FileType to protocol TreeNodeType
fn convert_file_type(ft: FileType) -> TreeNodeType {
    match ft {
        FileType::File => TreeNodeType::File,
        FileType::Directory => TreeNodeType::Directory,
        FileType::Symlink => TreeNodeType::Symlink,
        FileType::Other => TreeNodeType::Other,
    }
}

/// Convert internal GitFileStatus to protocol GitStatus
fn convert_git_status(status: GitFileStatus) -> ProtoGitStatus {
    match status {
        GitFileStatus::Untracked => ProtoGitStatus::Untracked,
        GitFileStatus::New => ProtoGitStatus::New,
        GitFileStatus::Modified => ProtoGitStatus::Modified,
        GitFileStatus::Staged => ProtoGitStatus::Staged,
        GitFileStatus::Deleted => ProtoGitStatus::Deleted,
        GitFileStatus::Renamed => ProtoGitStatus::Renamed,
        GitFileStatus::Conflicted => ProtoGitStatus::Conflicted,
        GitFileStatus::Ignored => ProtoGitStatus::Ignored,
        GitFileStatus::Clean => ProtoGitStatus::Staged, // Clean files don't need status
    }
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_tree() -> (TempDir, TreeProtocol) {
        let temp = TempDir::new().unwrap();

        // Create structure:
        // temp/
        //   src/
        //     main.rs
        //   Cargo.toml

        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();

        let tree = TreeState::new(temp.path());
        tree.load().unwrap();

        (temp, TreeProtocol::new(tree))
    }

    #[test]
    fn test_build_snapshot() {
        let (_temp, protocol) = setup_test_tree();
        let snapshot = protocol.build_snapshot();

        assert!(!snapshot.root.is_empty());
        assert!(!snapshot.nodes.is_empty());

        // Root should be first
        assert_eq!(snapshot.nodes[0].depth, 0);
    }

    #[test]
    fn test_convert_node() {
        let (_temp, protocol) = setup_test_tree();
        let root = protocol.tree.root_node().unwrap();
        let proto = protocol.convert_node(&root);

        assert_eq!(proto.id, root.id);
        assert_eq!(proto.node_type, TreeNodeType::Directory);
        assert!(proto.expanded); // Root is always expanded
    }

    #[test]
    fn test_handle_toggle() {
        let (_temp, protocol) = setup_test_tree();

        // Find src directory
        let visible = protocol.tree.visible_nodes();
        let src = visible.iter().find(|n| n.name == "src").unwrap();

        // Toggle (should collapse since directories start collapsed after load)
        let result = protocol.handle_action(&TreeAction::Toggle { id: src.id });
        assert!(result.success);
    }

    #[test]
    fn test_handle_refresh() {
        let (_temp, protocol) = setup_test_tree();
        let result = protocol.handle_action(&TreeAction::Refresh);
        assert!(result.success);
    }

    #[test]
    fn test_handle_create_file() {
        let (temp, protocol) = setup_test_tree();

        // Get src directory ID
        let visible = protocol.tree.visible_nodes();
        let src = visible.iter().find(|n| n.name == "src").unwrap();

        let result = protocol.handle_action(&TreeAction::CreateFile {
            parent_id: src.id,
            name: "new_file.rs".to_string(),
        });

        assert!(result.success);
        assert!(temp.path().join("src/new_file.rs").exists());
    }

    #[test]
    fn test_handle_delete() {
        let (temp, protocol) = setup_test_tree();

        // Find Cargo.toml
        let visible = protocol.tree.visible_nodes();
        let cargo = visible.iter().find(|n| n.name == "Cargo.toml").unwrap();

        let result = protocol.handle_action(&TreeAction::Delete {
            id: cargo.id,
            force: false,
        });

        assert!(result.success);
        assert!(!temp.path().join("Cargo.toml").exists());
    }

    #[test]
    fn test_build_update_node_added() {
        let (_temp, protocol) = setup_test_tree();

        // Find src directory
        let visible = protocol.tree.visible_nodes();
        let src = visible.iter().find(|n| n.name == "src").unwrap();

        let event = TreeEvent::NodeAdded(src.id);
        let update = protocol.build_update(&event);

        assert!(update.is_some());
        match update.unwrap() {
            TreeUpdate::NodeAdded { node, .. } => {
                assert_eq!(node.name, "src");
            }
            _ => panic!("Expected NodeAdded"),
        }
    }

    #[test]
    fn test_build_update_node_toggled() {
        let (_temp, protocol) = setup_test_tree();

        // Find src directory
        let visible = protocol.tree.visible_nodes();
        let src = visible.iter().find(|n| n.name == "src").unwrap();

        let event = TreeEvent::NodeToggled(src.id);
        let update = protocol.build_update(&event);

        assert!(update.is_some());
        match update.unwrap() {
            TreeUpdate::NodeToggled { id, expanded, .. } => {
                assert_eq!(id, src.id);
                // Default is not expanded
                assert!(!expanded);
            }
            _ => panic!("Expected NodeToggled"),
        }
    }
}
