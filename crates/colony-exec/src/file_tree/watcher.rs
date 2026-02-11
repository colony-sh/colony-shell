//! File system watcher using notify v6
//!
//! Watches a directory for changes and emits tree_change tuples
//! to the tuple space. Also invalidates the GitStatusCache when
//! .git directory changes are detected.

use super::git_status::GitStatusCache;
use colony_tuple::{Tuple, TupleSpace, Value};
use notify::{
    event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, warn};

/// File system watcher that emits tuple space events on changes
pub struct FileSystemWatcher {
    _watcher: RecommendedWatcher,
    root: PathBuf,
}

impl FileSystemWatcher {
    /// Start watching a directory.
    ///
    /// Emits `("tree_change", kind, path)` tuples when files change.
    /// Kinds: "created", "modified", "removed", "renamed"
    pub fn start(
        root: &Path,
        tuple_space: Arc<TupleSpace>,
        git_cache: Arc<GitStatusCache>,
    ) -> notify::Result<Self> {
        let root_path = root.to_path_buf();
        let root_for_filter = root.to_path_buf();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let event = match res {
                Ok(e) => e,
                Err(e) => {
                    warn!("File watcher error: {}", e);
                    return;
                }
            };

            let kind_str = match event.kind {
                EventKind::Create(CreateKind::File | CreateKind::Folder) => "created",
                EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Name(RenameMode::To)) => {
                    "modified"
                }
                EventKind::Remove(RemoveKind::File | RemoveKind::Folder) => "removed",
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => "renamed",
                _ => return,
            };

            for path in &event.paths {
                // Skip hidden dirs except .git detection
                if Self::is_git_change(path) {
                    git_cache.invalidate();
                    continue;
                }

                if Self::should_ignore(path, &root_for_filter) {
                    continue;
                }

                debug!("File {}: {}", kind_str, path.display());

                tuple_space.out(Tuple::from(vec![
                    Value::Str("tree_change".to_string()),
                    Value::Str(kind_str.to_string()),
                    Value::Str(path.display().to_string()),
                ]));
            }
        })?;

        watcher.watch(root, RecursiveMode::Recursive)?;
        watcher.configure(Config::default().with_poll_interval(std::time::Duration::from_secs(2)))?;

        Ok(Self {
            _watcher: watcher,
            root: root_path,
        })
    }

    /// Get the watched root directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn is_git_change(path: &Path) -> bool {
        path.components()
            .any(|c| c.as_os_str() == ".git")
    }

    fn should_ignore(path: &Path, _root: &Path) -> bool {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();

        // Skip hidden files
        if name.starts_with('.') {
            return true;
        }

        // Skip common build/dependency directories
        path.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            matches!(
                s.as_ref(),
                "node_modules" | "target" | "__pycache__" | ".git" | "dist" | "build" | "vendor"
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_git_change() {
        assert!(FileSystemWatcher::is_git_change(Path::new(
            "/project/.git/index"
        )));
        assert!(FileSystemWatcher::is_git_change(Path::new(
            "/project/.git/refs/heads/main"
        )));
        assert!(!FileSystemWatcher::is_git_change(Path::new(
            "/project/src/main.rs"
        )));
    }

    #[test]
    fn test_should_ignore() {
        let root = PathBuf::from("/project");
        assert!(FileSystemWatcher::should_ignore(
            Path::new("/project/.hidden"),
            &root
        ));
        assert!(FileSystemWatcher::should_ignore(
            Path::new("/project/node_modules/dep/index.js"),
            &root
        ));
        assert!(FileSystemWatcher::should_ignore(
            Path::new("/project/target/debug/main"),
            &root
        ));
        assert!(!FileSystemWatcher::should_ignore(
            Path::new("/project/src/main.rs"),
            &root
        ));
    }
}
