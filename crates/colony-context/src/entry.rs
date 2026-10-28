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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum Priority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Critical = 3,
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
        file.entries
            .push(ContextEntry::new(ContextKind::Knowledge, "a".repeat(100)));
        file.entries
            .push(ContextEntry::new(ContextKind::File, "b".repeat(200)));
        assert_eq!(file.total_tokens(), 25 + 50);
    }

    #[test]
    fn builder_chain() {
        let entry = ContextEntry::new(ContextKind::Knowledge, "important".to_string())
            .with_label("my note")
            .with_priority(Priority::High)
            .pinned();
        assert_eq!(entry.label.as_deref(), Some("my note"));
        assert_eq!(entry.metadata.priority, Priority::High);
        assert!(entry.metadata.pinned);
    }
}
