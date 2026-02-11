//! Git status caching for file tree annotations
//!
//! Runs `git status --porcelain` and caches results with a 500ms TTL.
//! Invalidated on .git directory changes.

use super::tree_state::GitFileStatus;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const GIT_CACHE_TTL: Duration = Duration::from_millis(500);

/// Cached git status results for a repository
pub struct GitStatusCache {
    root: PathBuf,
    cache: Mutex<CacheEntry>,
}

struct CacheEntry {
    statuses: HashMap<PathBuf, GitFileStatus>,
    last_updated: Option<Instant>,
}

impl GitStatusCache {
    /// Create a new cache for the given repository root
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            cache: Mutex::new(CacheEntry {
                statuses: HashMap::new(),
                last_updated: None,
            }),
        }
    }

    /// Get cached git statuses, refreshing if stale
    pub fn statuses(&self) -> HashMap<PathBuf, GitFileStatus> {
        let mut entry = self.cache.lock();

        let should_refresh = entry
            .last_updated
            .map(|t| t.elapsed() > GIT_CACHE_TTL)
            .unwrap_or(true);

        if should_refresh {
            entry.statuses = Self::run_git_status(&self.root);
            entry.last_updated = Some(Instant::now());
        }

        entry.statuses.clone()
    }

    /// Force cache invalidation
    pub fn invalidate(&self) {
        let mut entry = self.cache.lock();
        entry.last_updated = None;
    }

    /// Check if the root is a git repository
    pub fn is_git_repo(&self) -> bool {
        self.root.join(".git").exists()
    }

    fn run_git_status(root: &Path) -> HashMap<PathBuf, GitFileStatus> {
        let output = Command::new("git")
            .args(["status", "--porcelain", "-uall"])
            .current_dir(root)
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return HashMap::new(),
        };

        let stdout = match std::str::from_utf8(&output.stdout) {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };

        let mut statuses = HashMap::new();

        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }

            let index_status = line.as_bytes()[0] as char;
            let work_status = line.as_bytes()[1] as char;
            let file_path_str = &line[3..];

            // For renames: "R  old -> new", take the new path
            let file_path_str = if let Some(arrow_pos) = file_path_str.find(" -> ") {
                &file_path_str[arrow_pos + 4..]
            } else {
                file_path_str
            };

            let abs_path = root.join(file_path_str);

            // Prefer the working tree status over index status
            let status = if work_status != ' ' && work_status != '?' && work_status != '!' {
                GitFileStatus::from_porcelain_char(work_status)
            } else {
                GitFileStatus::from_porcelain_char(index_status)
            };

            statuses.insert(abs_path, status);
        }

        statuses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_creation() {
        let cache = GitStatusCache::new(PathBuf::from("/tmp/test"));
        let statuses = cache.statuses();
        // No git repo at /tmp/test, so empty
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_invalidate() {
        let cache = GitStatusCache::new(PathBuf::from("/tmp/test"));
        let _ = cache.statuses();
        cache.invalidate();
        // After invalidation, next call refreshes
        let _ = cache.statuses();
    }

    #[test]
    fn test_is_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        let cache = GitStatusCache::new(dir.path().to_path_buf());
        assert!(!cache.is_git_repo());
    }
}
