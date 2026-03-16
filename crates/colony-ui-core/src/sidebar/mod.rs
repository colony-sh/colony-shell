//! Sidebar management for Colony Shell
//!
//! Manages left and right sidebar panels that display contextual information
//! like file trees, session info, MCP servers, and git status.

mod manager;
pub mod panels;

pub use manager::{SidebarData, SidebarManager, SidebarPanelData};
pub use panels::{
    ContextPanel, DiagPanel, FileTreePanel, GitFilesPanel, LlmPanel, McpPanel, SessionPanel,
    ToolPanel,
};
