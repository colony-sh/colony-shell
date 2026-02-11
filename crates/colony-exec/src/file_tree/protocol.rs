//! File tree protocol layer (F029)
//!
//! Observes `tree_change` tuples from the tuple space and emits
//! OSC 2337 `FileTree` messages to the terminal for synchronization.
//!
//! Message types:
//! - `FileTreeSnapshot` — full tree (sent on cd / initial load)
//! - `FileTreeDelta` — incremental update (created/modified/removed)
//! - `FileTreeGit` — git status overlay

use super::{GitStatusCache, TreeState};
use colony_protocol::{
    emit_file_tree_delta, emit_file_tree_git, emit_file_tree_snapshot, FileTreeChangeKind,
};
use crate::builtins::BuiltinResult;
use crate::Result;
use colony_tuple::{Tuple, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::debug;

/// Observes tree_change tuples and emits OSC 2337 FileTree messages
pub struct TreeProtocol {
    tree_state: TreeState,
    git_cache: Arc<GitStatusCache>,
    max_depth: usize,
    shell_instance_id: String,
}

impl TreeProtocol {
    /// Create a new TreeProtocol for the given root directory
    pub fn new(root: PathBuf, git_cache: Arc<GitStatusCache>, max_depth: usize) -> Self {
        Self {
            tree_state: TreeState::new(root),
            git_cache,
            max_depth,
            shell_instance_id: std::process::id().to_string(),
        }
    }

    /// Get the root directory
    pub fn root(&self) -> &Path {
        self.tree_state.root()
    }

    /// Perform initial scan and emit full snapshot to terminal
    pub fn initial_sync(&mut self) -> std::io::Result<()> {
        self.tree_state.scan(self.max_depth)?;

        if self.git_cache.is_git_repo() {
            let statuses = self.git_cache.statuses();
            self.tree_state.apply_git_status(&statuses);
        }

        self.emit_snapshot();
        self.emit_git_status();
        Ok(())
    }

    /// Change root directory (e.g., on cd) and emit full snapshot
    pub fn change_root(&mut self, new_root: PathBuf) -> std::io::Result<()> {
        self.tree_state = TreeState::new(new_root);
        self.initial_sync()
    }

    /// Process a tree_change tuple and emit the appropriate delta
    ///
    /// Expected tuple format: ("tree_change", kind_str, path_str)
    pub fn process_tree_change(&mut self, tuple: &Tuple) {
        let values = &tuple.fields;
        if values.len() < 3 {
            return;
        }

        let kind_str = match &values[1] {
            Value::Str(s) => s.as_str(),
            _ => return,
        };

        let path_str = match &values[2] {
            Value::Str(s) => s.as_str(),
            _ => return,
        };

        let path = PathBuf::from(path_str);

        let kind = match kind_str {
            "created" => {
                self.tree_state.notify_added(&path);
                FileTreeChangeKind::Created
            }
            "modified" => {
                self.tree_state.notify_modified(&path);
                FileTreeChangeKind::Modified
            }
            "removed" => {
                self.tree_state.notify_removed(&path);
                FileTreeChangeKind::Removed
            }
            _ => return,
        };

        debug!("FileTree delta: {:?} {}", kind, path_str);
        emit_file_tree_delta(kind, path_str, Some(&self.shell_instance_id));
    }

    /// Force a full resync (scan + emit snapshot)
    pub fn force_sync(&mut self) -> std::io::Result<()> {
        self.initial_sync()
    }

    /// Emit a full tree snapshot to the terminal
    fn emit_snapshot(&self) {
        let Some(root_node) = self.tree_state.root_node() else {
            return;
        };
        let Ok(tree_json) = serde_json::to_value(root_node) else {
            return;
        };
        let cwd = self.tree_state.root().display().to_string();
        emit_file_tree_snapshot(tree_json, &cwd, Some(&self.shell_instance_id));
    }

    /// Emit git status overlay to the terminal
    fn emit_git_status(&self) {
        if !self.git_cache.is_git_repo() {
            return;
        }

        let statuses = self.git_cache.statuses();
        if statuses.is_empty() {
            return;
        }

        let root = self.tree_state.root();
        let status_map: HashMap<String, &str> = statuses
            .iter()
            .filter_map(|(path, status)| {
                let rel = path.strip_prefix(root).ok()?;
                let status_str = match status {
                    super::GitFileStatus::Modified => "modified",
                    super::GitFileStatus::Added => "added",
                    super::GitFileStatus::Deleted => "deleted",
                    super::GitFileStatus::Renamed => "renamed",
                    super::GitFileStatus::Copied => "copied",
                    super::GitFileStatus::Untracked => "untracked",
                    super::GitFileStatus::Ignored => "ignored",
                    super::GitFileStatus::Conflicted => "conflicted",
                    super::GitFileStatus::Unmodified => return None,
                };
                Some((rel.display().to_string(), status_str))
            })
            .collect();

        let Ok(json) = serde_json::to_value(&status_map) else {
            return;
        };
        emit_file_tree_git(json, Some(&self.shell_instance_id));
    }

    /// Get the current tree state (for JSON dump)
    pub fn tree_state(&self) -> &TreeState {
        &self.tree_state
    }

    /// Get a mutable reference to the tree state
    pub fn tree_state_mut(&mut self) -> &mut TreeState {
        &mut self.tree_state
    }
}

/// `tree` builtin command
///
/// Usage:
///   `tree`         — print tree to stdout
///   `tree --json`  — dump full tree as JSON
///   `tree --sync`  — force full sync to terminal via OSC 2337
pub fn builtin_tree(args: &[String]) -> Result<BuiltinResult> {
    let cwd = std::env::current_dir()?;
    let git_cache = Arc::new(GitStatusCache::new(cwd.clone()));
    let mut protocol = TreeProtocol::new(cwd.clone(), git_cache, 4);

    if let Err(e) = protocol.initial_sync() {
        eprintln!("tree: failed to scan directory: {}", e);
        return Ok(BuiltinResult::Failure(1));
    }

    if args.is_empty() {
        let Some(root) = protocol.tree_state().root_node() else {
            eprintln!("tree: no tree data");
            return Ok(BuiltinResult::Failure(1));
        };
        print_tree_node(root, "", true);
        return Ok(BuiltinResult::Success(0));
    }

    match args[0].as_str() {
        "--json" => {
            let Some(root) = protocol.tree_state().root_node() else {
                eprintln!("tree: no tree data");
                return Ok(BuiltinResult::Failure(1));
            };
            let Ok(json) = serde_json::to_string_pretty(root) else {
                eprintln!("tree: failed to serialize");
                return Ok(BuiltinResult::Failure(1));
            };
            println!("{json}");
        }
        "--sync" => {
            if let Err(e) = protocol.force_sync() {
                eprintln!("tree: sync failed: {}", e);
                return Ok(BuiltinResult::Failure(1));
            }
            eprintln!("tree: synced to terminal");
        }
        other => {
            eprintln!("tree: unknown option '{other}'");
            eprintln!("Usage: tree [--json | --sync]");
            return Ok(BuiltinResult::Failure(1));
        }
    }

    Ok(BuiltinResult::Success(0))
}

fn print_tree_node(node: &super::FileNode, prefix: &str, is_last: bool) {
    let connector = if prefix.is_empty() {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let git_indicator = node.git_status.map_or("", |s| match s {
        super::GitFileStatus::Modified => " [M]",
        super::GitFileStatus::Added => " [A]",
        super::GitFileStatus::Deleted => " [D]",
        super::GitFileStatus::Renamed => " [R]",
        super::GitFileStatus::Untracked => " [?]",
        super::GitFileStatus::Conflicted => " [!]",
        _ => "",
    });

    let name_display = if node.is_dir {
        format!("{}/", node.name)
    } else {
        node.name.clone()
    };

    println!("{prefix}{connector}{name_display}{git_indicator}");

    let child_prefix = if prefix.is_empty() {
        String::new()
    } else if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };

    let child_count = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        print_tree_node(child, &child_prefix, i == child_count - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_protocol_creation() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(GitStatusCache::new(dir.path().to_path_buf()));
        let protocol = TreeProtocol::new(dir.path().to_path_buf(), cache, 3);
        assert_eq!(protocol.root(), dir.path());
    }

    #[test]
    fn test_initial_sync() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file.txt"), "content").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir/nested.txt"), "nested").unwrap();

        let cache = Arc::new(GitStatusCache::new(dir.path().to_path_buf()));
        let mut protocol = TreeProtocol::new(dir.path().to_path_buf(), cache, 3);

        protocol.initial_sync().unwrap();

        let root = protocol.tree_state().root_node().unwrap();
        assert!(root.is_dir);
        assert_eq!(root.children.len(), 2);
    }

    #[test]
    fn test_process_tree_change_created() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(GitStatusCache::new(dir.path().to_path_buf()));
        let mut protocol = TreeProtocol::new(dir.path().to_path_buf(), cache, 3);
        protocol.initial_sync().unwrap();

        let new_file = dir.path().join("new.rs");
        std::fs::write(&new_file, "fn main() {}").unwrap();

        let tuple = Tuple::from(vec![
            Value::Str("tree_change".to_string()),
            Value::Str("created".to_string()),
            Value::Str(new_file.display().to_string()),
        ]);

        protocol.process_tree_change(&tuple);
        // The node should be tracked in tree state now
        assert!(protocol.tree_state().get(&new_file).is_some());
    }

    #[test]
    fn test_process_tree_change_removed() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("temp.txt");
        std::fs::write(&file_path, "temp").unwrap();

        let cache = Arc::new(GitStatusCache::new(dir.path().to_path_buf()));
        let mut protocol = TreeProtocol::new(dir.path().to_path_buf(), cache, 3);
        protocol.initial_sync().unwrap();

        let tuple = Tuple::from(vec![
            Value::Str("tree_change".to_string()),
            Value::Str("removed".to_string()),
            Value::Str(file_path.display().to_string()),
        ]);

        protocol.process_tree_change(&tuple);
        assert!(protocol.tree_state().get(&file_path).is_none());
    }

    #[test]
    fn test_process_tree_change_ignores_invalid_tuple() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(GitStatusCache::new(dir.path().to_path_buf()));
        let mut protocol = TreeProtocol::new(dir.path().to_path_buf(), cache, 3);

        // Too few values
        let tuple = Tuple::from(vec![Value::Str("tree_change".to_string())]);
        protocol.process_tree_change(&tuple);

        // Unknown kind
        let tuple = Tuple::from(vec![
            Value::Str("tree_change".to_string()),
            Value::Str("unknown_kind".to_string()),
            Value::Str("/some/path".to_string()),
        ]);
        protocol.process_tree_change(&tuple);
    }

    #[test]
    fn test_change_root() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        std::fs::write(dir2.path().join("new_root.txt"), "content").unwrap();

        let cache = Arc::new(GitStatusCache::new(dir1.path().to_path_buf()));
        let mut protocol = TreeProtocol::new(dir1.path().to_path_buf(), cache, 3);
        protocol.initial_sync().unwrap();

        assert_eq!(protocol.root(), dir1.path());

        // Note: change_root creates a new TreeState, but keeps the same git cache
        // In practice, you'd also update the git cache root
        protocol.change_root(dir2.path().to_path_buf()).unwrap();
        assert_eq!(protocol.root(), dir2.path());

        let root = protocol.tree_state().root_node().unwrap();
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn test_force_sync() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();

        let cache = Arc::new(GitStatusCache::new(dir.path().to_path_buf()));
        let mut protocol = TreeProtocol::new(dir.path().to_path_buf(), cache, 3);
        protocol.initial_sync().unwrap();

        // Add a file after initial sync
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();

        // Force sync should pick it up
        protocol.force_sync().unwrap();
        let root = protocol.tree_state().root_node().unwrap();
        assert_eq!(root.children.len(), 2);
    }
}
