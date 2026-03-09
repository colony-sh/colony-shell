//! File system watching

use crate::node::FileNode;
use crate::state::TreeState;
use colony_reflection::ReflectionObserver;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;

use std::time::Duration;
use tokio::sync::mpsc;

/// Events from the file system watcher
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// File or directory created
    Created(PathBuf),
    /// File or directory modified
    Modified(PathBuf),
    /// File or directory removed
    Removed(PathBuf),
    /// File or directory renamed
    Renamed { from: PathBuf, to: PathBuf },
    /// Watcher error
    Error(String),
}

/// File system watcher that updates TreeState
pub struct FileSystemWatcher {
    /// Tree state to update
    tree: TreeState,
    /// The underlying watcher (kept alive)
    _watcher: RecommendedWatcher,
    /// Receiver for watch events
    rx: mpsc::Receiver<WatchEvent>,
    /// Optional reflection observer for emitting file activity observations
    observer: Option<Arc<dyn ReflectionObserver>>,
}

impl FileSystemWatcher {
    /// Create a new watcher for the tree state
    pub fn new(tree: TreeState) -> miette::Result<Self> {
        let (tx, rx) = mpsc::channel(100);
        let tree_clone = tree.clone();

        // Create the notify watcher
        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    let watch_events = Self::convert_event(event);
                    for watch_event in watch_events {
                        let _ = tx.blocking_send(watch_event);
                    }
                }
                Err(e) => {
                    let _ = tx.blocking_send(WatchEvent::Error(e.to_string()));
                }
            }
        })
        .map_err(|e| miette::miette!("Failed to create watcher: {}", e))?;

        Ok(Self {
            tree: tree_clone,
            _watcher: watcher,
            rx,
            observer: None,
        })
    }

    /// Start watching the tree root
    pub fn start(&mut self) -> miette::Result<()> {
        self._watcher
            .watch(self.tree.root_path(), RecursiveMode::Recursive)
            .map_err(|e| miette::miette!("Failed to start watching: {}", e))?;
        
        tracing::info!("Started watching {:?}", self.tree.root_path());
        Ok(())
    }

    /// Stop watching
    pub fn stop(&mut self) -> miette::Result<()> {
        self._watcher
            .unwatch(self.tree.root_path())
            .map_err(|e| miette::miette!("Failed to stop watching: {}", e))?;
        Ok(())
    }

    /// Set a reflection observer that will receive file activity observations.
    pub fn set_observer(&mut self, observer: Arc<dyn ReflectionObserver>) {
        self.observer = Some(observer);
    }

    /// Process the next watch event, emitting a reflection observation.
    pub async fn next_event(&mut self) -> Option<WatchEvent> {
        let event = self.rx.recv().await?;

        if let Some(ref observer) = self.observer {
            let (path, action) = match &event {
                WatchEvent::Created(p) => (p.to_string_lossy().into_owned(), "created"),
                WatchEvent::Modified(p) => (p.to_string_lossy().into_owned(), "modified"),
                WatchEvent::Removed(p) => (p.to_string_lossy().into_owned(), "removed"),
                WatchEvent::Renamed { from, .. } => {
                    (from.to_string_lossy().into_owned(), "renamed")
                }
                WatchEvent::Error(_) => return Some(event),
            };

            observer
                .observe_file_activity(colony_reflection::FileActivityData {
                    path,
                    action: action.to_string(),
                    language: None,
                })
                .await;
        }

        Some(event)
    }

    /// Process pending events (call in a loop or task)
    pub async fn process_events(&mut self) -> Option<Vec<WatchEvent>> {
        let mut events = Vec::new();
        
        // Collect events with a small timeout for batching
        loop {
            match tokio::time::timeout(Duration::from_millis(50), self.rx.recv()).await {
                Ok(Some(event)) => {
                    events.push(event);
                }
                _ => break,
            }
        }

        if events.is_empty() {
            return None;
        }

        // Apply events to tree
        for event in &events {
            self.apply_event(event);
        }

        Some(events)
    }

    /// Apply a single event to the tree state
    fn apply_event(&self, event: &WatchEvent) {
        match event {
            WatchEvent::Created(path) => {
                if let Err(e) = self.tree.add_node(path) {
                    tracing::warn!("Failed to add node for {:?}: {}", path, e);
                }
                // Also refresh git status
                self.tree.refresh_git_status();
            }
            WatchEvent::Modified(path) => {
                let id = FileNode::hash_path(path);
                if let Err(e) = self.tree.refresh_node(id) {
                    tracing::warn!("Failed to refresh node {:?}: {}", path, e);
                }
                // Refresh git status on file changes
                self.tree.refresh_git_status();
            }
            WatchEvent::Removed(path) => {
                let id = FileNode::hash_path(path);
                self.tree.remove_node(id);
                self.tree.refresh_git_status();
            }
            WatchEvent::Renamed { from, to } => {
                // Remove old, add new
                let from_id = FileNode::hash_path(from);
                self.tree.remove_node(from_id);
                if let Err(e) = self.tree.add_node(to) {
                    tracing::warn!("Failed to add renamed node {:?}: {}", to, e);
                }
                self.tree.refresh_git_status();
            }
            WatchEvent::Error(msg) => {
                tracing::error!("Watch error: {}", msg);
            }
        }
    }

    /// Convert notify Event to WatchEvent(s)
    fn convert_event(event: Event) -> Vec<WatchEvent> {
        let mut results = Vec::new();
        
        match event.kind {
            EventKind::Create(_) => {
                for path in event.paths {
                    results.push(WatchEvent::Created(path));
                }
            }
            EventKind::Modify(_) => {
                for path in event.paths {
                    results.push(WatchEvent::Modified(path));
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    results.push(WatchEvent::Removed(path));
                }
            }
            EventKind::Access(_) => {
                // Ignore access events
            }
            EventKind::Other => {
                // Check for rename (paths will have from and to)
                if event.paths.len() >= 2 {
                    results.push(WatchEvent::Renamed {
                        from: event.paths[0].clone(),
                        to: event.paths[1].clone(),
                    });
                }
            }
            _ => {}
        }
        
        results
    }

    /// Get receiver for external event processing
    pub fn event_receiver(&mut self) -> &mut mpsc::Receiver<WatchEvent> {
        &mut self.rx
    }
}

