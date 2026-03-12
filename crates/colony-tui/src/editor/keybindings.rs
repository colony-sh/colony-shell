//! Keymap dispatch for each editor mode.
//!
//! Maps `crossterm::event::KeyEvent` to `EditorAction` based on the current
//! `EditorMode`. Handles multi-key sequences like `dd`, `gg`, `ciw`, `diw`
//! via a pending-key buffer.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::mode::{EditorMode, SearchDirection};

/// Action produced by processing a key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    /// No completed action yet (may be accumulating a multi-key sequence).
    Continue,
    /// Submit the current buffer contents to the shell.
    Submit(String),
    /// Cancel the current input (e.g., Ctrl+C).
    Cancel,
    /// Request tab-completion for the given prefix.
    Complete(String),
    /// Navigate to previous history entry.
    HistoryPrev,
    /// Navigate to next history entry.
    HistoryNext,
    /// Execute a `:` command (e.g., `:q`, `:set`).
    Execute(String),
    /// Start a search with the given query and direction.
    Search(String, SearchDirection),
    /// Navigate to next search match.
    SearchNext,
    /// Navigate to previous search match.
    SearchPrev,
    /// Mode changed (for UI updates).
    ModeChanged(EditorMode),
}

/// Pending state for multi-key sequences in Normal mode.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
pub enum PendingKey {
    #[default]
    None,
    /// Waiting for second key in a two-key sequence.
    /// E.g., `d` waiting for `d`/`i`/`w`, `c` waiting for `c`/`i`/`w`, `g` waiting for `g`.
    Operator(char),
    /// Operator + `i` (e.g., `di` waiting for `w`, `ci` waiting for `w`).
    OperatorInner(char),
}


/// Processes key events and produces editor actions.
///
/// This struct is the core dispatch table. It does NOT own the buffer or mode --
/// those are owned by `InputEditor`, which calls into this module.
#[derive(Debug, Clone)]
pub struct KeyDispatcher {
    pub pending: PendingKey,
}

impl Default for KeyDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyDispatcher {
    pub fn new() -> Self {
        Self {
            pending: PendingKey::None,
        }
    }

    /// Clear the pending key buffer (e.g., on Escape or timeout).
    pub fn clear_pending(&mut self) {
        self.pending = PendingKey::None;
    }

    /// Dispatch a key event given the current mode.
    ///
    /// Returns a `KeyAction` describing what the `InputEditor` should do.
    pub fn dispatch(&mut self, key: KeyEvent, mode: EditorMode) -> KeyAction {
        match mode {
            EditorMode::Normal => self.dispatch_normal(key),
            EditorMode::Insert => self.dispatch_insert(key),
            EditorMode::Command => self.dispatch_command(key),
            EditorMode::Search => self.dispatch_search(key),
        }
    }

    // ── Normal mode dispatch ──────────────────────────────────

