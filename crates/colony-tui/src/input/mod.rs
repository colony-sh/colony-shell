//! Input editor module

mod command_palette;
mod completion;
mod editor;
mod highlight;
mod history_search;

pub use command_palette::{CommandPalette, PaletteAction, PaletteCategory, PaletteEntry};
pub use completion::{GhostCompletion, GhostSuggestion, SuggestionSource};
pub use editor::Editor;
pub use highlight::Highlighter;
pub use history_search::{HistoryMatch, HistorySearch};