/// Builder for FileSystemWatcher with configuration
pub struct WatcherBuilder {
    tree: TreeState,
    debounce: Duration,
    ignore_patterns: Vec<String>,
}

impl WatcherBuilder {
    /// Create a new builder
    pub fn new(tree: TreeState) -> Self {
        Self {
            tree,
            debounce: Duration::from_millis(100),
            ignore_patterns: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
            ],
        }
    }

    /// Set debounce duration
    pub fn debounce(mut self, duration: Duration) -> Self {
        self.debounce = duration;
        self
    }

    /// Add ignore patterns
    pub fn ignore(mut self, patterns: Vec<String>) -> Self {
        self.ignore_patterns.extend(patterns);
        self
    }

    /// Build the watcher
    pub fn build(self) -> miette::Result<FileSystemWatcher> {
        FileSystemWatcher::new(self.tree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_watcher_creation() {
        let temp = TempDir::new().unwrap();
        let tree = TreeState::new(temp.path());
        tree.load().unwrap();

        let watcher = FileSystemWatcher::new(tree);
        assert!(watcher.is_ok());
    }

    #[tokio::test]
    async fn test_watcher_start_stop() {
        let temp = TempDir::new().unwrap();
        let tree = TreeState::new(temp.path());
        tree.load().unwrap();

        let mut watcher = FileSystemWatcher::new(tree).unwrap();
        assert!(watcher.start().is_ok());
        assert!(watcher.stop().is_ok());
    }

    #[test]
    fn test_convert_create_event() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/test/file.rs")],
            attrs: Default::default(),
        };

        let converted = FileSystemWatcher::convert_event(event);
        assert_eq!(converted.len(), 1);
        assert!(matches!(converted[0], WatchEvent::Created(_)));
    }
}
