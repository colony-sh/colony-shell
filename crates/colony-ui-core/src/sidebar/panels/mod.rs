//! Built-in sidebar panel components

mod context;
mod diag;
mod file_tree;
mod git_files;
mod llm;
mod mcp;
mod session;
mod tool;

pub use context::ContextPanel;
pub use diag::DiagPanel;
pub use file_tree::FileTreePanel;
pub use git_files::GitFilesPanel;
pub use llm::LlmPanel;
pub use mcp::McpPanel;
pub use session::SessionPanel;
pub use tool::ToolPanel;
