//! Color theme and styling for the TUI

use ratatui::style::{Color, Modifier, Style};

/// TUI color theme
#[derive(Debug, Clone)]
pub struct Theme {
    /// Background color
    pub bg: Color,
    /// Foreground text color
    pub fg: Color,
    /// Prompt area background
    pub prompt_bg: Color,
    /// Input area background
    pub input_bg: Color,
    /// Mode indicator colors
    pub mode_normal: Color,
    pub mode_insert: Color,
    pub mode_command: Color,
    /// Syntax highlighting
    pub syntax_keyword: Color,
    pub syntax_string: Color,
    pub syntax_variable: Color,
    pub syntax_command: Color,
    pub syntax_operator: Color,
    pub syntax_comment: Color,
    /// Status colors
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    /// Selection/highlight
    pub selection: Color,
    pub cursor: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::Reset,
            prompt_bg: Color::Reset,
            input_bg: Color::Reset,
            mode_normal: Color::Blue,
            mode_insert: Color::Green,
            mode_command: Color::Yellow,
            syntax_keyword: Color::Magenta,
            syntax_string: Color::Green,
            syntax_variable: Color::Cyan,
            syntax_command: Color::Yellow,
            syntax_operator: Color::White,
            syntax_comment: Color::DarkGray,
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Blue,
            selection: Color::DarkGray,
            cursor: Color::White,
        }
    }
}

impl Theme {
    /// Style for normal mode indicator
    pub fn mode_style(&self, mode: &crate::keybindings::Mode) -> Style {
        let color = match mode {
            crate::keybindings::Mode::Normal => self.mode_normal,
            crate::keybindings::Mode::Insert => self.mode_insert,
            crate::keybindings::Mode::Command => self.mode_command,
            crate::keybindings::Mode::Search => self.info,
            crate::keybindings::Mode::HistorySearch => Color::Cyan,
            crate::keybindings::Mode::CommandPalette => Color::Magenta,
            crate::keybindings::Mode::PromptWidget => Color::Yellow,
        };
        Style::default()
            .fg(Color::Black)
            .bg(color)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for prompt text
    pub fn prompt_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.prompt_bg)
    }

    /// Style for input text
    pub fn input_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.input_bg)
    }

    /// Style for cursor
    pub fn cursor_style(&self) -> Style {
        Style::default().fg(self.bg).bg(self.cursor)
    }

    /// Style for error messages
    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    /// Style for success messages
    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }
}