    fn dispatch_normal(&mut self, key: KeyEvent) -> KeyAction {
        // Handle multi-key sequences first
        match &self.pending {
            PendingKey::Operator(op) => {
                let op = *op;
                self.pending = PendingKey::None;
                return self.dispatch_operator_second(op, key);
            }
            PendingKey::OperatorInner(op) => {
                let op = *op;
                self.pending = PendingKey::None;
                return self.dispatch_operator_inner(op, key);
            }
            PendingKey::None => {}
        }

        match (key.modifiers, key.code) {
            // Cursor movement
            (KeyModifiers::NONE, KeyCode::Char('h')) | (KeyModifiers::NONE, KeyCode::Left) => {
                KeyAction::MoveLeft
            }
            (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
                KeyAction::MoveDown
            }
            (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
                KeyAction::MoveUp
            }
            (KeyModifiers::NONE, KeyCode::Char('l')) | (KeyModifiers::NONE, KeyCode::Right) => {
                KeyAction::MoveRight
            }

            // Word motions
            (KeyModifiers::NONE, KeyCode::Char('w')) => KeyAction::WordForward,
            (KeyModifiers::NONE, KeyCode::Char('b')) => KeyAction::WordBackward,
            (KeyModifiers::NONE, KeyCode::Char('e')) => KeyAction::WordEnd,

            // Line motions
            (KeyModifiers::NONE, KeyCode::Char('0')) => KeyAction::LineStart,
            (KeyModifiers::NONE, KeyCode::Char('$')) | (KeyModifiers::SHIFT, KeyCode::Char('$')) => {
                KeyAction::LineEnd
            }
            (KeyModifiers::NONE, KeyCode::Char('^')) | (KeyModifiers::SHIFT, KeyCode::Char('^')) => {
                KeyAction::FirstNonBlank
            }

            // Enter insert mode
            (KeyModifiers::NONE, KeyCode::Char('i')) => {
                KeyAction::EnterMode(EditorMode::Insert, InsertPosition::BeforeCursor)
            }
            (KeyModifiers::NONE, KeyCode::Char('a')) => {
                KeyAction::EnterMode(EditorMode::Insert, InsertPosition::AfterCursor)
            }
            (KeyModifiers::SHIFT, KeyCode::Char('I')) => {
                KeyAction::EnterMode(EditorMode::Insert, InsertPosition::LineStart)
            }
            (KeyModifiers::SHIFT, KeyCode::Char('A')) => {
                KeyAction::EnterMode(EditorMode::Insert, InsertPosition::LineEnd)
            }
            (KeyModifiers::NONE, KeyCode::Char('o')) => {
                KeyAction::EnterMode(EditorMode::Insert, InsertPosition::NewLineBelow)
            }
            (KeyModifiers::SHIFT, KeyCode::Char('O')) => {
                KeyAction::EnterMode(EditorMode::Insert, InsertPosition::NewLineAbove)
            }

            // Operators (start multi-key sequences)
            (KeyModifiers::NONE, KeyCode::Char('d')) => {
                self.pending = PendingKey::Operator('d');
                KeyAction::Pending
            }
            (KeyModifiers::NONE, KeyCode::Char('c')) => {
                self.pending = PendingKey::Operator('c');
                KeyAction::Pending
            }
            (KeyModifiers::NONE, KeyCode::Char('g')) => {
                self.pending = PendingKey::Operator('g');
                KeyAction::Pending
            }

            // Single-key actions
            (KeyModifiers::NONE, KeyCode::Char('x')) => KeyAction::DeleteCharForward,
            (KeyModifiers::NONE, KeyCode::Char('p')) => KeyAction::PasteAfter,
            (KeyModifiers::SHIFT, KeyCode::Char('P')) => KeyAction::PasteBefore,
            (KeyModifiers::NONE, KeyCode::Char('u')) => KeyAction::Undo,
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => KeyAction::Redo,

            // Buffer motions
            (KeyModifiers::SHIFT, KeyCode::Char('G')) => KeyAction::BufferEnd,

            // Mode transitions
            (KeyModifiers::NONE, KeyCode::Char(':')) | (KeyModifiers::SHIFT, KeyCode::Char(':')) => {
                KeyAction::EnterCommandMode
            }
            (KeyModifiers::NONE, KeyCode::Char('/')) => {
                KeyAction::EnterSearchMode(SearchDirection::Forward)
            }
            (KeyModifiers::NONE, KeyCode::Char('?')) | (KeyModifiers::SHIFT, KeyCode::Char('?')) => {
                KeyAction::EnterSearchMode(SearchDirection::Backward)
            }

            // Search navigation
            (KeyModifiers::NONE, KeyCode::Char('n')) => KeyAction::SearchNext,
            (KeyModifiers::SHIFT, KeyCode::Char('N')) => KeyAction::SearchPrev,

            // Cancel
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => KeyAction::Cancel,

            // Enter submits in Normal mode too
            (KeyModifiers::NONE, KeyCode::Enter) => KeyAction::Submit,

            _ => KeyAction::Noop,
        }
    }

    /// Handle the second key of an operator sequence like `dd`, `dw`, `di`, `cc`, `cw`, `ci`, `gg`.
    fn dispatch_operator_second(&mut self, operator: char, key: KeyEvent) -> KeyAction {
        match (operator, key.code) {
            // dd - delete line
            ('d', KeyCode::Char('d')) => KeyAction::DeleteLine,
            // dw - delete word
            ('d', KeyCode::Char('w')) => KeyAction::DeleteWord,
            // di - start inner-object sequence
            ('d', KeyCode::Char('i')) => {
                self.pending = PendingKey::OperatorInner('d');
                KeyAction::Pending
            }

            // cc - change line
            ('c', KeyCode::Char('c')) => KeyAction::ChangeLine,
            // cw - change word
            ('c', KeyCode::Char('w')) => KeyAction::ChangeWord,
            // ci - start inner-object sequence
            ('c', KeyCode::Char('i')) => {
                self.pending = PendingKey::OperatorInner('c');
                KeyAction::Pending
            }

            // gg - go to buffer start
            ('g', KeyCode::Char('g')) => KeyAction::BufferStart,

            // Unknown second key - cancel
            _ => KeyAction::Noop,
        }
    }

