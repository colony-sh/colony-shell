//! Reactive TUI for Colony Shell
//!
//! A ratatui-based terminal user interface featuring:
//! - Vim-style modal editing (normal/insert modes)
//! - Tree-sitter syntax highlighting
//! - Reactive prompt components via tuple space
//! - Scrollable shell output
//! - Collapsible sidebars with file tree, session info, MCP status
//!
//! This crate provides a portable TUI that works in any terminal,
//! complementing colony-terminal's GPU-rendered experience.
//!
//! ## Architecture
//!
//! colony-tui consumes components from colony-ui-core and renders them with ratatui.
//! The same component definitions are used by colony-terminal for GPU rendering,
//! ensuring consistent behavior across both rendering backends.

pub mod app;
pub mod content;
pub mod content_area;
pub mod editor;
pub mod event;
pub mod input;
pub mod keybindings;
pub mod layout;
pub mod prompt;
pub mod prompt_renderer;
pub mod sidebar;
pub mod sidebar_renderer;
pub mod theme;

pub use app::{App, AppState};
pub use content_area::ContentArea;
pub use editor::{EditorAction, EditorHistory, InputEditor};
pub use event::Event;
pub use keybindings::{Action, Mode};
pub use layout::{LayoutState, TuiLayout};
pub use prompt_renderer::{map_color, PromptRenderer as LegacyPromptRenderer};
pub use sidebar::SidebarRenderer;
pub use sidebar_renderer::SidebarRenderer as LegacySidebarRenderer;
pub use theme::Theme;

/// Run the TUI application
pub async fn run() -> miette::Result<()> {
    let mut app = App::new()?;
    app.run().await
}
