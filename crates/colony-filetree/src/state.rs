//! Tree state management

use crate::git::GitStatusCache;
use crate::node::{FileNode, NodeId};
use parking_lot::RwLock;
use scc::HashMap as SccHashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Events emitted by tree state changes
#[derive(Debug, Clone)]
pub enum TreeEvent {
    /// A node was added
    NodeAdded(NodeId),
    /// A node was removed
    NodeRemoved(NodeId),
    /// A node was modified
    NodeModified(NodeId),
    /// A node was expanded/collapsed
    NodeToggled(NodeId),
    /// Tree was fully reloaded
    TreeReloaded,
    /// Git status was updated
    GitStatusUpdated,
}

/// Configuration for tree loading
#[derive(Debug, Clone)]
pub struct TreeConfig {
    /// Maximum depth to load initially
    pub max_depth: usize,
    /// Show hidden files
    pub show_hidden: bool,
    /// Patterns to ignore (gitignore-style)
    pub ignore_patterns: Vec<String>,
    /// Whether to watch for changes
    pub watch: bool,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            max_depth: 3,
            show_hidden: false,
            ignore_patterns: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                "__pycache__".to_string(),
                ".DS_Store".to_string(),
            ],
            watch: true,
        }
    }
}

/// Complete tree state
#[derive(Clone)]
pub struct TreeState {
    /// Root path of the tree
    root_path: PathBuf,
    /// All nodes indexed by ID
    nodes: Arc<SccHashMap<NodeId, FileNode>>,
    /// Root node ID
    root_id: Arc<RwLock<Option<NodeId>>>,
    /// Git status cache
    git_status: Arc<RwLock<Option<GitStatusCache>>>,
    /// Configuration
    config: Arc<RwLock<TreeConfig>>,
    /// Event subscribers (tuple space integration)
    event_tx: Arc<RwLock<Option<tokio::sync::mpsc::Sender<TreeEvent>>>>,
}

impl TreeState {
    /// Create a new tree state for a root path
    pub fn new(root_path: &Path) -> Self {
        Self {
            root_path: root_path.to_path_buf(),
            nodes: Arc::new(SccHashMap::new()),
            root_id: Arc::new(RwLock::new(None)),
            git_status: Arc::new(RwLock::new(None)),
            config: Arc::new(RwLock::new(TreeConfig::default())),
            event_tx: Arc::new(RwLock::new(None)),
        }
    }

    /// Create with custom config
    pub fn with_config(root_path: &Path, config: TreeConfig) -> Self {
        let state = Self::new(root_path);
        *state.config.write() = config;
        state
    }

    /// Get the root path
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Set event channel for notifications
    pub fn set_event_channel(&self, tx: tokio::sync::mpsc::Sender<TreeEvent>) {
        *self.event_tx.write() = Some(tx);
    }

    /// Load the tree from disk
    pub fn load(&self) -> miette::Result<()> {
        let config = self.config.read().clone();

        // Clear existing nodes
        self.nodes.clear();

        // Create root node
        let root = FileNode::root(self.root_path.clone())
            .map_err(|e| miette::miette!("Failed to create root node: {}", e))?;

        let root_id = root.id;
        let _ = self.nodes.insert(root_id, root);
        *self.root_id.write() = Some(root_id);

        // Load children recursively
        self.load_children(root_id, 0, &config)?;

        // Initialize git status
        if let Ok(cache) = GitStatusCache::new(&self.root_path) {
            *self.git_status.write() = Some(cache);
        }

        self.emit_event(TreeEvent::TreeReloaded);
        Ok(())
    }

    /// Load children of a node
    fn load_children(
        &self,
        parent_id: NodeId,
        depth: usize,
        config: &TreeConfig,
    ) -> miette::Result<()> {
        if depth >= config.max_depth {
            return Ok(());
        }

        let parent_path = {
            let entry = self.nodes.get(&parent_id);
            match entry {
                Some(e) => e.get().path.clone(),
                None => return Ok(()),
            }
        };

        // Read directory contents
        let entries = std::fs::read_dir(&parent_path)
            .map_err(|e| miette::miette!("Failed to read directory {:?}: {}", parent_path, e))?;

        let mut child_ids = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            // Skip hidden files if configured
            if !config.show_hidden && name.starts_with('.') {
                continue;
            }

            // Skip ignored patterns
            if config.ignore_patterns.iter().any(|p| name == *p) {
                continue;
            }

            // Create node
            if let Ok(node) = FileNode::new(path, Some(parent_id), depth + 1) {
                let node_id = node.id;
                let is_dir = node.is_dir();

                let _ = self.nodes.insert(node_id, node);
                child_ids.push(node_id);

                // Recursively load directory children
                if is_dir {
                    let _ = self.load_children(node_id, depth + 1, config);
                }
            }
        }

