//! File tree state management and watching (F028)
//!
//! Provides shell-side awareness of the project file tree:
//! - `TreeState`: In-memory file tree with git status annotations
//! - `FileSystemWatcher`: Detects file changes via `notify` crate v6
//! - `GitStatusCache`: Caches `git status --porcelain` results (500ms TTL)
//!
//! Changes are emitted as tuples to the tuple space so that agents
//! and the terminal can react to file system events.

mod git_status;
pub mod protocol;
mod tree_state;
mod watcher;

pub use git_status::GitStatusCache;
pub use protocol::TreeProtocol;
pub use tree_state::{FileNode, GitFileStatus, TreeState};
pub use watcher::FileSystemWatcher;
