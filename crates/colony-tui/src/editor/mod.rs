//! Vim-mode input editor for the colony-shell reactive TUI.
//!
//! This module implements a modal text editor component that replaces rustyline's
//! line editing. It provides:
//!
//! - **Modal editing** (Normal, Insert, Command, Search modes)
//! - **Vim keybindings** (motions, operators, text objects)
//! - **Tab completion** with fuzzy matching and multiple sources
//! - **History navigation** (Up/Down, search)
//! - **Syntax highlighting** via tree-sitter
//! - **Ghost text** inline suggestions from history
//!
//! All editor logic lives in pure data structures. No terminal I/O happens in
//! this module -- rendering is the responsibility of the TUI layer that reads
//! the editor's state.
//!
//! # Usage
//!
//! ```rust,no_run
//! use colony_tui::editor::{InputEditor, EditorAction, EditorMode};
//! use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
//!
//! let mut editor = InputEditor::new();
//!
//! // Simulate typing "ls"
//! let key = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
//! editor.handle_event(key);
//! let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE);
//! editor.handle_event(key);
//!
//! // Submit with Enter
//! let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
//! let action = editor.handle_event(key);
//! assert!(matches!(action, EditorAction::Submit(ref s) if s == "ls"));
//! ```

pub mod buffer;
pub mod completion;
pub mod completion_popup;
pub mod highlight;
pub mod history;
pub mod input;
pub mod keybindings;
pub mod mode;

// Re-export primary types at module level for convenience.
pub use buffer::EditorBuffer;
pub use completion::{
    ghost_text, CompletionContext, CompletionEngine, CompletionItem, CompletionKind,
};
pub use completion_popup::CompletionPopup;
pub use highlight::{highlight, HighlightStyle, StyledSpan};
pub use history::EditorHistory;
pub use input::InputEditor;
pub use keybindings::EditorAction;
pub use mode::{EditorMode, SearchDirection};
