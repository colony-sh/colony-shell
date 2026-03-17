//! Built-in sidebar panel components

mod context;
mod file_tree;
mod git_files;
mod mcp;
mod session;

pub use context::ContextPanel;
pub use file_tree::FileTreePanel;
pub use git_files::GitFilesPanel;
pub use mcp::McpPanel;
pub use session::SessionPanel;
