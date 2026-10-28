# F074: Command Recommendation Engine — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a `colony-recommend` crate that unifies and extends the existing completion sources (history, path, command, git) with confidence scoring, directory-aware filtering, and an AI-powered suggestion source.

**Architecture:** Extract core recommendation logic from `colony-tui`'s `GhostCompletion` and `CompletionEngine` into a new `colony-recommend` crate. Add a `RecommendationEngine` that manages multiple `RecommendSource` trait implementations, scores by confidence, and filters by directory context. Wire the engine back into `colony-tui` replacing direct completion calls.

**Tech Stack:** Rust, existing colony-agent for AI suggestions, existing colony-memory for semantic scoring

**Important:** The existing crate structure uses `colony-*` naming. The workspace is at `/Users/mcfearsome/src/github.com/colony-sh/colony-shell/`. Cargo.toml workspace members must be updated. Follow the Rust style guide in `RUST_STYLE_GUIDE.md` — especially: no nested `if let`, use `let-else`, combinators over pyramids, `thiserror` for errors.

---

## File Structure

```
crates/colony-recommend/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API: RecommendationEngine, Recommendation, RecommendInput
│   ├── engine.rs           # RecommendationEngine: orchestrates sources, scores, filters
│   ├── input.rs            # RecommendInput: parsed line context (command, arg position, word)
│   ├── source.rs           # RecommendSource trait + Recommendation type
│   ├── sources/
│   │   ├── mod.rs          # Re-exports
│   │   ├── history.rs      # HistorySource: frequency-weighted history matching
│   │   ├── command.rs      # CommandSource: PATH + builtins
│   │   ├── path.rs         # PathSource: file/directory completion
│   │   ├── git.rs          # GitSource: branches, remotes, subcommands
│   │   └── ai.rs           # AiSource: async LLM-powered suggestions (optional)
│   └── scoring.rs          # Confidence scoring and ranking
```

**Existing files to modify:**
- `Cargo.toml` (workspace root) — add `colony-recommend` member
- `crates/colony-tui/Cargo.toml` — add `colony-recommend` dependency
- `crates/colony-tui/src/app.rs` — wire RecommendationEngine into ghost text rendering
- `crates/colony-tui/src/input/completion.rs` — delegate to colony-recommend

---

## Chunk 1: Crate Skeleton + Core Types

### Task 1: Create colony-recommend crate

**Files:**
- Create: `crates/colony-recommend/Cargo.toml`
- Create: `crates/colony-recommend/src/lib.rs`
- Create: `crates/colony-recommend/src/source.rs`
- Create: `crates/colony-recommend/src/input.rs`
- Create: `crates/colony-recommend/src/scoring.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "colony-recommend"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
description = "Command recommendation engine for Colony Shell"

[dependencies]
thiserror = "2.0"
serde = { version = "1", features = ["derive"] }

[dev-dependencies]
```

- [ ] **Step 2: Create source.rs with core types**

```rust
//! Recommendation source trait and core types.

use serde::{Deserialize, Serialize};

/// Where a recommendation came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecommendKind {
    History,
    Command,
    Path,
    File,
    Directory,
    Variable,
    Git,
    Builtin,
    Ai,
}

impl RecommendKind {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::History => "HIST",
            Self::Command => "CMD",
            Self::Path => "PATH",
            Self::File => "FILE",
            Self::Directory => "DIR",
            Self::Variable => "VAR",
            Self::Git => "GIT",
            Self::Builtin => "BLT",
            Self::Ai => "AI",
        }
    }
}

/// A single recommendation.
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// Text to insert (replaces current word or appends).
    pub text: String,
    /// Display label.
    pub label: String,
    /// Optional description.
    pub description: Option<String>,
    /// Source kind.
    pub kind: RecommendKind,
    /// Confidence score 0.0–1.0.
    pub confidence: f64,
    /// Character indices in label that matched (for highlighting).
    pub match_indices: Vec<usize>,
}

/// Trait for recommendation sources.
pub trait RecommendSource: Send + Sync {
    /// Generate recommendations for the given input context.
    fn recommend(&self, input: &super::input::RecommendInput) -> Vec<Recommendation>;

    /// Source identifier for debugging.
    fn name(&self) -> &'static str;
}
```

- [ ] **Step 3: Create input.rs**

