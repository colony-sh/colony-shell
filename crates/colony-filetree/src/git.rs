//! Git status integration

use git2::{Repository, Status, StatusOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Git file status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitFileStatus {
    /// Untracked file
    Untracked,
    /// New file (staged)
    New,
    /// Modified (unstaged)
    Modified,
    /// Modified (staged)
    Staged,
    /// Deleted
    Deleted,
    /// Renamed
    Renamed,
    /// Conflicted
    Conflicted,
    /// Ignored
    Ignored,
    /// Clean (tracked, no changes)
    Clean,
}

impl GitFileStatus {
    /// Get an icon for this status
    pub fn icon(&self) -> &'static str {
        match self {
            GitFileStatus::Untracked => "?",
            GitFileStatus::New => "+",
            GitFileStatus::Modified => "M",
            GitFileStatus::Staged => "S",
            GitFileStatus::Deleted => "D",
            GitFileStatus::Renamed => "R",
            GitFileStatus::Conflicted => "!",
            GitFileStatus::Ignored => "I",
            GitFileStatus::Clean => " ",
        }
    }

    /// Get a color hint for this status (for rendering)
    pub fn color(&self) -> &'static str {
        match self {
            GitFileStatus::Untracked => "gray",
            GitFileStatus::New => "green",
            GitFileStatus::Modified => "yellow",
            GitFileStatus::Staged => "green",
            GitFileStatus::Deleted => "red",
            GitFileStatus::Renamed => "cyan",
            GitFileStatus::Conflicted => "red",
            GitFileStatus::Ignored => "gray",
            GitFileStatus::Clean => "default",
        }
    }

    /// Parse from git2 Status flags
    fn from_git2_status(status: Status) -> Option<Self> {
        if status.is_conflicted() {
            return Some(GitFileStatus::Conflicted);
        }
        if status.is_ignored() {
            return Some(GitFileStatus::Ignored);
        }
        if status.is_wt_new() {
            return Some(GitFileStatus::Untracked);
        }
        if status.is_index_new() {
            return Some(GitFileStatus::New);
        }
        if status.is_index_modified() || status.is_index_renamed() || status.is_index_typechange() {
            return Some(GitFileStatus::Staged);
        }
        if status.is_wt_modified() || status.is_wt_typechange() {
            return Some(GitFileStatus::Modified);
        }
        if status.is_wt_deleted() {
            return Some(GitFileStatus::Deleted);
        }
        if status.is_index_deleted() {
            return Some(GitFileStatus::Deleted);
        }
        if status.is_wt_renamed() || status.is_index_renamed() {
            return Some(GitFileStatus::Renamed);
        }

        // No status flags = clean
        None
    }
}

/// Cache of git statuses for a repository
#[derive(Debug)]
pub struct GitStatusCache {
    /// Repository root path
    repo_root: PathBuf,
    /// Cached statuses by relative path
    statuses: HashMap<PathBuf, GitFileStatus>,
    /// Current branch name
    branch: Option<String>,
    /// Whether repo is dirty (has uncommitted changes)
    dirty: bool,
    /// Number of commits ahead of upstream
    ahead: usize,
    /// Number of commits behind upstream
    behind: usize,
}

impl GitStatusCache {
    /// Create a new cache for a path (finds git repo)
    pub fn new(path: &Path) -> Result<Self, git2::Error> {
        let repo = Repository::discover(path)?;
        let repo_root = repo
            .workdir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| path.to_path_buf());

        let mut cache = Self {
            repo_root,
            statuses: HashMap::new(),
            branch: None,
            dirty: false,
            ahead: 0,
            behind: 0,
        };

        cache.refresh(&repo)?;
        Ok(cache)
    }

    /// Refresh the cache from the repository
    pub fn refresh(&mut self, repo: &Repository) -> Result<(), git2::Error> {
        self.statuses.clear();

        // Get branch info
        self.branch = repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(|s| s.to_string()));

        // Get ahead/behind counts
        if let Ok(head) = repo.head() {
            if let Some(branch_name) = head.shorthand() {
                if let Ok(local) = repo.find_branch(branch_name, git2::BranchType::Local) {
                    if let Ok(upstream) = local.upstream() {
                        if let (Some(local_oid), Some(upstream_oid)) =
                            (local.get().target(), upstream.get().target())
                        {
                            if let Ok((ahead, behind)) =
                                repo.graph_ahead_behind(local_oid, upstream_oid)
                            {
                                self.ahead = ahead;
                                self.behind = behind;
                            }
                        }
                    }
                }
            }
        }

        // Get file statuses
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .include_ignored(false)
            .include_unmodified(false)
            .recurse_untracked_dirs(true);

        let statuses = repo.statuses(Some(&mut opts))?;

        self.dirty = !statuses.is_empty();

        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                if let Some(status) = GitFileStatus::from_git2_status(entry.status()) {
                    self.statuses.insert(PathBuf::from(path), status);
                }
            }
        }

        Ok(())
    }

    /// Get status for a path (relative to repo root)
    pub fn status(&self, path: &Path) -> Option<GitFileStatus> {
        // Try to make path relative to repo root
        let relative = path
            .strip_prefix(&self.repo_root)
            .ok()
            .or(Some(path));

        relative.and_then(|rel| self.statuses.get(rel).copied())
    }

    /// Get status for a path, checking parent directories too
    pub fn status_recursive(&self, path: &Path) -> Option<GitFileStatus> {
        // First check exact path
        if let Some(status) = self.status(path) {
            return Some(status);
        }

        // Check if any child is modified (for directories)
        let relative = path.strip_prefix(&self.repo_root).unwrap_or(path);

        for (file_path, status) in &self.statuses {
            if file_path.starts_with(relative) {
                // Return the "worst" status found
                return Some(*status);
            }
        }

        None
    }

    /// Get the current branch name
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// Check if repo has uncommitted changes
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Get commits ahead of upstream
    pub fn ahead(&self) -> usize {
        self.ahead
    }

    /// Get commits behind upstream
    pub fn behind(&self) -> usize {
        self.behind
    }

    /// Get all modified files
    pub fn modified_files(&self) -> Vec<(&Path, GitFileStatus)> {
        self.statuses
            .iter()
            .filter(|(_, status)| !matches!(status, GitFileStatus::Clean | GitFileStatus::Ignored))
            .map(|(path, status)| (path.as_path(), *status))
            .collect()
    }

    /// Get count of files by status
    pub fn status_counts(&self) -> HashMap<GitFileStatus, usize> {
        let mut counts = HashMap::new();
        for status in self.statuses.values() {
            *counts.entry(*status).or_insert(0) += 1;
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_icons() {
        assert_eq!(GitFileStatus::Modified.icon(), "M");
        assert_eq!(GitFileStatus::Untracked.icon(), "?");
        assert_eq!(GitFileStatus::Staged.icon(), "S");
    }

    #[test]
    fn test_status_colors() {
        assert_eq!(GitFileStatus::Modified.color(), "yellow");
        assert_eq!(GitFileStatus::Deleted.color(), "red");
        assert_eq!(GitFileStatus::New.color(), "green");
    }

    // Note: Full git tests require a real git repository
    // These are tested in integration tests
}
