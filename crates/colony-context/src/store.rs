//! Persistent context storage using MessagePack.

use std::fs;
use std::path::PathBuf;

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

    /// Load a context file by project ID. Returns empty `ContextFile` if not found.
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

        // Remove unpinned entries by priority (ascending), then by added_at (ascending = oldest first)
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

    /// Pin an entry by ID. Returns whether the entry was found.
    pub fn pin_entry(&self, project_id: &str, entry_id: Uuid) -> Result<bool, StoreError> {
        let mut file = self.load(project_id)?;
        let Some(entry) = file.entries.iter_mut().find(|e| e.id == entry_id) else {
            return Ok(false);
        };
        entry.metadata.pinned = true;
        file.updated_at = chrono::Utc::now();
        self.save(project_id, &file)?;
        Ok(true)
    }

    fn project_path(&self, project_id: &str) -> PathBuf {
        self.root.join("projects").join(format!("{project_id}.ctx"))
    }
}

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
        store.add_entry("project-abc", entry).unwrap();

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

    #[test]
    fn clear_keeps_pinned() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        let normal = ContextEntry::new(ContextKind::File, "temp".to_string());
        let pinned = ContextEntry::new(ContextKind::Knowledge, "keep".to_string()).pinned();
        store.add_entry("proj", normal).unwrap();
        store.add_entry("proj", pinned).unwrap();

        let removed = store.clear("proj", true).unwrap();
        assert_eq!(removed, 1);

        let file = store.load("proj").unwrap();
        assert_eq!(file.entries.len(), 1);
        assert_eq!(file.entries[0].content, "keep");
    }

    #[test]
    fn clear_all() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        let pinned = ContextEntry::new(ContextKind::Knowledge, "keep".to_string()).pinned();
        store.add_entry("proj", pinned).unwrap();

        let removed = store.clear("proj", false).unwrap();
        assert_eq!(removed, 1);

        let file = store.load("proj").unwrap();
        assert!(file.entries.is_empty());
    }

    #[test]
    fn pin_entry() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        let entry = ContextEntry::new(ContextKind::File, "data".to_string());
        let id = entry.id;
        store.add_entry("proj", entry).unwrap();

        assert!(store.pin_entry("proj", id).unwrap());

        let file = store.load("proj").unwrap();
        assert!(file.entries[0].metadata.pinned);
    }

    #[test]
    fn multiple_entries_persist() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        for i in 0..5 {
            let entry = ContextEntry::new(ContextKind::Knowledge, format!("entry {i}"));
            store.add_entry("proj", entry).unwrap();
        }

        let file = store.load("proj").unwrap();
        assert_eq!(file.entries.len(), 5);
        assert_eq!(file.entries[0].content, "entry 0");
        assert_eq!(file.entries[4].content, "entry 4");
    }
}