```rust
//! Parsed input context for recommendation generation.

use std::path::PathBuf;

/// Parsed context from the current command line input.
#[derive(Debug, Clone)]
pub struct RecommendInput {
    /// Full input line.
    pub line: String,
    /// Current word being typed (under cursor).
    pub word: String,
    /// Cursor position in the line.
    pub cursor: usize,
    /// The command (first word), if any.
    pub command: Option<String>,
    /// Argument position (0 = command itself, 1 = first arg, etc.)
    pub arg_position: usize,
    /// Current working directory.
    pub cwd: PathBuf,
}

impl RecommendInput {
    /// Parse a raw input line at cursor position.
    pub fn parse(line: &str, cursor: usize, cwd: PathBuf) -> Self {
        let before_cursor = &line[..cursor.min(line.len())];
        let words: Vec<&str> = before_cursor.split_whitespace().collect();

        let command = words.first().map(|s| s.to_string());
        let arg_position = if words.is_empty() { 0 } else { words.len() - 1 };

        // Current word: last whitespace-delimited token, or empty if ends with space
        let word = if before_cursor.ends_with(' ') || before_cursor.is_empty() {
            String::new()
        } else {
            words.last().unwrap_or(&"").to_string()
        };

        Self {
            line: line.to_string(),
            word,
            cursor,
            command,
            arg_position,
            cwd,
        }
    }

    /// Whether we're completing the command name (position 0).
    pub fn is_command_position(&self) -> bool {
        self.arg_position == 0
    }
}
```

- [ ] **Step 4: Create scoring.rs**

```rust
//! Confidence scoring utilities.

use super::source::Recommendation;

/// Score a prefix match (0.0–1.0).
pub fn prefix_score(query: &str, candidate: &str) -> f64 {
    if candidate.starts_with(query) {
        0.8 + (0.2 * query.len() as f64 / candidate.len().max(1) as f64)
    } else {
        0.0
    }
}

/// Score a fuzzy match (0.0–1.0).
pub fn fuzzy_score(query: &str, candidate: &str) -> f64 {
    let query_lower = query.to_lowercase();
    let candidate_lower = candidate.to_lowercase();

    // Exact prefix is best
    if candidate_lower.starts_with(&query_lower) {
        return prefix_score(query, candidate);
    }

    // Substring match
    if candidate_lower.contains(&query_lower) {
        return 0.5 + (0.2 * query.len() as f64 / candidate.len().max(1) as f64);
    }

    // Character-by-character fuzzy
    let mut query_chars = query_lower.chars().peekable();
    let mut matched = 0;
    for c in candidate_lower.chars() {
        if query_chars.peek() == Some(&c) {
            query_chars.next();
            matched += 1;
        }
    }

    if query_chars.peek().is_some() {
        return 0.0; // Not all chars matched
    }

    0.2 * (matched as f64 / candidate.len().max(1) as f64)
}

/// Sort recommendations by confidence descending, then alphabetically.
pub fn rank(recommendations: &mut [Recommendation]) {
    recommendations.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.text.cmp(&b.text))
    });
}
```

- [ ] **Step 5: Create lib.rs**

```rust
//! Colony Shell command recommendation engine.
//!
//! Provides contextual command suggestions from multiple sources:
//! history, PATH commands, file paths, git refs, and (optionally) AI.

pub mod engine;
pub mod input;
pub mod scoring;
pub mod source;
pub mod sources;

pub use engine::RecommendationEngine;
pub use input::RecommendInput;
pub use source::{RecommendKind, RecommendSource, Recommendation};
```

- [ ] **Step 6: Add to workspace**

Add `"crates/colony-recommend"` to the `members` array in the root `Cargo.toml`.

- [ ] **Step 7: Verify it compiles**

