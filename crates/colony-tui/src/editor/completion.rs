//! Completion engine for the input editor.
//!
//! Provides multiple completion sources (commands, paths, variables, history,
//! git branches) with fuzzy matching and scoring.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The kind of a completion item, used for display badges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompletionKind {
    Command,
    Path,
    File,
    Directory,
    Variable,
    History,
    Git,
    Builtin,
}

impl CompletionKind {
    /// Short badge string for display in the completion popup.
    pub fn badge(&self) -> &'static str {
        match self {
            CompletionKind::Command => "CMD",
            CompletionKind::Path => "PATH",
            CompletionKind::File => "FILE",
            CompletionKind::Directory => "DIR",
            CompletionKind::Variable => "VAR",
            CompletionKind::History => "HIST",
            CompletionKind::Git => "GIT",
            CompletionKind::Builtin => "BLT",
        }
    }
}

/// A single completion candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionItem {
    /// The text to insert when this item is selected.
    pub text: String,
    /// Display label (may differ from text, e.g., showing description).
    pub label: String,
    /// Optional description.
    pub description: Option<String>,
    /// The kind of completion.
    pub kind: CompletionKind,
    /// Match score (higher is better).
    pub score: i64,
    /// Character indices in `label` that matched the query (for highlighting).
    pub match_indices: Vec<usize>,
}

/// Context provided to the completion engine.
#[derive(Debug, Clone)]
pub struct CompletionContext {
    /// Current working directory.
    pub cwd: PathBuf,
    /// Directories in $PATH.
    pub path_dirs: Vec<PathBuf>,
    /// Environment variable names.
    pub env_vars: Vec<String>,
    /// Command history entries.
    pub history: Vec<String>,
    /// Built-in command names.
    pub builtins: Vec<String>,
}

impl Default for CompletionContext {
    fn default() -> Self {
        Self {
            cwd: PathBuf::from("."),
            path_dirs: Vec::new(),
            env_vars: Vec::new(),
            history: Vec::new(),
            builtins: Vec::new(),
        }
    }
}

impl CompletionContext {
    /// Build a context from the current environment.
    pub fn from_env(builtins: Vec<String>, history: Vec<String>) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let path_dirs = std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .map(PathBuf::from)
            .collect();
        let env_vars = std::env::vars().map(|(k, _)| k).collect();

        Self {
            cwd,
            path_dirs,
            env_vars,
            history,
            builtins,
        }
    }
}

/// The main completion engine.
#[derive(Debug)]
pub struct CompletionEngine;

impl CompletionEngine {
    /// Generate completions for the given prefix and context.
    ///
    /// Completions are sorted by score (descending).
    pub fn complete(prefix: &str, context: &CompletionContext) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        // Detect what kind of completion to provide based on prefix.
        if prefix.starts_with('$') || prefix.starts_with("${") {
            // Variable completion
            let var_prefix = prefix.trim_start_matches('$').trim_start_matches('{');
            items.extend(complete_variables(var_prefix, &context.env_vars));
        } else if prefix.contains('/') || prefix.starts_with('.') || prefix.starts_with('~') {
            // Path completion
            items.extend(complete_paths(prefix, &context.cwd));
        } else {
            // Command completion (builtins + PATH executables)
            items.extend(complete_commands(
                prefix,
                &context.builtins,
                &context.path_dirs,
            ));
            // Also include history matches
            items.extend(complete_history(prefix, &context.history));
            // Git branch completion if we're in a git context
            items.extend(complete_git_branches(prefix, &context.cwd));
        }

        items.sort_by(|a, b| b.score.cmp(&a.score));
        items
    }
}

/// Complete command names from builtins and PATH executables.
pub fn complete_commands(
    prefix: &str,
    builtins: &[String],
    path_dirs: &[PathBuf],
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    // Builtins first
    for name in builtins {
        if let Some(score) = fuzzy_match(prefix, name) {
            let match_indices = compute_match_indices(prefix, name);
            if seen.insert(name.clone()) {
                items.push(CompletionItem {
                    text: name.clone(),
                    label: name.clone(),
                    description: Some("builtin".to_string()),
                    kind: CompletionKind::Builtin,
                    score: score + 10, // Boost builtins slightly
                    match_indices,
                });
            }
        }
    }

    // PATH executables
    for dir in path_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            let Some(name) = entry.file_name().to_str().map(String::from) else {
                continue;
            };
            let Some(score) = fuzzy_match(prefix, &name) else {
                continue;
            };
            if seen.insert(name.clone()) {
                let match_indices = compute_match_indices(prefix, &name);
                items.push(CompletionItem {
                    text: name.clone(),
                    label: name,
                    description: None,
                    kind: CompletionKind::Command,
                    score,
                    match_indices,
                });
            }
        }
    }

    items
}