        // Update parent with children
        if let Some(mut entry) = self.nodes.get(&parent_id) {
            entry.get_mut().children = child_ids;
        }

        Ok(())
    }

    /// Get a node by ID
    pub fn get_node(&self, id: NodeId) -> Option<FileNode> {
        self.nodes.get(&id).map(|e| e.get().clone())
    }

    /// Get the root node
    pub fn root_node(&self) -> Option<FileNode> {
        self.root_id.read().and_then(|id| self.get_node(id))
    }

    /// Get children of a node
    pub fn children(&self, id: NodeId) -> Vec<FileNode> {
        self.get_node(id)
            .map(|node| {
                node.children
                    .iter()
                    .filter_map(|child_id| self.get_node(*child_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Toggle expanded state of a directory
    pub fn toggle_expanded(&self, id: NodeId) -> bool {
        if let Some(mut entry) = self.nodes.get(&id) {
            let node = entry.get_mut();
            if node.is_dir() {
                node.toggle_expanded();

                // If expanding and no children loaded, load them
                if node.expanded && node.children.is_empty() {
                    let config = self.config.read().clone();
                    let _ = self.load_children(id, node.depth, &config);
                }

                self.emit_event(TreeEvent::NodeToggled(id));
                return true;
            }
        }
        false
    }

    /// Expand a node and all its ancestors (for navigation)
    pub fn expand_to(&self, path: &Path) -> bool {
        // Find node for path
        let target_id = FileNode::hash_path(path);

        // Build path from root to target
        let mut current = self.root_id.read().clone();
        let mut path_parts: Vec<_> = path
            .strip_prefix(&self.root_path)
            .unwrap_or(path)
            .components()
            .collect();

        while let Some(id) = current {
            if let Some(mut entry) = self.nodes.get(&id) {
                let node = entry.get_mut();
                if node.is_dir() && !node.expanded {
                    node.expanded = true;
                    if node.children.is_empty() {
                        let config = self.config.read().clone();
                        drop(entry); // Release lock before recursive call
                        let _ = self.load_children(id, 0, &config);
                    }
                }
            }

            if path_parts.is_empty() || id == target_id {
                break;
            }

            // Find next child matching path component
            let next_name = path_parts.remove(0);
            let next_name_str = next_name.as_os_str().to_string_lossy();

            current = self
                .children(id)
                .into_iter()
                .find(|n| n.name == next_name_str)
                .map(|n| n.id);
        }

        self.nodes.contains(&target_id)
    }

    /// Refresh a specific node (re-read from disk)
    pub fn refresh_node(&self, id: NodeId) -> miette::Result<()> {
        let path = self
            .get_node(id)
            .map(|n| n.path.clone())
            .ok_or_else(|| miette::miette!("Node not found"))?;

        if let Ok(new_node) = FileNode::new(path, None, 0) {
            if let Some(mut entry) = self.nodes.get(&id) {
                let old = entry.get_mut();
                old.size = new_node.size;
                old.modified = new_node.modified;
                self.emit_event(TreeEvent::NodeModified(id));
            }
        }
        Ok(())
    }

    /// Add a new node (called when file is created)
    pub fn add_node(&self, path: &Path) -> miette::Result<NodeId> {
        let parent_path = path
            .parent()
            .ok_or_else(|| miette::miette!("Path has no parent"))?;
        let parent_id = FileNode::hash_path(parent_path);

        // Get parent depth
        let depth = self.get_node(parent_id).map(|n| n.depth + 1).unwrap_or(0);

        let node = FileNode::new(path.to_path_buf(), Some(parent_id), depth)
            .map_err(|e| miette::miette!("Failed to create node: {}", e))?;

        let node_id = node.id;
        let _ = self.nodes.insert(node_id, node);

        // Add to parent's children
        if let Some(mut entry) = self.nodes.get(&parent_id) {
            entry.get_mut().children.push(node_id);
        }

        self.emit_event(TreeEvent::NodeAdded(node_id));
        Ok(node_id)
    }

    /// Remove a node (called when file is deleted)
    pub fn remove_node(&self, id: NodeId) -> bool {
        if let Some((_, node)) = self.nodes.remove(&id) {
            // Remove from parent's children
            if let Some(parent_id) = node.parent {
                if let Some(mut entry) = self.nodes.get(&parent_id) {
                    entry.get_mut().children.retain(|&child_id| child_id != id);
                }
            }

            // Remove all descendants
            for child_id in node.children {
                self.remove_node(child_id);
            }

            self.emit_event(TreeEvent::NodeRemoved(id));
            return true;
        }
        false
    }

    /// Get visible nodes for rendering (respects expanded state)
    pub fn visible_nodes(&self) -> Vec<FileNode> {
        let mut result = Vec::new();

        if let Some(root_id) = *self.root_id.read() {
            self.collect_visible(root_id, &mut result);
        }

        result
    }

    /// Collect visible nodes recursively
    fn collect_visible(&self, id: NodeId, result: &mut Vec<FileNode>) {
        if let Some(node) = self.get_node(id) {
            result.push(node.clone());

            if node.expanded {
                // Sort children: directories first, then alphabetically
                let mut children = self.children(id);
                children.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                });

                for child in children {
                    self.collect_visible(child.id, result);
                }
            }
        }
    }

    /// Refresh git status
    pub fn refresh_git_status(&self) {
        if let Ok(cache) = GitStatusCache::new(&self.root_path) {
            *self.git_status.write() = Some(cache);
            self.emit_event(TreeEvent::GitStatusUpdated);
        }
    }

    /// Get git status for a path
    pub fn git_status(&self, path: &Path) -> Option<crate::git::GitFileStatus> {
        self.git_status
            .read()
            .as_ref()
            .and_then(|cache| cache.status(path))
    }

    /// Get node count
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Emit an event
    fn emit_event(&self, event: TreeEvent) {
        if let Some(tx) = self.event_tx.read().as_ref() {
            let _ = tx.try_send(event);
        }
    }
}

impl std::fmt::Debug for TreeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeState")
            .field("root_path", &self.root_path)
            .field("node_count", &self.nodes.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_tree() -> (TempDir, TreeState) {
        let temp = TempDir::new().unwrap();

        // Create structure:
        // temp/
        //   src/
        //     main.rs
        //     lib.rs
        //   Cargo.toml
        //   .gitignore (hidden)

        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(temp.path().join("src/lib.rs"), "pub fn lib() {}").unwrap();
        fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(temp.path().join(".gitignore"), "target/").unwrap();

        let state = TreeState::new(temp.path());
        (temp, state)
    }

    #[test]
    fn test_load_tree() {
        let (_temp, state) = setup_test_tree();
        state.load().unwrap();

        // Should have root + src + 2 rs files + Cargo.toml = 5 nodes (hidden ignored)
        assert!(state.node_count() >= 4);
    }

    #[test]
    fn test_visible_nodes() {
        let (_temp, state) = setup_test_tree();
        state.load().unwrap();

        let visible = state.visible_nodes();
        assert!(!visible.is_empty());

        // First should be root
        assert_eq!(visible[0].depth, 0);
    }

    #[test]
    fn test_toggle_expanded() {
        let (_temp, state) = setup_test_tree();
        state.load().unwrap();

        // Find src directory
        let visible = state.visible_nodes();
        let src_node = visible.iter().find(|n| n.name == "src").unwrap();

        // src should start collapsed (expanded = false)
        let initial_node = state.get_node(src_node.id).unwrap();
        assert!(!initial_node.expanded, "src should start collapsed");

        // Toggle to expand
        assert!(state.toggle_expanded(src_node.id));

        // Should be expanded now
        let node = state.get_node(src_node.id).unwrap();
        assert!(node.expanded, "src should be expanded after toggle");

        // Toggle again to collapse
        assert!(state.toggle_expanded(src_node.id));

        // Should be collapsed now
        let node = state.get_node(src_node.id).unwrap();
        assert!(
            !node.expanded,
            "src should be collapsed after second toggle"
        );
    }

    #[test]
    fn test_children() {
        let (_temp, state) = setup_test_tree();
        state.load().unwrap();

        let root = state.root_node().unwrap();
        let children = state.children(root.id);

        // Should have src dir and Cargo.toml
        assert!(children.len() >= 2);
    }

    #[test]
    fn test_config_show_hidden() {
        let (temp, _) = setup_test_tree();

        let config = TreeConfig {
            show_hidden: true,
            ..Default::default()
        };
        let state = TreeState::with_config(temp.path(), config);
        state.load().unwrap();

        let visible = state.visible_nodes();
        assert!(visible.iter().any(|n| n.name == ".gitignore"));
    }
}