Run: `cargo check -p colony-recommend`
Expected: compiles with no errors (engine.rs and sources/ don't exist yet — lib.rs will need `#[cfg]` or stub modules)

Create stub files:

`crates/colony-recommend/src/engine.rs`:
```rust
//! Recommendation engine — orchestrates sources and ranks results.
// Implemented in Task 3.
```

`crates/colony-recommend/src/sources/mod.rs`:
```rust
//! Recommendation sources.
// Implemented in Task 2.
```

- [ ] **Step 8: Write tests for input parsing and scoring**

Create `crates/colony-recommend/src/input.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_command_position() {
        let input = RecommendInput::parse("gi", 2, PathBuf::from("/tmp"));
        assert!(input.is_command_position());
        assert_eq!(input.word, "gi");
        assert_eq!(input.command, Some("gi".to_string()));
    }

    #[test]
    fn parse_arg_position() {
        let input = RecommendInput::parse("git com", 7, PathBuf::from("/tmp"));
        assert!(!input.is_command_position());
        assert_eq!(input.word, "com");
        assert_eq!(input.command, Some("git".to_string()));
        assert_eq!(input.arg_position, 1);
    }

    #[test]
    fn parse_trailing_space() {
        let input = RecommendInput::parse("git ", 4, PathBuf::from("/tmp"));
        assert!(!input.is_command_position());
        assert_eq!(input.word, "");
        assert_eq!(input.arg_position, 1);
    }

    #[test]
    fn parse_empty() {
        let input = RecommendInput::parse("", 0, PathBuf::from("/tmp"));
        assert!(input.is_command_position());
        assert_eq!(input.word, "");
    }
}
```

Add scoring tests in `crates/colony-recommend/src/scoring.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_match_scores_high() {
        let score = prefix_score("git", "git");
        assert!(score > 0.9);
    }

    #[test]
    fn prefix_partial_scores_moderate() {
        let score = prefix_score("gi", "git");
        assert!(score > 0.8);
    }

    #[test]
    fn no_match_scores_zero() {
        let score = prefix_score("xyz", "git");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn fuzzy_substring_scores() {
        let score = fuzzy_score("commit", "git-commit");
        assert!(score > 0.4);
    }
}
```

- [ ] **Step 9: Run tests**

Run: `cargo test -p colony-recommend`
Expected: all tests pass

- [ ] **Step 10: Commit**

```bash
git add crates/colony-recommend/ Cargo.toml
git commit -m "feat(F074): colony-recommend crate skeleton with core types, input parsing, scoring"
```

---

## Chunk 2: Recommendation Sources

### Task 2: Implement HistorySource

**Files:**
- Create: `crates/colony-recommend/src/sources/history.rs`
- Modify: `crates/colony-recommend/src/sources/mod.rs`

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::RecommendInput;
    use std::path::PathBuf;

    #[test]
    fn matches_prefix() {
        let source = HistorySource::new(vec![
            "git commit -m 'fix'".to_string(),
            "git push origin main".to_string(),
            "cargo test".to_string(),
        ]);
        let input = RecommendInput::parse("git c", 5, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, RecommendKind::History);
        assert!(results[0].text.contains("commit"));
    }

    #[test]
    fn frequency_boost() {
        let mut source = HistorySource::new(vec![
            "ls".to_string(),
            "ls -la".to_string(),
            "ls -la".to_string(),
            "ls -la".to_string(),
        ]);
        source.rebuild_frequency();
        let input = RecommendInput::parse("ls", 2, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        // "ls -la" should score higher due to frequency
        assert!(results.len() >= 2);
        assert!(results[0].confidence >= results[1].confidence);
    }

    #[test]
    fn empty_input_returns_empty() {
        let source = HistorySource::new(vec!["git status".to_string()]);
        let input = RecommendInput::parse("", 0, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(results.is_empty());
    }
}
```

- [ ] **Step 2: Implement HistorySource**

```rust
//! History-based recommendation source.

use std::collections::HashMap;

use crate::input::RecommendInput;
use crate::scoring::fuzzy_score;
use crate::source::{RecommendKind, RecommendSource, Recommendation};

/// Recommends commands from shell history, weighted by frequency and recency.
pub struct HistorySource {
    entries: Vec<String>,
    frequency: HashMap<String, u32>,
}

impl HistorySource {
    pub fn new(entries: Vec<String>) -> Self {
        let mut source = Self {
            entries,
            frequency: HashMap::new(),
        };
        source.rebuild_frequency();
        source
    }

    pub fn rebuild_frequency(&mut self) {
        self.frequency.clear();
        for entry in &self.entries {
            *self.frequency.entry(entry.clone()).or_insert(0) += 1;
        }
    }

    pub fn push(&mut self, entry: String) {
        *self.frequency.entry(entry.clone()).or_insert(0) += 1;
        self.entries.push(entry);
    }
}

impl RecommendSource for HistorySource {
    fn recommend(&self, input: &RecommendInput) -> Vec<Recommendation> {
        if input.line.is_empty() {
            return Vec::new();
        }

        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();

        // Iterate in reverse for recency bias
        for (recency_idx, entry) in self.entries.iter().rev().enumerate() {
            if !seen.insert(entry.as_str()) {
                continue; // Skip duplicates
            }

            let score = fuzzy_score(&input.line, entry);
            if score < 0.1 {
                continue;
            }

            let freq = self.frequency.get(entry).copied().unwrap_or(1) as f64;
            let recency_boost = 0.1 / (recency_idx as f64 + 1.0);
            let freq_boost = (freq.ln() + 1.0) * 0.05;
            let confidence = (score + recency_boost + freq_boost).min(1.0);

            // Ghost text: show the part after current input
            let ghost = if entry.starts_with(&input.line) {
                entry[input.line.len()..].to_string()
            } else {
                entry.clone()
            };

            results.push(Recommendation {
                text: ghost,
                label: entry.clone(),
                description: None,
                kind: RecommendKind::History,
                confidence,
                match_indices: Vec::new(),
            });

            if results.len() >= 20 {
                break;
            }
        }

        results
    }

    fn name(&self) -> &'static str {
        "history"
    }
}
```

- [ ] **Step 3: Update sources/mod.rs**

```rust
pub mod history;

pub use history::HistorySource;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p colony-recommend`

- [ ] **Step 5: Commit**

```bash
git add crates/colony-recommend/src/sources/
git commit -m "feat(F074): HistorySource with frequency-weighted matching"
```

### Task 2b: Implement CommandSource and PathSource

**Files:**
- Create: `crates/colony-recommend/src/sources/command.rs`
- Create: `crates/colony-recommend/src/sources/path.rs`
- Modify: `crates/colony-recommend/src/sources/mod.rs`

- [ ] **Step 1: Implement CommandSource**

```rust
//! Command name recommendation source (PATH executables + builtins).

use crate::input::RecommendInput;
use crate::scoring::fuzzy_score;
use crate::source::{RecommendKind, RecommendSource, Recommendation};

/// Recommends command names from PATH and builtins.
pub struct CommandSource {
    commands: Vec<(String, RecommendKind)>,
}

impl CommandSource {
    pub fn new(builtins: Vec<String>, path_commands: Vec<String>) -> Self {
        let mut commands: Vec<(String, RecommendKind)> = builtins
            .into_iter()
            .map(|c| (c, RecommendKind::Builtin))
            .collect();
        commands.extend(path_commands.into_iter().map(|c| (c, RecommendKind::Command)));
        Self { commands }
    }
}

impl RecommendSource for CommandSource {
    fn recommend(&self, input: &RecommendInput) -> Vec<Recommendation> {
        if !input.is_command_position() || input.word.is_empty() {
            return Vec::new();
        }

        self.commands
            .iter()
            .filter_map(|(cmd, kind)| {
                let score = fuzzy_score(&input.word, cmd);
                if score < 0.1 {
                    return None;
                }
                Some(Recommendation {
                    text: cmd[input.word.len().min(cmd.len())..].to_string(),
                    label: cmd.clone(),
                    description: None,
                    kind: *kind,
                    confidence: score,
                    match_indices: Vec::new(),
                })
            })
            .take(20)
            .collect()
    }

    fn name(&self) -> &'static str {
        "command"
    }
}
```

- [ ] **Step 2: Implement PathSource**

```rust
//! File and directory path recommendation source.

