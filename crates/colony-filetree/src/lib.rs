//! File tree state management and watching for Colony Shell
//!
//! Provides:
//! - `FileNode` - Individual file/directory in the tree
//! - `TreeState` - Complete tree state with operations
//! - `FileSystemWatcher` - Watch for file changes using notify
//! - `GitStatusCache` - Git status for files (modified, staged, etc.)
//!
//! # Example
//!
//! ```no_run
//! use colony_filetree::{TreeState, FileSystemWatcher};
//! use std::path::Path;
//!
//! # fn example() -> miette::Result<()> {
//! // Create tree state for a directory
//! let tree = TreeState::new(Path::new("/path/to/project"));
//! tree.load()?;
//!
//! // Start watching for changes
//! let mut watcher = FileSystemWatcher::new(tree.clone())?;
//! watcher.start()?;
//! # Ok(())
//! # }
//! ```

mod git;
mod node;
mod state;
mod watcher;

pub use git::{GitFileStatus, GitStatusCache};
pub use node::{FileNode, FileType, NodeId};
pub use state::{TreeState, TreeEvent};
pub use watcher::{FileSystemWatcher, WatchEvent};
