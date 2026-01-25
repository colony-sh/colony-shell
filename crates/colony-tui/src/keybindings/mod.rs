//! Keybinding system with vim-style modal editing

mod actions;
mod modes;

pub use actions::Action;
pub use modes::Mode;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Maps key events to actions based on current mode
pub struct KeyMapper {
    mode: Mode,
}

impl KeyMapper {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// Map a key event to an action
    pub fn map(&self, key: KeyEvent) -> Option<Action> {
        // Global keybindings (work in all modes)
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Some(Action::Interrupt),
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => return Some(Action::Eof),
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => return Some(Action::CommandPalette),
            (KeyModifiers::CONTROL, KeyCode::Char('b')) => return Some(Action::ToggleLeftSidebar),
            (KeyModifiers::CONTROL, KeyCode::Char('n')) => return Some(Action::ToggleRightSidebar),
            _ => {}
        }

        match self.mode {
            Mode::Normal => self.map_normal(key),
            Mode::Insert => self.map_insert(key),
            Mode::Command => self.map_command(key),
            Mode::Search => self.map_search(key),
        }
    }

    fn map_normal(&self, key: KeyEvent) -> Option<Action> {
        match (key.modifiers, key.code) {
            // Enter insert mode
            (KeyModifiers::NONE, KeyCode::Char('i')) => Some(Action::EnterInsert),
            (KeyModifiers::NONE, KeyCode::Char('a')) => Some(Action::EnterInsertAfter),
            (KeyModifiers::NONE, KeyCode::Char('A')) => Some(Action::EnterInsertEnd),
            (KeyModifiers::NONE, KeyCode::Char('I')) => Some(Action::EnterInsertStart),
            (KeyModifiers::NONE, KeyCode::Char('o')) => Some(Action::InsertLineBelow),
            (KeyModifiers::NONE, KeyCode::Char('O')) => Some(Action::InsertLineAbove),

            // Navigation
            (KeyModifiers::NONE, KeyCode::Char('h')) | (KeyModifiers::NONE, KeyCode::Left) => {
                Some(Action::CursorLeft)
            }
            (KeyModifiers::NONE, KeyCode::Char('l')) | (KeyModifiers::NONE, KeyCode::Right) => {
                Some(Action::CursorRight)
            }
            (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
                Some(Action::ScrollDown)
            }
            (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
                Some(Action::ScrollUp)
            }
            (KeyModifiers::NONE, KeyCode::Char('0')) => Some(Action::CursorLineStart),
            (KeyModifiers::NONE, KeyCode::Char('$')) => Some(Action::CursorLineEnd),
            (KeyModifiers::NONE, KeyCode::Char('w')) => Some(Action::CursorWordForward),
            (KeyModifiers::NONE, KeyCode::Char('b')) => Some(Action::CursorWordBackward),
            (KeyModifiers::NONE, KeyCode::Char('e')) => Some(Action::CursorWordEnd),

            // Scroll content
            (KeyModifiers::NONE, KeyCode::Char('g')) => Some(Action::ScrollToTop),
            (KeyModifiers::SHIFT, KeyCode::Char('G')) => Some(Action::ScrollToBottom),
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => Some(Action::ScrollHalfPageUp),
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => Some(Action::ScrollHalfPageDown),

            // Enter command mode
            (KeyModifiers::NONE, KeyCode::Char(':')) => Some(Action::EnterCommand),

            // Search
            (KeyModifiers::NONE, KeyCode::Char('/')) => Some(Action::EnterSearch),

            // Delete
            (KeyModifiers::NONE, KeyCode::Char('x')) => Some(Action::DeleteChar),
            (KeyModifiers::NONE, KeyCode::Char('d')) => Some(Action::DeletePending),

            // Yank/paste
            (KeyModifiers::NONE, KeyCode::Char('y')) => Some(Action::YankPending),
            (KeyModifiers::NONE, KeyCode::Char('p')) => Some(Action::PasteAfter),
            (KeyModifiers::SHIFT, KeyCode::Char('P')) => Some(Action::PasteBefore),

            // Undo/redo
            (KeyModifiers::NONE, KeyCode::Char('u')) => Some(Action::Undo),
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => Some(Action::Redo),

            _ => None,
        }
    }

    fn map_insert(&self, key: KeyEvent) -> Option<Action> {
        match (key.modifiers, key.code) {
            // Exit insert mode
            (KeyModifiers::NONE, KeyCode::Esc) => Some(Action::EnterNormal),

            // Text input
            (KeyModifiers::NONE, KeyCode::Char(c))
            | (KeyModifiers::SHIFT, KeyCode::Char(c)) => Some(Action::InsertChar(c)),
            (KeyModifiers::NONE, KeyCode::Backspace) => Some(Action::DeleteCharBackward),
            (KeyModifiers::NONE, KeyCode::Delete) => Some(Action::DeleteChar),
            (KeyModifiers::NONE, KeyCode::Enter) => Some(Action::Execute),

            // Navigation (allow in insert mode too)
            (KeyModifiers::NONE, KeyCode::Left) => Some(Action::CursorLeft),
            (KeyModifiers::NONE, KeyCode::Right) => Some(Action::CursorRight),
            (KeyModifiers::NONE, KeyCode::Home) => Some(Action::CursorLineStart),
            (KeyModifiers::NONE, KeyCode::End) => Some(Action::CursorLineEnd),

            // History
            (KeyModifiers::NONE, KeyCode::Up) => Some(Action::HistoryPrev),
            (KeyModifiers::NONE, KeyCode::Down) => Some(Action::HistoryNext),
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => Some(Action::HistorySearch),

            // Completion
            (KeyModifiers::NONE, KeyCode::Tab) => Some(Action::AcceptCompletion),
            (KeyModifiers::CONTROL, KeyCode::Char('n')) => Some(Action::NextCompletion),
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => Some(Action::PrevCompletion),

            // Word operations
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => Some(Action::DeleteWordBackward),
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => Some(Action::DeleteToLineStart),
            (KeyModifiers::CONTROL, KeyCode::Char('k')) => Some(Action::DeleteToLineEnd),

            _ => None,
        }
    }

    fn map_command(&self, key: KeyEvent) -> Option<Action> {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => Some(Action::EnterNormal),
            (KeyModifiers::NONE, KeyCode::Enter) => Some(Action::ExecuteCommand),
            (KeyModifiers::NONE, KeyCode::Char(c))
            | (KeyModifiers::SHIFT, KeyCode::Char(c)) => Some(Action::InsertChar(c)),
            (KeyModifiers::NONE, KeyCode::Backspace) => Some(Action::DeleteCharBackward),
            (KeyModifiers::NONE, KeyCode::Left) => Some(Action::CursorLeft),
            (KeyModifiers::NONE, KeyCode::Right) => Some(Action::CursorRight),
            _ => None,
        }
    }

    fn map_search(&self, key: KeyEvent) -> Option<Action> {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => Some(Action::EnterNormal),
            (KeyModifiers::NONE, KeyCode::Enter) => Some(Action::ExecuteSearch),
            (KeyModifiers::NONE, KeyCode::Char(c))
            | (KeyModifiers::SHIFT, KeyCode::Char(c)) => Some(Action::InsertChar(c)),
            (KeyModifiers::NONE, KeyCode::Backspace) => Some(Action::DeleteCharBackward),
            (KeyModifiers::CONTROL, KeyCode::Char('n')) => Some(Action::SearchNext),
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => Some(Action::SearchPrev),
            _ => None,
        }
    }
}

impl Default for KeyMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_mode_i_enters_insert() {
        let mapper = KeyMapper::new();
        let key = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        assert_eq!(mapper.map(key), Some(Action::EnterInsert));
    }

    #[test]
    fn test_insert_mode_esc_enters_normal() {
        let mut mapper = KeyMapper::new();
        mapper.set_mode(Mode::Insert);
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(mapper.map(key), Some(Action::EnterNormal));
    }

    #[test]
    fn test_global_ctrl_c() {
        let mapper = KeyMapper::new();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(mapper.map(key), Some(Action::Interrupt));
    }
}