use std::fs;

use crate::input::RecommendInput;
use crate::source::{RecommendKind, RecommendSource, Recommendation};

/// Recommends file and directory paths.
pub struct PathSource;

impl RecommendSource for PathSource {
    fn recommend(&self, input: &RecommendInput) -> Vec<Recommendation> {
        // Only complete when word looks like a path
        let word = &input.word;
        if word.is_empty() && input.arg_position == 0 {
            return Vec::new();
        }

        let (dir, prefix) = if word.contains('/') {
            let last_slash = word.rfind('/').unwrap();
            let dir_part = &word[..=last_slash];
            let file_part = &word[last_slash + 1..];

            let resolved = if dir_part.starts_with('~') {
                dirs::home_dir()
                    .map(|h| h.join(&dir_part[2..]))
                    .unwrap_or_else(|| input.cwd.join(dir_part))
            } else if dir_part.starts_with('/') {
                std::path::PathBuf::from(dir_part)
            } else {
                input.cwd.join(dir_part)
            };

            (resolved, file_part.to_string())
        } else {
            (input.cwd.clone(), word.clone())
        };

        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };

        let mut results = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !prefix.is_empty() && !name.starts_with(&prefix) {
                continue;
            }

            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let suffix = if is_dir { "/" } else { "" };
            let completion = format!("{}{}", &name[prefix.len()..], suffix);

