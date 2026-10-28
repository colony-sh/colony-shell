# F075: Context Management System — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a `colony-context` crate that provides persistent context storage, a context composition builder, and a `context` builtin command — wiring the existing fd 3 channel, agent ContextManager, and memory substrate into a unified system.

**Architecture:** New `colony-context` crate owns context persistence (MessagePack files in `~/.colony/context/`) and the composition builder that assembles `@context` for agents from project info, added files, conversations, and knowledge entries. The `context` builtin exposes this to users. Integration hooks wire context entries into `colony-agent`'s ContextManager before each LLM call.

**Tech Stack:** Rust, rmp-serde (MessagePack), existing colony-channels (fd 3), existing colony-agent (ContextManager), existing colony-memory (LanceDB)

**Important:** Follow the Rust style guide in `RUST_STYLE_GUIDE.md`. The workspace is at `/Users/mcfearsome/src/github.com/colony-sh/colony-shell/`. Cargo.toml workspace members must be updated.

---

## File Structure

```
crates/colony-context/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API
│   ├── store.rs            # ContextStore: load/save .ctx files (MessagePack)
│   ├── entry.rs            # ContextEntry, ContextKind, EntryMetadata types
│   ├── composer.rs         # ContextComposer: build @context from multiple sources
│   ├── project.rs          # ProjectContext: auto-gather from cwd (git, languages, deps)
│   └── builtin.rs          # `context` builtin implementation (show, add, remove, clear, pin, prune)
```

**Existing files to modify:**
- `Cargo.toml` (workspace root) — add `colony-context` member
- `crates/colony-exec/Cargo.toml` — add `colony-context` dependency
- `crates/colony-exec/src/builtins/mod.rs` — register `context` builtin
- `crates/colony-agent/src/agent.rs` — inject composed context before LLM calls

---

## Chunk 1: Core Types + Persistent Store

### Task 1: Create colony-context crate with entry types

**Files:**
- Create: `crates/colony-context/Cargo.toml`
- Create: `crates/colony-context/src/lib.rs`
- Create: `crates/colony-context/src/entry.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "colony-context"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
description = "Context management system for Colony Shell"

[dependencies]
thiserror = "2.0"
serde = { version = "1", features = ["derive"] }
rmp-serde = "1"
chrono = { version = "0.4", features = ["serde"] }
dirs = "6"
uuid = { version = "1", features = ["v4", "serde"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create entry.rs**

```rust
//! Context entry types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Kind of context entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContextKind {
    /// Auto-gathered project info (languages, build system, git state).
    Project,
    /// Explicitly added file content.
    File,
    /// Agent conversation history.
    Conversation,
    /// User-curated knowledge.
    Knowledge,
    /// From fd 3 (stdcontext) channel.
    Channel,
}

/// Priority level for context entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// A single context entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub id: Uuid,
    pub kind: ContextKind,
    pub content: String,
    pub label: Option<String>,
    pub metadata: EntryMetadata,
}

/// Metadata attached to a context entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryMetadata {
    pub tokens: usize,
    pub added_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub use_count: u32,
    pub pinned: bool,
    pub priority: Priority,
}

impl ContextEntry {
    /// Create a new entry with default metadata.
    pub fn new(kind: ContextKind, content: String) -> Self {
        let tokens = content.len() / 4; // Rough estimate: 4 chars/token
        Self {
            id: Uuid::new_v4(),
            kind,
            content,
            label: None,
            metadata: EntryMetadata {
                tokens,
                added_at: Utc::now(),
                last_used: None,
                use_count: 0,
                pinned: false,
                priority: Priority::default(),
            },
        }
    }

    /// Create with a label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Pin this entry (won't be auto-pruned).
    pub fn pinned(mut self) -> Self {
        self.metadata.pinned = true;
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.metadata.priority = priority;
        self
    }

    /// Mark as used (updates last_used and use_count).
    pub fn mark_used(&mut self) {
        self.metadata.last_used = Some(Utc::now());
        self.metadata.use_count += 1;
    }
}

/// A context file containing versioned entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub entries: Vec<ContextEntry>,
}

impl ContextFile {
    pub fn new() -> Self {
        Self {
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            entries: Vec::new(),
        }
    }