/// Complete filesystem paths.
pub fn complete_paths(prefix: &str, cwd: &Path) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    let (dir_path, file_prefix) = if prefix.contains('/') {
        let path = PathBuf::from(prefix);
        if prefix.ends_with('/') {
            (prefix.to_string(), String::new())
        } else {
            let parent = path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let file = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            (
                if parent.is_empty() {
                    ".".to_string()
                } else {
                    parent
                },
                file,
            )
        }
    } else {
        (".".to_string(), prefix.to_string())
    };

    // Expand ~ to home directory
    let expanded_dir = if dir_path.starts_with('~') {
        std::env::var("HOME")
            .map(|home| dir_path.replacen('~', &home, 1))
            .unwrap_or_else(|_| dir_path.clone())
    } else if dir_path == "." {
        cwd.to_string_lossy().to_string()
    } else if !dir_path.starts_with('/') {
        cwd.join(&dir_path).to_string_lossy().to_string()
    } else {
        dir_path.clone()
    };

    let Ok(entries) = std::fs::read_dir(&expanded_dir) else {
        return items;
    };

    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(String::from) else {
            continue;
        };

        // Skip hidden files unless prefix starts with .
        if name.starts_with('.') && !file_prefix.starts_with('.') {
            continue;
        }

        let Some(score) = fuzzy_match(&file_prefix, &name) else {
            continue;
        };

        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let kind = if is_dir {
            CompletionKind::Directory
        } else {
            CompletionKind::File
        };

        let display = if dir_path == "." {
            name.clone()
        } else if dir_path.ends_with('/') {
            format!("{}{}", dir_path, name)
        } else {
            format!("{}/{}", dir_path, name)
        };

        let text = if is_dir {
            format!("{}/", display)
        } else {
            display.clone()
        };

        let match_indices = compute_match_indices(&file_prefix, &name);

        items.push(CompletionItem {
            text,
            label: display,
            description: if is_dir {
                Some("directory".to_string())
            } else {
                None
            },
            kind,
            score,
            match_indices,
        });
    }

    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

/// Complete environment variable names.
pub fn complete_variables(prefix: &str, env_vars: &[String]) -> Vec<CompletionItem> {
    env_vars
        .iter()
        .filter_map(|var| {
            let score = fuzzy_match(prefix, var)?;
            let match_indices = compute_match_indices(prefix, var);
            Some(CompletionItem {
                text: format!("${}", var),
                label: var.clone(),
                description: std::env::var(var).ok(),
                kind: CompletionKind::Variable,
                score,
                match_indices,
            })
        })
        .collect()
}

/// Complete from history entries containing the prefix.
pub fn complete_history(prefix: &str, history: &[String]) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    // Iterate in reverse (most recent first) for recency boost
    for (idx, entry) in history.iter().rev().enumerate() {
        let Some(score) = fuzzy_match(prefix, entry) else {
            continue;
        };
        if !seen.insert(entry.clone()) {
            continue;
        }

        // Recency boost: more recent = higher score bonus
        let recency_bonus = (100i64).saturating_sub(idx as i64).max(0);
        let match_indices = compute_match_indices(prefix, entry);

        items.push(CompletionItem {
            text: entry.clone(),
            label: entry.clone(),
            description: Some("history".to_string()),
            kind: CompletionKind::History,
            score: score + recency_bonus,
            match_indices,
        });
    }

    items
}

/// Complete git branch names if in a git repository.
pub fn complete_git_branches(prefix: &str, cwd: &Path) -> Vec<CompletionItem> {
    // Check if we're in a git repo by looking for .git
    let mut check = cwd.to_path_buf();
    let mut is_git = false;
    loop {
        if check.join(".git").exists() {
            is_git = true;
            break;
        }
        if !check.pop() {
            break;
        }
    }

    if !is_git {
        return Vec::new();
    }

    // Read branches from git
    let Ok(output) = std::process::Command::new("git")
        .args(["branch", "--no-color", "-a"])
        .current_dir(cwd)
        .output()
    else {
        return Vec::new();
    };

    let Ok(stdout) = std::str::from_utf8(&output.stdout) else {
        return Vec::new();
    };

    stdout
        .lines()
        .filter_map(|line| {
            let branch = line
                .trim()
                .trim_start_matches("* ")
                .trim_start_matches("remotes/");
            if branch.contains("HEAD ->") {
                return None;
            }
            let score = fuzzy_match(prefix, branch)?;
            let match_indices = compute_match_indices(prefix, branch);
            Some(CompletionItem {
                text: branch.to_string(),
                label: branch.to_string(),
                description: Some("git branch".to_string()),
                kind: CompletionKind::Git,
                score,
                match_indices,
            })
        })
        .collect()
}