            let confidence = if name.starts_with(&prefix) {
                0.7 + (0.2 * prefix.len() as f64 / name.len().max(1) as f64)
            } else {
                0.3
            };

            // Sort directories first
            let dir_boost = if is_dir { 0.05 } else { 0.0 };

            results.push(Recommendation {
                text: completion,
                label: format!("{}{}", name, suffix),
                description: None,
                kind: if is_dir { RecommendKind::Directory } else { RecommendKind::File },
                confidence: confidence + dir_boost,
                match_indices: Vec::new(),
            });
        }

        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(20);
        results
    }

    fn name(&self) -> &'static str {
        "path"
    }
}
```

- [ ] **Step 3: Implement GitSource**

Create `crates/colony-recommend/src/sources/git.rs`:
```rust
//! Git-aware recommendation source.

use std::process::Command;

use crate::input::RecommendInput;
use crate::scoring::fuzzy_score;
use crate::source::{RecommendKind, RecommendSource, Recommendation};

/// Git subcommands and branch name recommendations.
pub struct GitSource;

const GIT_SUBCOMMANDS: &[(&str, &str)] = &[
    ("add", "Add file contents to the index"),
    ("branch", "List, create, or delete branches"),
    ("checkout", "Switch branches or restore files"),
    ("commit", "Record changes to the repository"),
    ("diff", "Show changes between commits"),
    ("fetch", "Download objects and refs from remote"),
    ("log", "Show commit logs"),
    ("merge", "Join development histories together"),
    ("pull", "Fetch and integrate from remote"),
    ("push", "Update remote refs"),
    ("rebase", "Reapply commits on top of another base"),
    ("reset", "Reset current HEAD to specified state"),
    ("stash", "Stash changes in a dirty working directory"),
    ("status", "Show the working tree status"),
    ("switch", "Switch branches"),
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
                    } else {
                        sub[input.word.len().min(sub.len())..].to_string()
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
        if !matches!(subcmd, "checkout" | "switch" | "merge" | "rebase" | "branch") {
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
                let score = fuzzy_score(&input.word, branch);
                if score < 0.1 && !input.word.is_empty() {
                    return None;
                }
                let text = if input.word.is_empty() {
                    branch.to_string()
                } else {
                    branch[input.word.len().min(branch.len())..].to_string()
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
```

- [ ] **Step 4: Update sources/mod.rs**

```rust
pub mod command;
pub mod git;
pub mod history;
pub mod path;

pub use command::CommandSource;
pub use git::GitSource;
pub use history::HistorySource;
pub use path::PathSource;
```

- [ ] **Step 5: Add `dirs` dependency to Cargo.toml**

```toml
[dependencies]
thiserror = "2.0"
serde = { version = "1", features = ["derive"] }
dirs = "6"
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p colony-recommend`

- [ ] **Step 7: Commit**

```bash
git add crates/colony-recommend/
git commit -m "feat(F074): CommandSource, PathSource, GitSource recommendation providers"
```

---

## Chunk 3: Recommendation Engine + TUI Integration

### Task 3: RecommendationEngine

**Files:**
- Create: `crates/colony-recommend/src/engine.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::HistorySource;
    use std::path::PathBuf;

    #[test]
    fn engine_aggregates_sources() {
        let history = HistorySource::new(vec![
            "git status".to_string(),
            "git commit -m 'test'".to_string(),
        ]);
        let mut engine = RecommendationEngine::new();
        engine.add_source(Box::new(history));

        let results = engine.recommend("git ", 4, PathBuf::from("/tmp"));
        assert!(!results.is_empty());
    }

    #[test]
    fn engine_returns_sorted_by_confidence() {
        let history = HistorySource::new(vec![
            "git status".to_string(),
            "git commit".to_string(),
        ]);
        let mut engine = RecommendationEngine::new();
        engine.add_source(Box::new(history));

        let results = engine.recommend("git ", 4, PathBuf::from("/tmp"));
        for window in results.windows(2) {
            assert!(window[0].confidence >= window[1].confidence);
        }
    }

    #[test]
    fn ghost_text_returns_top() {
        let history = HistorySource::new(vec!["cargo test".to_string()]);
        let mut engine = RecommendationEngine::new();
        engine.add_source(Box::new(history));

        let ghost = engine.ghost_text("cargo ", 6, PathBuf::from("/tmp"));
        assert!(ghost.is_some());
    }
}
```

- [ ] **Step 2: Implement engine**

```rust
//! Recommendation engine — orchestrates sources and ranks results.

use std::path::PathBuf;

use crate::input::RecommendInput;
use crate::scoring::rank;
use crate::source::{RecommendSource, Recommendation};

/// Orchestrates multiple recommendation sources, aggregates and ranks results.
pub struct RecommendationEngine {
    sources: Vec<Box<dyn RecommendSource>>,
    max_results: usize,
}

impl RecommendationEngine {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            max_results: 20,
        }
    }

    pub fn with_max_results(mut self, n: usize) -> Self {
        self.max_results = n;
        self
    }

    pub fn add_source(&mut self, source: Box<dyn RecommendSource>) {
        self.sources.push(source);
    }

    /// Generate ranked recommendations.
    pub fn recommend(&self, line: &str, cursor: usize, cwd: PathBuf) -> Vec<Recommendation> {
        let input = RecommendInput::parse(line, cursor, cwd);

        let mut all: Vec<Recommendation> = self
            .sources
            .iter()
            .flat_map(|s| s.recommend(&input))
            .collect();

        rank(&mut all);
        all.truncate(self.max_results);
        all
    }

    /// Get the top recommendation as ghost text (text to append after input).
    pub fn ghost_text(&self, line: &str, cursor: usize, cwd: PathBuf) -> Option<String> {
        self.recommend(line, cursor, cwd)
            .into_iter()
            .next()
            .map(|r| r.text)
            .filter(|t| !t.is_empty())
    }
}