    pub fn total_tokens(&self) -> usize {
        self.entries.iter().map(|e| e.metadata.tokens).sum()
    }
}

impl Default for ContextFile {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 3: Create lib.rs**

```rust
//! Colony Shell context management system.
//!
//! Persistent context storage, composition, and builtin commands
//! for managing AI interaction context.

pub mod builtin;
pub mod composer;
pub mod entry;
pub mod project;
pub mod store;

pub use composer::ContextComposer;
pub use entry::{ContextEntry, ContextFile, ContextKind, EntryMetadata, Priority};
pub use store::ContextStore;
```

- [ ] **Step 4: Add to workspace**

Add `"crates/colony-context"` to the `members` array in root `Cargo.toml`.

- [ ] **Step 5: Create stub files**

`crates/colony-context/src/store.rs`:
```rust
//! Context store — persistent storage.
// Implemented in Task 2.
```

`crates/colony-context/src/composer.rs`:
```rust
//! Context composer — builds @context for agents.
// Implemented in Task 3.
```

`crates/colony-context/src/project.rs`:
```rust
//! Project context auto-gathering.
// Implemented in Task 3.
```

`crates/colony-context/src/builtin.rs`:
```rust
//! Context builtin command.
// Implemented in Task 4.
```

- [ ] **Step 6: Write entry tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_entry_estimates_tokens() {
        let entry = ContextEntry::new(ContextKind::Knowledge, "hello world".to_string());
        assert_eq!(entry.metadata.tokens, 2); // 11 chars / 4
    }

    #[test]
    fn mark_used_increments() {
        let mut entry = ContextEntry::new(ContextKind::File, "test".to_string());
        assert_eq!(entry.metadata.use_count, 0);
        entry.mark_used();
        assert_eq!(entry.metadata.use_count, 1);
        assert!(entry.metadata.last_used.is_some());
    }

    #[test]
    fn context_file_total_tokens() {
        let mut file = ContextFile::new();
        file.entries.push(ContextEntry::new(ContextKind::Knowledge, "a".repeat(100)));
        file.entries.push(ContextEntry::new(ContextKind::File, "b".repeat(200)));
        assert_eq!(file.total_tokens(), 25 + 50);
    }
}
```

- [ ] **Step 7: Verify compile**

Run: `cargo check -p colony-context`

- [ ] **Step 8: Commit**

```bash
git add crates/colony-context/ Cargo.toml
git commit -m "feat(F075): colony-context crate with entry types and ContextFile"
```

### Task 2: ContextStore (persistence)

**Files:**
- Modify: `crates/colony-context/src/store.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{ContextEntry, ContextKind};
    use tempfile::TempDir;

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        let entry = ContextEntry::new(ContextKind::Knowledge, "Rust is great".to_string());
        store.add_entry("project-abc", entry.clone()).unwrap();

        let file = store.load("project-abc").unwrap();
        assert_eq!(file.entries.len(), 1);
        assert_eq!(file.entries[0].content, "Rust is great");
    }

    #[test]
    fn remove_entry() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        let entry = ContextEntry::new(ContextKind::Knowledge, "test".to_string());
        let id = entry.id;
        store.add_entry("proj", entry).unwrap();
        store.remove_entry("proj", id).unwrap();

        let file = store.load("proj").unwrap();
        assert!(file.entries.is_empty());
    }

    #[test]
    fn load_missing_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());
        let file = store.load("nonexistent").unwrap();
        assert!(file.entries.is_empty());
    }

    #[test]
    fn prune_removes_unpinned_by_age() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        let old = ContextEntry::new(ContextKind::Channel, "old".to_string());
        let pinned = ContextEntry::new(ContextKind::Knowledge, "keep".to_string()).pinned();
        store.add_entry("proj", old).unwrap();
        store.add_entry("proj", pinned).unwrap();

        let removed = store.prune("proj", 0).unwrap(); // max_tokens=0 forces prune of all unpinned
        assert_eq!(removed, 1);

        let file = store.load("proj").unwrap();
        assert_eq!(file.entries.len(), 1);
        assert!(file.entries[0].metadata.pinned);
    }
}
```

- [ ] **Step 2: Implement ContextStore**

```rust
//! Persistent context storage using MessagePack.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;
use uuid::Uuid;

