//! Ratatui-based TUI renderer for Colony Shell
//!
//! Provides the terminal user interface using ratatui, consuming
//! component definitions from `colony-ui-core` and rendering them
//! as interactive terminal widgets.

pub mod app;
pub mod content_area;
pub mod editor;
pub mod layout;
pub mod prompt_renderer;
pub mod sidebar_renderer;

pub use app::TuiApp;
pub use content_area::ContentArea;
pub use editor::{EditorAction, EditorHistory, InputEditor};
pub use layout::{LayoutState, TuiLayout};
pub use prompt_renderer::{map_color, PromptRenderer};
pub use sidebar_renderer::SidebarRenderer;