    /// Handle the third key of an operator-inner sequence like `diw`, `ciw`.
    fn dispatch_operator_inner(&self, operator: char, key: KeyEvent) -> KeyAction {
        match (operator, key.code) {
            ('d', KeyCode::Char('w')) => KeyAction::DeleteInnerWord,
            ('c', KeyCode::Char('w')) => KeyAction::ChangeInnerWord,
            _ => KeyAction::Noop,
        }
    }

    // ── Insert mode dispatch ──────────────────────────────────

    fn dispatch_insert(&self, key: KeyEvent) -> KeyAction {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => KeyAction::EnterNormalMode,
            (KeyModifiers::NONE, KeyCode::Enter) => KeyAction::SubmitOrNewline,
            (KeyModifiers::NONE, KeyCode::Backspace) => KeyAction::DeleteCharBackward,
            (KeyModifiers::NONE, KeyCode::Delete) => KeyAction::DeleteCharForward,
            (KeyModifiers::NONE, KeyCode::Tab) => KeyAction::TriggerCompletion,
            (KeyModifiers::NONE, KeyCode::Up) => KeyAction::HistoryPrev,
            (KeyModifiers::NONE, KeyCode::Down) => KeyAction::HistoryNext,
            (KeyModifiers::NONE, KeyCode::Left) => KeyAction::MoveLeft,
            (KeyModifiers::NONE, KeyCode::Right) => KeyAction::MoveRight,
            (KeyModifiers::NONE, KeyCode::Home) => KeyAction::LineStart,
            (KeyModifiers::NONE, KeyCode::End) => KeyAction::LineEnd,
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => KeyAction::Cancel,
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => KeyAction::DeleteWordBackward,
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => KeyAction::DeleteToLineStart,
            (KeyModifiers::SHIFT, KeyCode::Char(c)) => KeyAction::InsertChar(c),
            (KeyModifiers::NONE, KeyCode::Char(c)) => KeyAction::InsertChar(c),
            // Handle Shift+Tab for completion navigation
            (KeyModifiers::SHIFT, KeyCode::BackTab) => KeyAction::CompletionPrev,
            _ => KeyAction::Noop,
        }
    }

    // ── Command mode dispatch ─────────────────────────────────

    fn dispatch_command(&self, key: KeyEvent) -> KeyAction {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => KeyAction::EnterNormalMode,
            (KeyModifiers::NONE, KeyCode::Enter) => KeyAction::ExecuteCommand,
            (KeyModifiers::NONE, KeyCode::Backspace) => KeyAction::CommandDeleteChar,
            (KeyModifiers::NONE, KeyCode::Left) => KeyAction::CommandCursorLeft,
            (KeyModifiers::NONE, KeyCode::Right) => KeyAction::CommandCursorRight,
            (KeyModifiers::SHIFT, KeyCode::Char(c)) => KeyAction::CommandInsertChar(c),
            (KeyModifiers::NONE, KeyCode::Char(c)) => KeyAction::CommandInsertChar(c),
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => KeyAction::EnterNormalMode,
            _ => KeyAction::Noop,
        }
    }

    // ── Search mode dispatch ──────────────────────────────────

    fn dispatch_search(&self, key: KeyEvent) -> KeyAction {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => KeyAction::EnterNormalMode,
            (KeyModifiers::NONE, KeyCode::Enter) => KeyAction::ExecuteSearch,
            (KeyModifiers::NONE, KeyCode::Backspace) => KeyAction::SearchDeleteChar,
            (KeyModifiers::SHIFT, KeyCode::Char(c)) => KeyAction::SearchInsertChar(c),
            (KeyModifiers::NONE, KeyCode::Char(c)) => KeyAction::SearchInsertChar(c),
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => KeyAction::EnterNormalMode,
            _ => KeyAction::Noop,
        }
    }
}