use crate::entry::{ContextEntry, ContextFile};

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("deserialization error: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
}

/// Persistent context store backed by MessagePack files.
///
/// Storage layout:
/// ```text
/// {root}/
/// ├── projects/{project_id}.ctx
/// ├── global/knowledge.ctx
/// └── snapshots/{name}.ctx
/// ```
pub struct ContextStore {
    root: PathBuf,
}

impl ContextStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Default store at `~/.colony/context/`.
    pub fn default_store() -> Self {
        let root = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".colony")
            .join("context");
        Self::new(root)
    }

    /// Load a context file by project ID. Returns empty ContextFile if not found.
    pub fn load(&self, project_id: &str) -> Result<ContextFile, StoreError> {
        let path = self.project_path(project_id);
        if !path.exists() {
            return Ok(ContextFile::new());
        }
        let data = fs::read(&path)?;
        let file: ContextFile = rmp_serde::from_slice(&data)?;
        Ok(file)
    }

    /// Save a context file for a project.
    pub fn save(&self, project_id: &str, file: &ContextFile) -> Result<(), StoreError> {
        let path = self.project_path(project_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = rmp_serde::to_vec(file)?;
        fs::write(&path, data)?;
        Ok(())
    }

    /// Add an entry to a project's context.
    pub fn add_entry(&self, project_id: &str, entry: ContextEntry) -> Result<(), StoreError> {
        let mut file = self.load(project_id)?;
        file.entries.push(entry);
        file.updated_at = chrono::Utc::now();
        self.save(project_id, &file)
    }

    /// Remove an entry by ID.
    pub fn remove_entry(&self, project_id: &str, entry_id: Uuid) -> Result<bool, StoreError> {
        let mut file = self.load(project_id)?;
        let before = file.entries.len();
        file.entries.retain(|e| e.id != entry_id);
        let removed = before != file.entries.len();
        if removed {
            file.updated_at = chrono::Utc::now();
            self.save(project_id, &file)?;
        }
        Ok(removed)
    }

    /// Clear all non-pinned entries (or all if `keep_pinned` is false).
    pub fn clear(&self, project_id: &str, keep_pinned: bool) -> Result<usize, StoreError> {
        let mut file = self.load(project_id)?;
        let before = file.entries.len();
        if keep_pinned {
            file.entries.retain(|e| e.metadata.pinned);
        } else {
            file.entries.clear();
        }
        let removed = before - file.entries.len();
        file.updated_at = chrono::Utc::now();
        self.save(project_id, &file)?;
        Ok(removed)
    }

    /// Prune entries to fit within `max_tokens`. Removes unpinned lowest-priority,
    /// oldest entries first. Returns count removed.
    pub fn prune(&self, project_id: &str, max_tokens: usize) -> Result<usize, StoreError> {
        let mut file = self.load(project_id)?;
        let mut removed = 0;

        // Sort unpinned entries by priority (ascending), then by added_at (ascending = oldest first)
        while file.total_tokens() > max_tokens {
            let Some(idx) = file
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.metadata.pinned)
                .min_by_key(|(_, e)| (e.metadata.priority, e.metadata.added_at))
                .map(|(i, _)| i)
            else {
                break; // Only pinned entries remain
            };
            file.entries.remove(idx);
            removed += 1;
        }

        if removed > 0 {
            file.updated_at = chrono::Utc::now();
            self.save(project_id, &file)?;
        }
        Ok(removed)
    }

    fn project_path(&self, project_id: &str) -> PathBuf {
        self.root.join("projects").join(format!("{project_id}.ctx"))
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p colony-context`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-context/src/store.rs
git commit -m "feat(F075): ContextStore with MessagePack persistence, prune, clear"
```

---

## Chunk 2: Project Context + Composer

### Task 3: ProjectContext auto-gathering and ContextComposer

**Files:**
- Modify: `crates/colony-context/src/project.rs`
- Modify: `crates/colony-context/src/composer.rs`

- [ ] **Step 1: Implement ProjectContext**

```rust
//! Auto-gather project context from current working directory.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Detected project information.
#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub root: String,
    pub git_branch: Option<String>,
    pub git_status_summary: Option<String>,
    pub languages: Vec<String>,
    pub build_file: Option<String>,
}

impl ProjectContext {
    /// Gather project context from the given directory.
    pub fn gather(cwd: &Path) -> Self {
        Self {
            root: cwd.to_string_lossy().to_string(),
            git_branch: Self::git_branch(cwd),
            git_status_summary: Self::git_status(cwd),
            languages: Self::detect_languages(cwd),
            build_file: Self::detect_build_file(cwd),
        }
    }

    /// Render as text for inclusion in context.
    pub fn render(&self) -> String {
        let mut lines = vec![format!("Project: {}", self.root)];

        if let Some(ref branch) = self.git_branch {
            lines.push(format!("Git branch: {branch}"));
        }
        if let Some(ref status) = self.git_status_summary {
            lines.push(format!("Git status: {status}"));
        }
        if !self.languages.is_empty() {
            lines.push(format!("Languages: {}", self.languages.join(", ")));
        }
        if let Some(ref build) = self.build_file {
            lines.push(format!("Build: {build}"));
        }

        lines.join("\n")
    }

    fn git_branch(cwd: &Path) -> Option<String> {
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(cwd)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn git_status(cwd: &Path) -> Option<String> {
        let output = Command::new("git")
            .args(["status", "--short"])
            .current_dir(cwd)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let lines: Vec<&str> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .take(10)
            .collect();
        let count = lines.len();
        if count == 0 {
            return Some("clean".to_string());
        }
        Some(format!("{count} changed files"))
    }

    fn detect_languages(cwd: &Path) -> Vec<String> {
        let indicators: &[(&str, &str)] = &[
            ("Cargo.toml", "Rust"),
            ("package.json", "JavaScript/TypeScript"),
            ("go.mod", "Go"),
            ("pyproject.toml", "Python"),
            ("requirements.txt", "Python"),
            ("Gemfile", "Ruby"),
            ("mix.exs", "Elixir"),
            ("build.gradle", "Java/Kotlin"),
            ("pom.xml", "Java"),
            ("Package.swift", "Swift"),
        ];

        indicators
            .iter()
            .filter(|(file, _)| cwd.join(file).exists())
            .map(|(_, lang)| lang.to_string())
            .collect()
    }

    fn detect_build_file(cwd: &Path) -> Option<String> {
        let files = [
            "Cargo.toml",
            "package.json",
            "Makefile",
            "CMakeLists.txt",
            "build.gradle",
            "pom.xml",
            "mix.exs",
        ];
        files.iter().find(|f| cwd.join(f).exists()).map(|f| f.to_string())
    }
}
```

- [ ] **Step 2: Implement ContextComposer**

```rust
//! Composes a unified context from multiple sources within a token budget.

use crate::entry::{ContextEntry, ContextKind, Priority};
use crate::project::ProjectContext;
use crate::store::ContextStore;

/// Builds a unified `@context` string for agent consumption.
pub struct ContextComposer {
    max_tokens: usize,
}

impl ContextComposer {
    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }

    /// Compose context for a project, respecting the token budget.
    ///
    /// Priority order:
    /// 1. Pinned entries (always included)
    /// 2. Project context (auto-gathered)
    /// 3. High-priority entries
    /// 4. Channel entries (fd 3)
    /// 5. Normal-priority entries
    /// 6. Low-priority entries (space permitting)
    pub fn compose(
        &self,
        store: &ContextStore,
        project_id: &str,
        project_ctx: &ProjectContext,
        channel_entries: &[String],
    ) -> ComposedContext {
        let file = store.load(project_id).unwrap_or_default();
        let mut sections: Vec<ContextSection> = Vec::new();
        let mut used_tokens = 0;

        // 1. Project context (auto-gathered)
        let project_text = project_ctx.render();
        let project_tokens = project_text.len() / 4;
        if used_tokens + project_tokens <= self.max_tokens {
            sections.push(ContextSection {
                label: "Project".to_string(),
                content: project_text,
                tokens: project_tokens,
                kind: ContextKind::Project,
            });
            used_tokens += project_tokens;
        }

        // 2. Pinned entries (always included)
        for entry in file.entries.iter().filter(|e| e.metadata.pinned) {
            if used_tokens + entry.metadata.tokens > self.max_tokens {
                break;
            }
            sections.push(ContextSection::from_entry(entry));
            used_tokens += entry.metadata.tokens;
        }

        // 3. Channel entries (fd 3 — most recent first)
        for line in channel_entries.iter().rev().take(20) {
            let tokens = line.len() / 4;
            if used_tokens + tokens > self.max_tokens {
                break;
            }
            sections.push(ContextSection {
                label: "Channel".to_string(),
                content: line.clone(),
                tokens,
                kind: ContextKind::Channel,
            });
            used_tokens += tokens;
        }

        // 4. Remaining entries by priority (descending)
        let mut remaining: Vec<&ContextEntry> = file
            .entries
            .iter()
            .filter(|e| !e.metadata.pinned)
            .collect();
        remaining.sort_by(|a, b| {
            b.metadata
                .priority
                .cmp(&a.metadata.priority)
                .then_with(|| b.metadata.use_count.cmp(&a.metadata.use_count))
        });

        for entry in remaining {
            if used_tokens + entry.metadata.tokens > self.max_tokens {
                continue; // Skip large entries, try smaller ones
            }
            sections.push(ContextSection::from_entry(entry));
            used_tokens += entry.metadata.tokens;
        }

        ComposedContext {
            sections,
            total_tokens: used_tokens,
            budget: self.max_tokens,
        }
    }
}

/// A section in the composed context.
#[derive(Debug, Clone)]
pub struct ContextSection {
    pub label: String,
    pub content: String,
    pub tokens: usize,
    pub kind: ContextKind,
}

impl ContextSection {
    fn from_entry(entry: &ContextEntry) -> Self {
        Self {
            label: entry.label.clone().unwrap_or_else(|| format!("{:?}", entry.kind)),
            content: entry.content.clone(),
            tokens: entry.metadata.tokens,
            kind: entry.kind,
        }
    }
}

/// Result of context composition.
#[derive(Debug, Clone)]
pub struct ComposedContext {
    pub sections: Vec<ContextSection>,
    pub total_tokens: usize,
    pub budget: usize,
}

impl ComposedContext {
    /// Render as a single string for injection into agent context.
    pub fn render(&self) -> String {
        self.sections
            .iter()
            .map(|s| format!("## {}\n{}", s.label, s.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Token utilization as a fraction (0.0–1.0).
    pub fn utilization(&self) -> f64 {
        if self.budget == 0 {
            return 0.0;
        }
        self.total_tokens as f64 / self.budget as f64
    }
}

impl Default for ComposedContext {
    fn default() -> Self {
        Self {
            sections: Vec::new(),
            total_tokens: 0,
            budget: 0,
        }
    }
}
```

- [ ] **Step 3: Write tests**

Add to `composer.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::ContextEntry;
    use tempfile::TempDir;

    #[test]
    fn compose_includes_project() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());
        let project = ProjectContext {
            root: "/test".to_string(),
            git_branch: Some("main".to_string()),
            git_status_summary: None,
            languages: vec!["Rust".to_string()],
            build_file: Some("Cargo.toml".to_string()),
        };

        let composer = ContextComposer::new(10000);
        let result = composer.compose(&store, "test", &project, &[]);
        assert!(!result.sections.is_empty());
        assert!(result.render().contains("Rust"));
    }

    #[test]
    fn compose_respects_budget() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        // Add a large entry
        let big = ContextEntry::new(ContextKind::File, "x".repeat(40000));
        store.add_entry("proj", big).unwrap();

        let project = ProjectContext {
            root: "/test".to_string(),
            git_branch: None,
            git_status_summary: None,
            languages: Vec::new(),
            build_file: None,
        };

        let composer = ContextComposer::new(100); // Very small budget
        let result = composer.compose(&store, "proj", &project, &[]);
        assert!(result.total_tokens <= 100);
    }

    #[test]
    fn compose_includes_pinned_first() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        let pinned = ContextEntry::new(ContextKind::Knowledge, "important".to_string()).pinned();
        let normal = ContextEntry::new(ContextKind::Knowledge, "meh".to_string());
        store.add_entry("proj", normal).unwrap();
        store.add_entry("proj", pinned).unwrap();

        let project = ProjectContext {
            root: "/test".to_string(),
            git_branch: None,
            git_status_summary: None,
            languages: Vec::new(),
            build_file: None,
        };

        let composer = ContextComposer::new(10000);
        let result = composer.compose(&store, "proj", &project, &[]);
        let rendered = result.render();
        let pinned_pos = rendered.find("important").unwrap();
        let normal_pos = rendered.find("meh").unwrap();
        assert!(pinned_pos < normal_pos);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p colony-context`

- [ ] **Step 5: Commit**

```bash
git add crates/colony-context/src/project.rs crates/colony-context/src/composer.rs
git commit -m "feat(F075): ProjectContext auto-gathering + ContextComposer with token budgets"
```

---

## Chunk 3: Context Builtin + Agent Integration

### Task 4: `context` builtin command

**Files:**
- Modify: `crates/colony-context/src/builtin.rs`

- [ ] **Step 1: Implement builtin**

```rust
//! `context` builtin command.
//!
//! Usage:
//!   context show [--tokens]     Show current context summary
//!   context add <text>          Add knowledge entry
//!   context add --file <path>   Add file content
//!   context remove <id>         Remove entry by ID (prefix match)
//!   context clear [--all]       Clear non-pinned entries (--all for everything)
//!   context pin <id>            Pin an entry
//!   context prune <max_tokens>  Prune to fit token budget

use uuid::Uuid;

use crate::entry::{ContextEntry, ContextKind};
use crate::store::{ContextStore, StoreError};

/// Result of running a context subcommand.
pub enum BuiltinResult {
    /// Text to print to stdout.
    Output(String),
    /// Error message.
    Error(String),
}

/// Execute a `context` builtin invocation.
pub fn execute(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    let Some(subcmd) = args.first() else {
        return execute_show(store, project_id, false);
    };

    match *subcmd {
        "show" => {
            let tokens_flag = args.iter().any(|a| *a == "--tokens");
            execute_show(store, project_id, tokens_flag)
        }
        "add" => execute_add(&args[1..], store, project_id),
        "remove" => execute_remove(&args[1..], store, project_id),
        "clear" => {
            let all = args.iter().any(|a| *a == "--all");
            execute_clear(store, project_id, all)
        }
        "pin" => execute_pin(&args[1..], store, project_id),
        "prune" => execute_prune(&args[1..], store, project_id),
        _ => BuiltinResult::Error(format!("unknown subcommand: {subcmd}")),
    }
}

fn execute_show(store: &ContextStore, project_id: &str, show_tokens: bool) -> BuiltinResult {
    let file = match store.load(project_id) {
        Ok(f) => f,
        Err(e) => return BuiltinResult::Error(format!("failed to load context: {e}")),
    };

    if file.entries.is_empty() {
        return BuiltinResult::Output("Context is empty.".to_string());
    }

    let mut lines = Vec::new();
    let total = file.total_tokens();
    lines.push(format!("Context: {} entries, ~{} tokens", file.entries.len(), total));
    lines.push(String::new());

    for entry in &file.entries {
        let pin = if entry.metadata.pinned { " [pinned]" } else { "" };
        let id_short = &entry.id.to_string()[..8];
        let label = entry.label.as_deref().unwrap_or("—");
        let kind = format!("{:?}", entry.kind);

        let mut line = format!("  {id_short}  {kind:<12}  {label}");
        if show_tokens {
            line.push_str(&format!("  (~{} tokens)", entry.metadata.tokens));
        }
        line.push_str(pin);
        lines.push(line);
    }

    BuiltinResult::Output(lines.join("\n"))
}

fn execute_add(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::Error("usage: context add <text> | context add --file <path>".to_string());
    }

    if args.first() == Some(&"--file") {
        let Some(path) = args.get(1) else {
            return BuiltinResult::Error("usage: context add --file <path>".to_string());
        };
        let Ok(content) = std::fs::read_to_string(path) else {
            return BuiltinResult::Error(format!("failed to read file: {path}"));
        };
        let entry = ContextEntry::new(ContextKind::File, content).with_label(*path);
        if let Err(e) = store.add_entry(project_id, entry) {
            return BuiltinResult::Error(format!("failed to add: {e}"));
        }
        return BuiltinResult::Output(format!("Added file: {path}"));
    }

    let text = args.join(" ");
    let entry = ContextEntry::new(ContextKind::Knowledge, text);
    if let Err(e) = store.add_entry(project_id, entry) {
        return BuiltinResult::Error(format!("failed to add: {e}"));
    }
    BuiltinResult::Output("Added to context.".to_string())
}

fn execute_remove(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    let Some(id_prefix) = args.first() else {
        return BuiltinResult::Error("usage: context remove <id-prefix>".to_string());
    };

    let file = match store.load(project_id) {
        Ok(f) => f,
        Err(e) => return BuiltinResult::Error(format!("failed to load: {e}")),
    };

    let matching: Vec<&ContextEntry> = file
        .entries
        .iter()
        .filter(|e| e.id.to_string().starts_with(id_prefix))
        .collect();

    if matching.is_empty() {
        return BuiltinResult::Error(format!("no entry matching: {id_prefix}"));
    }
    if matching.len() > 1 {
        return BuiltinResult::Error(format!("{} entries match — be more specific", matching.len()));
    }

    let id = matching[0].id;
    match store.remove_entry(project_id, id) {
        Ok(true) => BuiltinResult::Output("Removed.".to_string()),
        Ok(false) => BuiltinResult::Error("entry not found".to_string()),
        Err(e) => BuiltinResult::Error(format!("failed to remove: {e}")),
    }
}

fn execute_clear(store: &ContextStore, project_id: &str, all: bool) -> BuiltinResult {
    match store.clear(project_id, !all) {
        Ok(n) => BuiltinResult::Output(format!("Cleared {n} entries.")),
        Err(e) => BuiltinResult::Error(format!("failed to clear: {e}")),
    }
}

fn execute_pin(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    let Some(id_prefix) = args.first() else {
        return BuiltinResult::Error("usage: context pin <id-prefix>".to_string());
    };

    let mut file = match store.load(project_id) {
        Ok(f) => f,
        Err(e) => return BuiltinResult::Error(format!("failed to load: {e}")),
    };

    let Some(entry) = file
        .entries
        .iter_mut()
        .find(|e| e.id.to_string().starts_with(id_prefix))
    else {
        return BuiltinResult::Error(format!("no entry matching: {id_prefix}"));
    };

    entry.metadata.pinned = true;
    match store.save(project_id, &file) {
        Ok(()) => BuiltinResult::Output("Pinned.".to_string()),
        Err(e) => BuiltinResult::Error(format!("failed to save: {e}")),
    }
}

fn execute_prune(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    let max_tokens: usize = args
        .first()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

    match store.prune(project_id, max_tokens) {
        Ok(n) => BuiltinResult::Output(format!("Pruned {n} entries.")),
        Err(e) => BuiltinResult::Error(format!("failed to prune: {e}")),
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p colony-context`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-context/src/builtin.rs
git commit -m "feat(F075): context builtin — show, add, remove, clear, pin, prune"
```

### Task 5: Wire into colony-exec builtins

**Files:**
- Modify: `crates/colony-exec/Cargo.toml` — add `colony-context` dependency
- Modify: `crates/colony-exec/src/builtins/mod.rs` — register `context` builtin

- [ ] **Step 1: Add dependency**

In `crates/colony-exec/Cargo.toml`:
```toml
colony-context = { path = "../colony-context" }
```

- [ ] **Step 2: Register builtin**

In the builtins registry (wherever builtins like `cd`, `export`, etc. are registered), add a `"context"` entry that:
1. Creates a `ContextStore::default_store()`
2. Derives `project_id` from current working directory hash (e.g., `sha256(cwd)[..16]`)
3. Calls `colony_context::builtin::execute(args, &store, &project_id)`
4. Prints output or error to stdout/stderr

The exact integration depends on the builtin registration pattern in colony-exec. Follow the existing pattern for builtins like `cd`, `export`.

- [ ] **Step 3: Build**

Run: `cargo build -p colony-exec`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-exec/
git commit -m "feat(F075): register context builtin in colony-exec"
```

- [ ] **Step 5: Update features.json**

Set F075 status to `"in_progress"` with note about Phase 1 completion.
