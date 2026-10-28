//! Modal editing state machine for the vim-mode input editor.
//!
//! Defines the editor modes (Normal, Insert, Command, Search) and
//! valid transitions between them.

/// The current editing mode of the input editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditorMode {
    /// Normal mode: navigation, motions, operators.
    #[default]
    Normal,
    /// Insert mode: typing text.
    Insert,
    /// Command-line mode (after `:`).
    Command,
    /// Search mode (after `/` or `?`).
    Search,
}

impl EditorMode {
    /// Returns a short indicator string suitable for display in a status bar.
    pub fn indicator(&self) -> &'static str {
        match self {
            EditorMode::Normal => "NORMAL",
            EditorMode::Insert => "INSERT",
            EditorMode::Command => "COMMAND",
            EditorMode::Search => "SEARCH",
        }
    }

    /// Returns whether the mode accepts arbitrary text input.
    pub fn is_text_entry(&self) -> bool {
        matches!(
            self,
            EditorMode::Insert | EditorMode::Command | EditorMode::Search
        )
    }
}

/// The direction of a search operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode_is_normal() {
        assert_eq!(EditorMode::default(), EditorMode::Normal);
    }

    #[test]
    fn test_indicator_strings() {
        assert_eq!(EditorMode::Normal.indicator(), "NORMAL");
        assert_eq!(EditorMode::Insert.indicator(), "INSERT");
        assert_eq!(EditorMode::Command.indicator(), "COMMAND");
        assert_eq!(EditorMode::Search.indicator(), "SEARCH");
    }

    #[test]
    fn test_is_text_entry() {
        assert!(!EditorMode::Normal.is_text_entry());
        assert!(EditorMode::Insert.is_text_entry());
        assert!(EditorMode::Command.is_text_entry());
        assert!(EditorMode::Search.is_text_entry());
    }
}