/// Low-level action produced by key dispatch, before being interpreted by `InputEditor`.
///
/// This is an internal enum consumed by `InputEditor::handle_event`. The editor
/// translates these into buffer mutations and public `EditorAction`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    Noop,
    Pending,

    // Movement
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    WordForward,
    WordBackward,
    WordEnd,
    LineStart,
    LineEnd,
    FirstNonBlank,
    BufferStart,
    BufferEnd,

    // Mode transitions
    EnterMode(EditorMode, InsertPosition),
    EnterNormalMode,
    EnterCommandMode,
    EnterSearchMode(SearchDirection),

    // Editing
    InsertChar(char),
    DeleteCharForward,
    DeleteCharBackward,
    DeleteLine,
    DeleteWord,
    DeleteInnerWord,
    ChangeLine,
    ChangeWord,
    ChangeInnerWord,
    DeleteWordBackward,
    DeleteToLineStart,
    PasteAfter,
    PasteBefore,
    Undo,
    Redo,

    // Submit / Cancel
    Submit,
    SubmitOrNewline,
    Cancel,

    // Completion
    TriggerCompletion,
    CompletionPrev,

    // History
    HistoryPrev,
    HistoryNext,

    // Search
    SearchNext,
    SearchPrev,
    SearchInsertChar(char),
    SearchDeleteChar,
    ExecuteSearch,

    // Command line
    CommandInsertChar(char),
    CommandDeleteChar,
    CommandCursorLeft,
    CommandCursorRight,
    ExecuteCommand,
}