// ── Fuzzy matching ────────────────────────────────────────────

/// Fuzzy match a query against a candidate string.
///
/// Returns `Some(score)` if the query matches, `None` otherwise.
///
/// Scoring: exact prefix > word boundary match > substring > fuzzy character skip
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }

    let query_lower = query.to_lowercase();
    let candidate_lower = candidate.to_lowercase();

    // Exact prefix match (highest score)
    if candidate_lower.starts_with(&query_lower) {
        // Prefer shorter candidates for prefix matches (more specific)
        let length_penalty = candidate.len() as i64;
        return Some(1000 - length_penalty);
    }

    // Word-boundary match: check if query chars appear at word boundaries
    let word_boundary_score = word_boundary_match(&query_lower, &candidate_lower);
    if let Some(score) = word_boundary_score {
        return Some(500 + score);
    }

    // Substring match
    if candidate_lower.contains(&query_lower) {
        let pos = candidate_lower.find(&query_lower).unwrap_or(0) as i64;
        return Some(300 - pos);
    }

    // Fuzzy character-skip match
    let mut query_chars = query_lower.chars();
    let mut current_query = query_chars.next()?;
    let mut score = 0i64;
    let mut consecutive = 0i64;

    for c in candidate_lower.chars() {
        if c == current_query {
            score += 10 + consecutive * 5;
            consecutive += 1;
            match query_chars.next() {
                Some(next) => current_query = next,
                None => return Some(score),
            }
        } else {
            consecutive = 0;
        }
    }

    None // Not all query chars were matched
}

/// Check if query characters appear at word boundaries in the candidate.
fn word_boundary_match(query: &str, candidate: &str) -> Option<i64> {
    let candidate_chars: Vec<char> = candidate.chars().collect();
    let mut query_iter = query.chars();
    let mut current_query = query_iter.next()?;
    let mut score = 0i64;

    for (i, &c) in candidate_chars.iter().enumerate() {
        if c != current_query {
            continue;
        }

        // Check if this position is a word boundary
        let is_boundary = i == 0
            || candidate_chars[i - 1] == '_'
            || candidate_chars[i - 1] == '-'
            || candidate_chars[i - 1] == '/'
            || (candidate_chars[i - 1].is_lowercase() && c.is_uppercase());

        if !is_boundary {
            continue;
        }

        score += 10;
        match query_iter.next() {
            Some(next) => current_query = next,
            None => return Some(score),
        }
    }

    None
}

/// Compute which character indices in `candidate` match the `query` (for highlighting).
pub fn compute_match_indices(query: &str, candidate: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let candidate_lower = candidate.to_lowercase();

    // Try prefix match first
    if candidate_lower.starts_with(&query_lower) {
        return (0..query.len()).collect();
    }

    // Try substring match
    if let Some(start) = candidate_lower.find(&query_lower) {
        return (start..start + query.len()).collect();
    }

    // Fuzzy match indices
    let mut indices = Vec::new();
    let mut query_chars = query_lower.chars();
    let Some(mut current) = query_chars.next() else {
        return indices;
    };

    for (i, c) in candidate_lower.chars().enumerate() {
        if c == current {
            indices.push(i);
            match query_chars.next() {
                Some(next) => current = next,
                None => break,
            }
        }
    }

    indices
}

// ── Ghost completion ──────────────────────────────────────────

