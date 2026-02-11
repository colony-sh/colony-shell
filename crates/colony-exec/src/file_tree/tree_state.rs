//! File tree state representation
//!
//! Holds an in-memory tree of FileNode entries that mirrors the project
//! directory. Each node carries name, path, type, and optional git status.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Git status for a single file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitFileStatus {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Ignored,
    Conflicted,
}

impl GitFileStatus {
    /// Parse from single-char git status indicator
    pub fn from_porcelain_char(c: char) -> Self {
        match c {
            ' ' => Self::Unmodified,
            'M' => Self::Modified,
            'A' => Self::Added,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            '?' => Self::Untracked,
            '!' => Self::Ignored,
            'U' => Self::Conflicted,
            _ => Self::Unmodified,
        }
    }
}

/// A node in the file tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub git_status: Option<GitFileStatus>,
    pub children: Vec<FileNode>,
}

impl FileNode {
    /// Create a new file node
    pub fn new_file(name: String, path: PathBuf) -> Self {
        Self {
            name,
            path,
            is_dir: false,
            git_status: None,
            children: Vec::new(),
        }
    }

    /// Create a new directory node
    pub fn new_dir(name: String, path: PathBuf) -> Self {
        Self {
            name,
            path,
            is_dir: true,
            git_status: None,
            children: Vec::new(),
        }
    }

    /// Count total nodes in this subtree (including self)
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(FileNode::node_count).sum::<usize>()
    }
}

/// In-memory file tree state for a directory
pub struct TreeState {
    root: PathBuf,
    nodes: HashMap<PathBuf, FileNode>,
    root_node: Option<FileNode>,
}

impl TreeState {
    /// Create empty tree state for a root directory
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            nodes: HashMap::new(),
            root_node: None,
        }
    }

    /// Get the root directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the root node of the tree
    pub fn root_node(&self) -> Option<&FileNode> {
        self.root_node.as_ref()
    }

    /// Build tree state by scanning the directory (up to max_depth)
    pub fn scan(&mut self, max_depth: usize) -> std::io::Result<()> {
        self.nodes.clear();
        let root_name = self
            .root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.root.display().to_string());

        let root_node = self.scan_dir(&self.root.clone(), &root_name, 0, max_depth)?;
        self.root_node = Some(root_node);
        Ok(())
    }

    fn scan_dir(
        &mut self,
        dir: &Path,
        name: &str,
        depth: usize,
        max_depth: usize,
    ) -> std::io::Result<FileNode> {
        let mut node = FileNode::new_dir(name.to_string(), dir.to_path_buf());

        if depth >= max_depth {
            return Ok(node);
        }

        let mut entries: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let entry_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files and common ignored directories
            if entry_name.starts_with('.') || Self::is_ignored(&entry_name) {
                continue;
            }

            let entry_path = entry.path();
            let ft = entry.file_type()?;

            let child = if ft.is_dir() {
                self.scan_dir(&entry_path, &entry_name, depth + 1, max_depth)?
            } else {
                let child_node = FileNode::new_file(entry_name.clone(), entry_path.clone());
                self.nodes.insert(entry_path, child_node.clone());
                child_node
            };

            node.children.push(child);
        }

        self.nodes.insert(dir.to_path_buf(), node.clone());
        Ok(node)
    }

    fn is_ignored(name: &str) -> bool {
        matches!(
            name,
            "node_modules" | "target" | "__pycache__" | ".git" | "dist" | "build" | "vendor"
        )
    }

    /// Apply git status annotations from a status cache
    pub fn apply_git_status(&mut self, statuses: &HashMap<PathBuf, GitFileStatus>) {
        if let Some(ref mut root) = self.root_node {
            Self::apply_status_recursive(root, statuses);
        }

        for (path, status) in statuses {
            if let Some(node) = self.nodes.get_mut(path) {
                node.git_status = Some(*status);
            }
        }
    }

    fn apply_status_recursive(
        node: &mut FileNode,
        statuses: &HashMap<PathBuf, GitFileStatus>,
    ) {
        if let Some(status) = statuses.get(&node.path) {
            node.git_status = Some(*status);
        }

        for child in &mut node.children {
            Self::apply_status_recursive(child, statuses);
        }
    }

    /// Get a node by path
    pub fn get(&self, path: &Path) -> Option<&FileNode> {
        self.nodes.get(path)
    }

    /// Notify that a file was added
    pub fn notify_added(&mut self, path: &Path) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let is_dir = path.is_dir();
        let node = if is_dir {
            FileNode::new_dir(name, path.to_path_buf())
        } else {
            FileNode::new_file(name, path.to_path_buf())
        };

        self.nodes.insert(path.to_path_buf(), node);
    }

    /// Notify that a file was removed
    pub fn notify_removed(&mut self, path: &Path) {
        self.nodes.remove(path);
    }

    /// Notify that a file was modified
    pub fn notify_modified(&mut self, path: &Path) {
        // Node stays in the tree, just invalidate git status
        if let Some(node) = self.nodes.get_mut(path) {
            node.git_status = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_node_creation() {
        let file = FileNode::new_file("test.rs".to_string(), PathBuf::from("/src/test.rs"));
        assert!(!file.is_dir);
        assert_eq!(file.name, "test.rs");
        assert!(file.children.is_empty());
        assert!(file.git_status.is_none());
    }

    #[test]
    fn test_dir_node_creation() {
        let dir = FileNode::new_dir("src".to_string(), PathBuf::from("/src"));
        assert!(dir.is_dir);
        assert_eq!(dir.name, "src");
    }

    #[test]
    fn test_node_count() {
        let mut dir = FileNode::new_dir("src".to_string(), PathBuf::from("/src"));
        dir.children
            .push(FileNode::new_file("a.rs".to_string(), PathBuf::from("/src/a.rs")));
        dir.children
            .push(FileNode::new_file("b.rs".to_string(), PathBuf::from("/src/b.rs")));
        assert_eq!(dir.node_count(), 3);
    }

    #[test]
    fn test_git_file_status_parsing() {
        assert_eq!(GitFileStatus::from_porcelain_char('M'), GitFileStatus::Modified);
        assert_eq!(GitFileStatus::from_porcelain_char('A'), GitFileStatus::Added);
        assert_eq!(GitFileStatus::from_porcelain_char('D'), GitFileStatus::Deleted);
        assert_eq!(GitFileStatus::from_porcelain_char('?'), GitFileStatus::Untracked);
        assert_eq!(GitFileStatus::from_porcelain_char(' '), GitFileStatus::Unmodified);
    }

    #[test]
    fn test_scan_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file.txt"), "content").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir/nested.txt"), "nested").unwrap();

        let mut tree = TreeState::new(dir.path().to_path_buf());
        tree.scan(3).unwrap();

        let root = tree.root_node().unwrap();
        assert!(root.is_dir);
        assert_eq!(root.children.len(), 2); // file.txt + subdir
    }

    #[test]
    fn test_apply_git_status() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("modified.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let mut tree = TreeState::new(dir.path().to_path_buf());
        tree.scan(3).unwrap();

        let mut statuses = HashMap::new();
        statuses.insert(file_path.clone(), GitFileStatus::Modified);
        tree.apply_git_status(&statuses);

        let node = tree.get(&file_path).unwrap();
        assert_eq!(node.git_status, Some(GitFileStatus::Modified));
    }
}