/// Where to position the cursor when entering insert mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertPosition {
    BeforeCursor,
    AfterCursor,
    LineStart,
    LineEnd,
    NewLineBelow,
    NewLineAbove,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn shift_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)
    }

    fn ctrl_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn test_normal_mode_movement() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('h')), EditorMode::Normal), KeyAction::MoveLeft);
        assert_eq!(d.dispatch(key(KeyCode::Char('j')), EditorMode::Normal), KeyAction::MoveDown);
        assert_eq!(d.dispatch(key(KeyCode::Char('k')), EditorMode::Normal), KeyAction::MoveUp);
        assert_eq!(d.dispatch(key(KeyCode::Char('l')), EditorMode::Normal), KeyAction::MoveRight);
    }

    #[test]
    fn test_normal_mode_word_motions() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('w')), EditorMode::Normal), KeyAction::WordForward);
        assert_eq!(d.dispatch(key(KeyCode::Char('b')), EditorMode::Normal), KeyAction::WordBackward);
        assert_eq!(d.dispatch(key(KeyCode::Char('e')), EditorMode::Normal), KeyAction::WordEnd);
    }

    #[test]
    fn test_normal_mode_line_motions() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('0')), EditorMode::Normal), KeyAction::LineStart);
    }

    #[test]
    fn test_enter_insert_mode() {
        let mut d = KeyDispatcher::new();
        assert_eq!(
            d.dispatch(key(KeyCode::Char('i')), EditorMode::Normal),
            KeyAction::EnterMode(EditorMode::Insert, InsertPosition::BeforeCursor)
        );
        assert_eq!(
            d.dispatch(key(KeyCode::Char('a')), EditorMode::Normal),
            KeyAction::EnterMode(EditorMode::Insert, InsertPosition::AfterCursor)
        );
        assert_eq!(
            d.dispatch(key(KeyCode::Char('o')), EditorMode::Normal),
            KeyAction::EnterMode(EditorMode::Insert, InsertPosition::NewLineBelow)
        );
    }

    #[test]
    fn test_shift_insert_modes() {
        let mut d = KeyDispatcher::new();
        assert_eq!(
            d.dispatch(shift_key('I'), EditorMode::Normal),
            KeyAction::EnterMode(EditorMode::Insert, InsertPosition::LineStart)
        );
        assert_eq!(
            d.dispatch(shift_key('A'), EditorMode::Normal),
            KeyAction::EnterMode(EditorMode::Insert, InsertPosition::LineEnd)
        );
        assert_eq!(
            d.dispatch(shift_key('O'), EditorMode::Normal),
            KeyAction::EnterMode(EditorMode::Insert, InsertPosition::NewLineAbove)
        );
    }

    #[test]
    fn test_dd_sequence() {
        let mut d = KeyDispatcher::new();
        let first = d.dispatch(key(KeyCode::Char('d')), EditorMode::Normal);
        assert_eq!(first, KeyAction::Pending);
        assert_eq!(d.pending, PendingKey::Operator('d'));

        let second = d.dispatch(key(KeyCode::Char('d')), EditorMode::Normal);
        assert_eq!(second, KeyAction::DeleteLine);
        assert_eq!(d.pending, PendingKey::None);
    }

    #[test]
    fn test_gg_sequence() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('g')), EditorMode::Normal), KeyAction::Pending);
        assert_eq!(d.dispatch(key(KeyCode::Char('g')), EditorMode::Normal), KeyAction::BufferStart);
    }

    #[test]
    fn test_ciw_sequence() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('c')), EditorMode::Normal), KeyAction::Pending);
        assert_eq!(d.dispatch(key(KeyCode::Char('i')), EditorMode::Normal), KeyAction::Pending);
        assert_eq!(d.dispatch(key(KeyCode::Char('w')), EditorMode::Normal), KeyAction::ChangeInnerWord);
    }

    #[test]
    fn test_diw_sequence() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('d')), EditorMode::Normal), KeyAction::Pending);
        assert_eq!(d.dispatch(key(KeyCode::Char('i')), EditorMode::Normal), KeyAction::Pending);
        assert_eq!(d.dispatch(key(KeyCode::Char('w')), EditorMode::Normal), KeyAction::DeleteInnerWord);
    }

    #[test]
    fn test_insert_mode_typing() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('a')), EditorMode::Insert), KeyAction::InsertChar('a'));
        assert_eq!(d.dispatch(key(KeyCode::Char('z')), EditorMode::Insert), KeyAction::InsertChar('z'));
    }

    #[test]
    fn test_insert_mode_escape() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Esc), EditorMode::Insert), KeyAction::EnterNormalMode);
    }

    #[test]
    fn test_insert_mode_special_keys() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Backspace), EditorMode::Insert), KeyAction::DeleteCharBackward);
        assert_eq!(d.dispatch(key(KeyCode::Delete), EditorMode::Insert), KeyAction::DeleteCharForward);
        assert_eq!(d.dispatch(key(KeyCode::Tab), EditorMode::Insert), KeyAction::TriggerCompletion);
        assert_eq!(d.dispatch(key(KeyCode::Up), EditorMode::Insert), KeyAction::HistoryPrev);
        assert_eq!(d.dispatch(key(KeyCode::Down), EditorMode::Insert), KeyAction::HistoryNext);
    }

    #[test]
    fn test_insert_mode_ctrl_keys() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(ctrl_key('w'), EditorMode::Insert), KeyAction::DeleteWordBackward);
        assert_eq!(d.dispatch(ctrl_key('u'), EditorMode::Insert), KeyAction::DeleteToLineStart);
        assert_eq!(d.dispatch(ctrl_key('c'), EditorMode::Insert), KeyAction::Cancel);
    }

    #[test]
    fn test_command_mode() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('q')), EditorMode::Command), KeyAction::CommandInsertChar('q'));
        assert_eq!(d.dispatch(key(KeyCode::Enter), EditorMode::Command), KeyAction::ExecuteCommand);
        assert_eq!(d.dispatch(key(KeyCode::Esc), EditorMode::Command), KeyAction::EnterNormalMode);
        assert_eq!(d.dispatch(key(KeyCode::Backspace), EditorMode::Command), KeyAction::CommandDeleteChar);
    }

    #[test]
    fn test_search_mode() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('f')), EditorMode::Search), KeyAction::SearchInsertChar('f'));
        assert_eq!(d.dispatch(key(KeyCode::Enter), EditorMode::Search), KeyAction::ExecuteSearch);
        assert_eq!(d.dispatch(key(KeyCode::Esc), EditorMode::Search), KeyAction::EnterNormalMode);
    }

    #[test]
    fn test_normal_mode_enter_command_mode() {
        let mut d = KeyDispatcher::new();
        assert_eq!(
            d.dispatch(shift_key(':'), EditorMode::Normal),
            KeyAction::EnterCommandMode
        );
    }

    #[test]
    fn test_normal_mode_enter_search_mode() {
        let mut d = KeyDispatcher::new();
        assert_eq!(
            d.dispatch(key(KeyCode::Char('/')), EditorMode::Normal),
            KeyAction::EnterSearchMode(SearchDirection::Forward)
        );
    }

    #[test]
    fn test_search_navigation_in_normal() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('n')), EditorMode::Normal), KeyAction::SearchNext);
        assert_eq!(d.dispatch(shift_key('N'), EditorMode::Normal), KeyAction::SearchPrev);
    }

    #[test]
    fn test_undo_redo_normal() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('u')), EditorMode::Normal), KeyAction::Undo);
        assert_eq!(d.dispatch(ctrl_key('r'), EditorMode::Normal), KeyAction::Redo);
    }

    #[test]
    fn test_paste_normal() {
        let mut d = KeyDispatcher::new();
        assert_eq!(d.dispatch(key(KeyCode::Char('p')), EditorMode::Normal), KeyAction::PasteAfter);
        assert_eq!(d.dispatch(shift_key('P'), EditorMode::Normal), KeyAction::PasteBefore);
    }
}