/// Generate ghost text (inline dimmed suggestion) from history.
///
/// Returns the suffix to display after the current input, or None if no match.
pub fn ghost_text(prefix: &str, history: &[String]) -> Option<String> {
    if prefix.is_empty() {
        return None;
    }

    // Find the most recent history entry that starts with the prefix
    for entry in history.iter().rev() {
        if entry.starts_with(prefix) && entry.len() > prefix.len() {
            return Some(entry[prefix.len()..].to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_exact_prefix() {
        let score = fuzzy_match("ls", "ls").unwrap();
        assert!(score > 900);
    }

    #[test]
    fn test_fuzzy_match_prefix() {
        let score = fuzzy_match("gi", "git").unwrap();
        assert!(score > 900);
    }

    #[test]
    fn test_fuzzy_match_substring() {
        let score = fuzzy_match("status", "git-status").unwrap();
        assert!(score > 200);
    }

    #[test]
    fn test_fuzzy_match_fuzzy() {
        let score = fuzzy_match("gst", "git-status");
        assert!(score.is_some());
    }

    #[test]
    fn test_fuzzy_match_no_match() {
        let score = fuzzy_match("xyz", "abc");
        assert!(score.is_none());
    }

    #[test]
    fn test_fuzzy_match_empty_query() {
        let score = fuzzy_match("", "anything");
        assert_eq!(score, Some(0));
    }

    #[test]
    fn test_fuzzy_match_case_insensitive() {
        let score = fuzzy_match("GIT", "git");
        assert!(score.is_some());
    }

    #[test]
    fn test_exact_prefix_scores_higher_than_substring() {
        let prefix_score = fuzzy_match("ls", "ls-files").unwrap();
        let substring_score = fuzzy_match("ls", "als").unwrap();
        assert!(prefix_score > substring_score);
    }

    #[test]
    fn test_compute_match_indices_prefix() {
        let indices = compute_match_indices("gi", "git");
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn test_compute_match_indices_substring() {
        let indices = compute_match_indices("stat", "git-status");
        assert_eq!(indices, vec![4, 5, 6, 7]);
    }

    #[test]
    fn test_compute_match_indices_fuzzy() {
        let indices = compute_match_indices("gst", "git-status");
        assert_eq!(indices.len(), 3);
        assert_eq!(indices[0], 0); // 'g' in "git"
    }

    #[test]
    fn test_ghost_text_match() {
        let history = vec![
            "git status".to_string(),
            "git commit -m 'test'".to_string(),
            "git push origin main".to_string(),
        ];
        let ghost = ghost_text("git p", &history);
        assert_eq!(ghost, Some("ush origin main".to_string()));
    }

    #[test]
    fn test_ghost_text_no_match() {
        let history = vec!["ls -la".to_string()];
        let ghost = ghost_text("git", &history);
        assert!(ghost.is_none());
    }

    #[test]
    fn test_ghost_text_empty_prefix() {
        let history = vec!["ls".to_string()];
        let ghost = ghost_text("", &history);
        assert!(ghost.is_none());
    }

    #[test]
    fn test_ghost_text_exact_match_returns_none() {
        let history = vec!["ls".to_string()];
        let ghost = ghost_text("ls", &history);
        assert!(ghost.is_none());
    }

    #[test]
    fn test_complete_variables() {
        let vars = vec!["HOME".to_string(), "HOST".to_string(), "PATH".to_string()];
        let items = complete_variables("HO", &vars);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.kind == CompletionKind::Variable));
    }

    #[test]
    fn test_complete_history() {
        let history = vec![
            "git status".to_string(),
            "ls -la".to_string(),
            "git push".to_string(),
        ];
        let items = complete_history("git", &history);
        assert_eq!(items.len(), 2);
        // Most recent (git push) should score higher due to recency
        assert_eq!(items[0].text, "git push");
    }

    #[test]
    fn test_completion_kind_badge() {
        assert_eq!(CompletionKind::Command.badge(), "CMD");
        assert_eq!(CompletionKind::Path.badge(), "PATH");
        assert_eq!(CompletionKind::Variable.badge(), "VAR");
        assert_eq!(CompletionKind::History.badge(), "HIST");
        assert_eq!(CompletionKind::Git.badge(), "GIT");
        assert_eq!(CompletionKind::Builtin.badge(), "BLT");
        assert_eq!(CompletionKind::File.badge(), "FILE");
        assert_eq!(CompletionKind::Directory.badge(), "DIR");
    }

    #[test]
    fn test_completion_context_default() {
        let ctx = CompletionContext::default();
        assert!(ctx.builtins.is_empty());
        assert!(ctx.history.is_empty());
        assert!(ctx.env_vars.is_empty());
    }

    #[test]
    fn test_word_boundary_match() {
        // "gs" should match "git-status" at word boundaries (g, s)
        let score = word_boundary_match("gs", "git-status");
        assert!(score.is_some());
    }

    #[test]
    fn test_word_boundary_no_match() {
        let score = word_boundary_match("xyz", "git-status");
        assert!(score.is_none());
    }
}
