//! Editor modes (vim-style)

/// Editor mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Normal mode - navigation and commands
    #[default]
    Normal,
    /// Insert mode - text input
    Insert,
    /// Command mode - ex commands (:)
    Command,
    /// Search mode - incremental search (/)
    Search,
    /// History search mode - fzf-style fuzzy search (Ctrl+r)
    HistorySearch,
    /// Command palette mode - quick action search (Shift+Cmd+P)
    CommandPalette,
    /// Prompt widget mode - interactive popup (file picker, confirm, etc.)
    PromptWidget,
}

impl Mode {
    /// Get the display name for the mode
    pub fn name(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Command => "COMMAND",
            Mode::Search => "SEARCH",
            Mode::HistorySearch => "HISTORY",
            Mode::CommandPalette => "PALETTE",
            Mode::PromptWidget => "PROMPT",
        }
    }

    /// Get a short indicator for the mode
    pub fn indicator(&self) -> &'static str {
        match self {
            Mode::Normal => "N",
            Mode::Insert => "I",
            Mode::Command => ":",
            Mode::Search => "/",
            Mode::HistorySearch => "?",
            Mode::CommandPalette => ">",
            Mode::PromptWidget => "W",
        }
    }
}