impl Default for RecommendationEngine {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p colony-recommend`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-recommend/src/engine.rs
git commit -m "feat(F074): RecommendationEngine with source aggregation and ranking"
```

### Task 4: Wire into colony-tui

**Files:**
- Modify: `crates/colony-tui/Cargo.toml` — add `colony-recommend` dependency
- Modify: `crates/colony-tui/src/app.rs` — replace GhostCompletion with RecommendationEngine

- [ ] **Step 1: Add dependency**

In `crates/colony-tui/Cargo.toml`:
```toml
colony-recommend = { path = "../colony-recommend" }
```

- [ ] **Step 2: Wire engine into App**

In `crates/colony-tui/src/app.rs`, find where `GhostCompletion` is initialized and used:

1. Add a `RecommendationEngine` field alongside (or replacing) `ghost: GhostCompletion`
2. In the app init, create the engine with HistorySource, CommandSource, PathSource, GitSource
3. In the render loop where `self.ghost.update(&buffer)` is called, also call `self.recommend_engine.ghost_text(&buffer, cursor, cwd)`
4. Use the engine's ghost text if available, fall back to existing GhostCompletion

The exact integration depends on the App struct. The key change is:
```rust
// Before:
self.ghost.update(&buffer);
let ghost_text = self.ghost.ghost_text().map(|s| s.to_string());

// After:
let ghost_text = self.recommend_engine
    .ghost_text(&buffer, buffer.len(), self.cwd.clone())
    .or_else(|| {
        self.ghost.update(&buffer);
        self.ghost.ghost_text().map(|s| s.to_string())
    });
```

- [ ] **Step 3: Build and test**

Run: `cargo build -p colony-tui`
Run: `cargo test -p colony-recommend`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-tui/ crates/colony-recommend/
git commit -m "feat(F074): wire RecommendationEngine into TUI ghost text"
```

- [ ] **Step 5: Update features.json**

Set F074 status to `"in_progress"` with note about Phase 1 completion.
