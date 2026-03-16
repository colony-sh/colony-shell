//! Git-aware recommendation source.

use std::process::Command;

use crate::input::RecommendInput;
use crate::scoring::fuzzy_score;
use crate::source::{RecommendKind, RecommendSource, Recommendation};

/// Git subcommands and branch name recommendations.
pub struct GitSource;

const GIT_SUBCOMMANDS: &[(&str, &str)] = &[
    ("add", "Add file contents to the index"),
    ("bisect", "Use binary search to find a bad commit"),
    ("branch", "List, create, or delete branches"),
    ("checkout", "Switch branches or restore files"),
    ("cherry-pick", "Apply changes from specific commits"),
    ("clone", "Clone a repository"),
    ("commit", "Record changes to the repository"),
    ("diff", "Show changes between commits"),
    ("fetch", "Download objects and refs from remote"),
    ("init", "Create an empty repository"),
    ("log", "Show commit logs"),
    ("merge", "Join development histories together"),
    ("mv", "Move or rename a file"),
    ("pull", "Fetch and integrate from remote"),
    ("push", "Update remote refs"),
    ("rebase", "Reapply commits on top of another base"),
    ("remote", "Manage set of tracked repositories"),
    ("reset", "Reset current HEAD to specified state"),
    ("restore", "Restore working tree files"),
    ("rm", "Remove files from working tree and index"),
    ("show", "Show various types of objects"),
    ("stash", "Stash changes in a dirty working directory"),
    ("status", "Show the working tree status"),
    ("switch", "Switch branches"),
    ("tag", "Create, list, delete or verify tags"),
    ("worktree", "Manage multiple working trees"),
];

impl RecommendSource for GitSource {
    fn recommend(&self, input: &RecommendInput) -> Vec<Recommendation> {
        let Some(ref cmd) = input.command else {
            return Vec::new();
        };
        if cmd != "git" {
            return Vec::new();
        }

        if input.arg_position == 1 {
            // Complete git subcommands
            return GIT_SUBCOMMANDS
                .iter()
                .filter_map(|(sub, desc)| {
                    let score = fuzzy_score(&input.word, sub);
                    if score < 0.1 && !input.word.is_empty() {
                        return None;
                    }
                    let text = if input.word.is_empty() {
                        sub.to_string()
                    } else if sub.starts_with(&*input.word) {
                        sub[input.word.len()..].to_string()
                    } else {
                        sub.to_string()
                    };
                    Some(Recommendation {
                        text,
                        label: sub.to_string(),
                        description: Some(desc.to_string()),
                        kind: RecommendKind::Git,
                        confidence: if input.word.is_empty() { 0.3 } else { score },
                        match_indices: Vec::new(),
                    })
                })
                .collect();
        }

        // For checkout/switch/merge/rebase: complete branch names
        let subcmd = input.line.split_whitespace().nth(1).unwrap_or("");
        if !matches!(
            subcmd,
            "checkout" | "switch" | "merge" | "rebase" | "branch"
        ) {
            return Vec::new();
        }

        let Ok(output) = Command::new("git")
            .args(["branch", "--format=%(refname:short)"])
            .current_dir(&input.cwd)
            .output()
        else {
            return Vec::new();
        };

        let branches = String::from_utf8_lossy(&output.stdout);
        branches
            .lines()
            .filter_map(|branch| {
                let branch = branch.trim();
                if branch.is_empty() {
                    return None;
                }
                let score = fuzzy_score(&input.word, branch);
                if score < 0.1 && !input.word.is_empty() {
                    return None;
                }
                let text = if input.word.is_empty() {
                    branch.to_string()
                } else if branch.starts_with(&*input.word) {
                    branch[input.word.len()..].to_string()
                } else {
                    branch.to_string()
                };
                Some(Recommendation {
                    text,
                    label: branch.to_string(),
                    description: None,
                    kind: RecommendKind::Git,
                    confidence: if input.word.is_empty() { 0.4 } else { score },
                    match_indices: Vec::new(),
                })
            })
            .take(20)
            .collect()
    }

    fn name(&self) -> &'static str {
        "git"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn suggests_subcommands() {
        let source = GitSource;
        let input = RecommendInput::parse("git com", 7, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.label == "commit"));
    }

    #[test]
    fn suggests_all_subcommands_on_empty() {
        let source = GitSource;
        let input = RecommendInput::parse("git ", 4, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(results.len() >= 10);
        assert!(results.iter().all(|r| r.kind == RecommendKind::Git));
    }

    #[test]
    fn ignores_non_git_commands() {
        let source = GitSource;
        let input = RecommendInput::parse("cargo ", 6, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(results.is_empty());
    }

    #[test]
    fn ignores_empty_input() {
        let source = GitSource;
        let input = RecommendInput::parse("", 0, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(results.is_empty());
    }

    #[test]
    fn subcommand_ghost_text() {
        let source = GitSource;
        let input = RecommendInput::parse("git st", 6, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        // Should find "stash", "status", "switch" etc.
        let status = results.iter().find(|r| r.label == "status");
        assert!(status.is_some());
        assert_eq!(status.unwrap().text, "atus");
    }
}
