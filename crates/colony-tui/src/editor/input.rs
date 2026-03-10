//! `InputEditor` — the primary vim-mode input editor struct.
//!
//! Owns the `EditorBuffer`, `EditorMode`, and `KeyDispatcher`. Processes
//! `crossterm::event::KeyEvent`s and returns high-level `EditorAction`s.

use crossterm::event::KeyEvent;

use super::buffer::EditorBuffer;
use super::keybindings::{EditorAction, InsertPosition, KeyAction, KeyDispatcher};
use super::mode::{EditorMode, SearchDirection};

/// The main input editor with vim-mode support.
///
/// All logic is in pure data structures; no terminal I/O happens here.
/// Rendering is handled externally by reading the editor state.
#[derive(Debug, Clone)]
pub struct InputEditor {
    /// The text buffer.
    pub buffer: EditorBuffer,
    /// Current editing mode.
    mode: EditorMode,
    /// Key dispatch table and pending multi-key state.
    dispatcher: KeyDispatcher,
    /// Command-line buffer (for `:` commands).
    command_buffer: String,
    /// Command-line cursor position.
    command_cursor: usize,
    /// Search buffer (for `/` and `?` searches).
    search_buffer: String,
    /// Search direction.
    search_direction: SearchDirection,
    /// Whether the editor is in multi-line mode (e.g., heredoc, continuation).
    multi_line: bool,
}

impl Default for InputEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl InputEditor {
    /// Create a new editor in Insert mode (ready for typing).
    pub fn new() -> Self {
        Self {
            buffer: EditorBuffer::new(),
            mode: EditorMode::Insert,
            dispatcher: KeyDispatcher::new(),
            command_buffer: String::new(),
            command_cursor: 0,
            search_buffer: String::new(),
            search_direction: SearchDirection::Forward,
            multi_line: false,
        }
    }

    /// Create a new editor starting in the specified mode.
    pub fn with_mode(mode: EditorMode) -> Self {
        Self {
            mode,
            ..Self::new()
        }
    }

    // ── Accessors ─────────────────────────────────────────────

    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    /// Override the current mode (e.g., to force insert mode).
    pub fn set_mode(&mut self, mode: EditorMode) {
        self.mode = mode;
    }

    pub fn command_buffer(&self) -> &str {
        &self.command_buffer
    }

    pub fn search_buffer(&self) -> &str {
        &self.search_buffer
    }

    pub fn search_direction(&self) -> SearchDirection {
        self.search_direction
    }

    pub fn is_multi_line(&self) -> bool {
        self.multi_line
    }

    pub fn set_multi_line(&mut self, multi_line: bool) {
        self.multi_line = multi_line;
    }

    /// Returns the status line text for the current mode.
    pub fn status_line(&self) -> String {
        match self.mode {
            EditorMode::Normal => {
                let pending = match &self.dispatcher.pending {
                    super::keybindings::PendingKey::None => String::new(),
                    super::keybindings::PendingKey::Operator(c) => format!("{}", c),
                    super::keybindings::PendingKey::OperatorInner(c) => format!("{}i", c),
                };
                if pending.is_empty() {
                    "-- NORMAL --".to_string()
                } else {
                    format!("-- NORMAL -- {}", pending)
                }
            }
            EditorMode::Insert => "-- INSERT --".to_string(),
            EditorMode::Command => format!(":{}", self.command_buffer),
            EditorMode::Search => {
                let prefix = match self.search_direction {
                    SearchDirection::Forward => "/",
                    SearchDirection::Backward => "?",
                };
                format!("{}{}", prefix, self.search_buffer)
            }
        }
    }

    /// Get the current buffer text as a string ready for submission.
    pub fn text(&self) -> String {
        self.buffer.contents()
    }

    /// Set the buffer text (e.g., from history navigation).
    pub fn set_text(&mut self, text: &str) {
        self.buffer.set_contents(text);
        // Place cursor at end
        let last_row = self.buffer.line_count().saturating_sub(1);
        let last_col = self.buffer.lines()[last_row].len();
        self.buffer.set_cursor(last_row, last_col);
    }

    /// Clear the editor for a new input line.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.mode = EditorMode::Insert;
        self.dispatcher.clear_pending();
        self.command_buffer.clear();
        self.command_cursor = 0;
        self.search_buffer.clear();
    }

    // ── Event handling ────────────────────────────────────────

    /// Process a key event and return the resulting editor action.
    ///
    /// This is the main entry point for input handling. The caller
    /// should match on the returned `EditorAction` to decide what
    /// to do (submit, cancel, complete, etc.).
    pub fn handle_event(&mut self, key: KeyEvent) -> EditorAction {
        let action = self.dispatcher.dispatch(key, self.mode);
        self.apply_action(action)
    }

    /// Apply a `KeyAction` to the editor state and produce an `EditorAction`.
    fn apply_action(&mut self, action: KeyAction) -> EditorAction {
        match action {
            KeyAction::Noop | KeyAction::Pending => EditorAction::Continue,

            // ── Movement ──────────────────────────────────────
            KeyAction::MoveLeft => {
                self.buffer.move_left();
                EditorAction::Continue
            }
            KeyAction::MoveRight => {
                self.buffer.move_right();
                EditorAction::Continue
            }
            KeyAction::MoveUp => {
                self.buffer.move_up();
                EditorAction::Continue
            }
            KeyAction::MoveDown => {
                self.buffer.move_down();
                EditorAction::Continue
            }
            KeyAction::WordForward => {
                self.buffer.move_word_forward();
                EditorAction::Continue
            }
            KeyAction::WordBackward => {
                self.buffer.move_word_backward();
                EditorAction::Continue
            }
            KeyAction::WordEnd => {
                self.buffer.move_word_end();
                EditorAction::Continue
            }
            KeyAction::LineStart => {
                self.buffer.move_to_line_start();
                EditorAction::Continue
            }
            KeyAction::LineEnd => {
                self.buffer.move_to_line_end();
                EditorAction::Continue
            }
            KeyAction::FirstNonBlank => {
                self.buffer.move_to_first_non_blank();
                EditorAction::Continue
            }
            KeyAction::BufferStart => {
                self.buffer.move_to_buffer_start();
                EditorAction::Continue
            }
            KeyAction::BufferEnd => {
                self.buffer.move_to_buffer_end();
                EditorAction::Continue
            }

            // ── Mode transitions ──────────────────────────────
            KeyAction::EnterMode(mode, position) => {
                self.enter_insert_mode(position);
                self.mode = mode;
                EditorAction::ModeChanged(mode)
            }
            KeyAction::EnterNormalMode => {
                self.mode = EditorMode::Normal;
                self.dispatcher.clear_pending();
                // In normal mode, cursor shouldn't go past end of line content
                if self.buffer.cursor_col() > 0
                    && self.buffer.cursor_col() >= self.buffer.current_line().len()
                {
                    self.buffer.move_left();
                }
                EditorAction::ModeChanged(EditorMode::Normal)
            }
            KeyAction::EnterCommandMode => {
                self.mode = EditorMode::Command;
                self.command_buffer.clear();
                self.command_cursor = 0;
                EditorAction::ModeChanged(EditorMode::Command)
            }
            KeyAction::EnterSearchMode(direction) => {
                self.mode = EditorMode::Search;
                self.search_buffer.clear();
                self.search_direction = direction;
                EditorAction::ModeChanged(EditorMode::Search)
            }

            // ── Insert editing ────────────────────────────────
            KeyAction::InsertChar(c) => {
                self.buffer.insert_char(c);
                EditorAction::Continue
            }
            KeyAction::DeleteCharBackward => {
                self.buffer.delete_char_backward();
                EditorAction::Continue
            }
            KeyAction::DeleteCharForward => {
                self.buffer.save_snapshot();
                self.buffer.delete_char_forward();
                EditorAction::Continue
            }
            KeyAction::DeleteWordBackward => {
                self.buffer.delete_word_backward();
                EditorAction::Continue
            }
            KeyAction::DeleteToLineStart => {
                self.buffer.delete_to_line_start();
                EditorAction::Continue
            }

            // ── Normal-mode editing ───────────────────────────
            KeyAction::DeleteLine => {
                self.buffer.delete_line();
                EditorAction::Continue
            }
            KeyAction::DeleteWord => {
                self.buffer.save_snapshot();
                self.buffer.move_word_forward();
                // A simple approach: delete from original pos to new pos
                // For a more correct implementation, we'd track the range.
                // For now, delete_inner_word is more useful. This is a placeholder.
                EditorAction::Continue
            }
            KeyAction::DeleteInnerWord => {
                self.buffer.delete_inner_word();
                EditorAction::Continue
            }
            KeyAction::ChangeLine => {
                self.buffer.change_line();
                self.mode = EditorMode::Insert;
                EditorAction::ModeChanged(EditorMode::Insert)
            }
            KeyAction::ChangeWord => {
                self.buffer.save_snapshot();
                // Change word: delete to next word boundary and enter insert
                self.buffer.delete_inner_word();
                self.mode = EditorMode::Insert;
                EditorAction::ModeChanged(EditorMode::Insert)
            }
            KeyAction::ChangeInnerWord => {
                self.buffer.change_inner_word();
                self.mode = EditorMode::Insert;
                EditorAction::ModeChanged(EditorMode::Insert)
            }
            KeyAction::PasteAfter => {
                self.buffer.paste_after();
                EditorAction::Continue
            }
            KeyAction::PasteBefore => {
                self.buffer.paste_before();
                EditorAction::Continue
            }
            KeyAction::Undo => {
                self.buffer.undo();
                EditorAction::Continue
            }
            KeyAction::Redo => {
                self.buffer.redo();
                EditorAction::Continue
            }

            // ── Submit / Cancel ───────────────────────────────
            KeyAction::Submit => {
                let text = self.buffer.contents();
                EditorAction::Submit(text)
            }
            KeyAction::SubmitOrNewline => {
                if self.multi_line {
                    self.buffer.insert_newline();
                    EditorAction::Continue
                } else {
                    let text = self.buffer.contents();
                    EditorAction::Submit(text)
                }
            }
            KeyAction::Cancel => EditorAction::Cancel,

            // ── Completion ────────────────────────────────────
            KeyAction::TriggerCompletion => {
                let prefix = self.current_word_prefix();
                EditorAction::Complete(prefix)
            }
            KeyAction::CompletionPrev => {
                // Handled by the completion popup externally
                EditorAction::Continue
            }

            // ── History ───────────────────────────────────────
            KeyAction::HistoryPrev => EditorAction::HistoryPrev,
            KeyAction::HistoryNext => EditorAction::HistoryNext,

            // ── Search ────────────────────────────────────────
            KeyAction::SearchInsertChar(c) => {
                self.search_buffer.push(c);
                EditorAction::Continue
            }
            KeyAction::SearchDeleteChar => {
                self.search_buffer.pop();
                EditorAction::Continue
            }
            KeyAction::ExecuteSearch => {
                let query = self.search_buffer.clone();
                let direction = self.search_direction;
                self.mode = EditorMode::Normal;
                EditorAction::Search(query, direction)
            }
            KeyAction::SearchNext => EditorAction::SearchNext,
            KeyAction::SearchPrev => EditorAction::SearchPrev,

            // ── Command line ──────────────────────────────────
            KeyAction::CommandInsertChar(c) => {
                self.command_buffer.insert(self.command_cursor, c);
                self.command_cursor += 1;
                EditorAction::Continue
            }
            KeyAction::CommandDeleteChar => {
                if self.command_cursor > 0 {
                    self.command_cursor -= 1;
                    self.command_buffer.remove(self.command_cursor);
                } else if self.command_buffer.is_empty() {
                    // Empty command line + backspace = cancel
                    self.mode = EditorMode::Normal;
                    return EditorAction::ModeChanged(EditorMode::Normal);
                }
                EditorAction::Continue
            }
            KeyAction::CommandCursorLeft => {
                if self.command_cursor > 0 {
                    self.command_cursor -= 1;
                }
                EditorAction::Continue
            }
            KeyAction::CommandCursorRight => {
                if self.command_cursor < self.command_buffer.len() {
                    self.command_cursor += 1;
                }
                EditorAction::Continue
            }
            KeyAction::ExecuteCommand => {
                let cmd = self.command_buffer.clone();
                self.command_buffer.clear();
                self.command_cursor = 0;
                self.mode = EditorMode::Normal;
                EditorAction::Execute(cmd)
            }
        }
    }

    // ── Helpers ───────────────────────────────────────────────

    /// Position the cursor appropriately when entering insert mode.
    fn enter_insert_mode(&mut self, position: InsertPosition) {
        match position {
            InsertPosition::BeforeCursor => {
                // Cursor stays where it is
            }
            InsertPosition::AfterCursor => {
                self.buffer.move_right();
            }
            InsertPosition::LineStart => {
                self.buffer.move_to_first_non_blank();
            }
            InsertPosition::LineEnd => {
                self.buffer.move_to_line_end();
            }
            InsertPosition::NewLineBelow => {
                self.buffer.open_line_below();
            }
            InsertPosition::NewLineAbove => {
                self.buffer.open_line_above();
            }
        }
    }

    /// Extract the word prefix under the cursor for completion.
    fn current_word_prefix(&self) -> String {
        let line = self.buffer.current_line();
        let col = self.buffer.cursor_col();
        let chars: Vec<char> = line.chars().collect();

        if col == 0 || chars.is_empty() {
            return String::new();
        }

        let mut start = col;
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }

        chars[start..col].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

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
    fn test_new_editor_starts_in_insert_mode() {
        let editor = InputEditor::new();
        assert_eq!(editor.mode(), EditorMode::Insert);
    }

    #[test]
    fn test_type_text_in_insert_mode() {
        let mut editor = InputEditor::new();
        editor.handle_event(key(KeyCode::Char('h')));
        editor.handle_event(key(KeyCode::Char('e')));
        editor.handle_event(key(KeyCode::Char('l')));
        editor.handle_event(key(KeyCode::Char('l')));
        editor.handle_event(key(KeyCode::Char('o')));
        assert_eq!(editor.text(), "hello");
    }

    #[test]
    fn test_escape_to_normal_mode() {
        let mut editor = InputEditor::new();
        editor.handle_event(key(KeyCode::Char('h')));
        editor.handle_event(key(KeyCode::Char('i')));
        let action = editor.handle_event(key(KeyCode::Esc));
        assert_eq!(editor.mode(), EditorMode::Normal);
        assert!(matches!(action, EditorAction::ModeChanged(EditorMode::Normal)));
    }

    #[test]
    fn test_i_to_insert_from_normal() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.buffer = EditorBuffer::with_text("hello");
        editor.buffer.set_cursor(0, 2);

        let action = editor.handle_event(key(KeyCode::Char('i')));
        assert_eq!(editor.mode(), EditorMode::Insert);
        assert!(matches!(action, EditorAction::ModeChanged(EditorMode::Insert)));
        assert_eq!(editor.buffer.cursor_col(), 2); // cursor stays
    }

    #[test]
    fn test_a_to_insert_after() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.buffer = EditorBuffer::with_text("hello");
        editor.buffer.set_cursor(0, 2);

        editor.handle_event(key(KeyCode::Char('a')));
        assert_eq!(editor.mode(), EditorMode::Insert);
        assert_eq!(editor.buffer.cursor_col(), 3); // cursor moves right
    }

    #[test]
    fn test_submit_on_enter_insert_mode() {
        let mut editor = InputEditor::new();
        editor.handle_event(key(KeyCode::Char('l')));
        editor.handle_event(key(KeyCode::Char('s')));
        let action = editor.handle_event(key(KeyCode::Enter));
        assert_eq!(action, EditorAction::Submit("ls".to_string()));
    }

    #[test]
    fn test_submit_on_enter_normal_mode() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.buffer = EditorBuffer::with_text("ls -la");
        let action = editor.handle_event(key(KeyCode::Enter));
        assert_eq!(action, EditorAction::Submit("ls -la".to_string()));
    }

    #[test]
    fn test_cancel_ctrl_c() {
        let mut editor = InputEditor::new();
        editor.handle_event(key(KeyCode::Char('h')));
        let action = editor.handle_event(ctrl_key('c'));
        assert_eq!(action, EditorAction::Cancel);
    }

    #[test]
    fn test_enter_command_mode() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.handle_event(shift_key(':'));
        assert_eq!(editor.mode(), EditorMode::Command);
    }

    #[test]
    fn test_command_mode_typing_and_execute() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.handle_event(shift_key(':'));
        editor.handle_event(key(KeyCode::Char('q')));
        assert_eq!(editor.command_buffer(), "q");

        let action = editor.handle_event(key(KeyCode::Enter));
        assert_eq!(action, EditorAction::Execute("q".to_string()));
        assert_eq!(editor.mode(), EditorMode::Normal);
    }

    #[test]
    fn test_command_mode_escape() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.handle_event(shift_key(':'));
        editor.handle_event(key(KeyCode::Char('q')));
        editor.handle_event(key(KeyCode::Esc));
        assert_eq!(editor.mode(), EditorMode::Normal);
    }

    #[test]
    fn test_search_mode_forward() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.handle_event(key(KeyCode::Char('/')));
        assert_eq!(editor.mode(), EditorMode::Search);
        assert_eq!(editor.search_direction(), SearchDirection::Forward);

        editor.handle_event(key(KeyCode::Char('f')));
        editor.handle_event(key(KeyCode::Char('o')));
        editor.handle_event(key(KeyCode::Char('o')));
        assert_eq!(editor.search_buffer(), "foo");

        let action = editor.handle_event(key(KeyCode::Enter));
        assert_eq!(
            action,
            EditorAction::Search("foo".to_string(), SearchDirection::Forward)
        );
        assert_eq!(editor.mode(), EditorMode::Normal);
    }

    #[test]
    fn test_dd_in_normal_mode() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.buffer = EditorBuffer::with_text("first\nsecond\nthird");
        editor.buffer.set_cursor(1, 0);

        editor.handle_event(key(KeyCode::Char('d')));
        editor.handle_event(key(KeyCode::Char('d')));

        assert_eq!(editor.buffer.line_count(), 2);
        assert_eq!(editor.buffer.lines()[0], "first");
        assert_eq!(editor.buffer.lines()[1], "third");
    }

    #[test]
    fn test_ciw_in_normal_mode() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.buffer = EditorBuffer::with_text("hello world");
        editor.buffer.set_cursor(0, 7);

        editor.handle_event(key(KeyCode::Char('c')));
        editor.handle_event(key(KeyCode::Char('i')));
        let action = editor.handle_event(key(KeyCode::Char('w')));

        assert!(matches!(action, EditorAction::ModeChanged(EditorMode::Insert)));
        assert_eq!(editor.mode(), EditorMode::Insert);
        assert_eq!(editor.buffer.contents(), "hello ");
    }

    #[test]
    fn test_history_navigation_insert_mode() {
        let mut editor = InputEditor::new();
        assert_eq!(editor.handle_event(key(KeyCode::Up)), EditorAction::HistoryPrev);
        assert_eq!(editor.handle_event(key(KeyCode::Down)), EditorAction::HistoryNext);
    }

    #[test]
    fn test_tab_completion() {
        let mut editor = InputEditor::new();
        editor.handle_event(key(KeyCode::Char('l')));
        editor.handle_event(key(KeyCode::Char('s')));
        let action = editor.handle_event(key(KeyCode::Tab));
        assert_eq!(action, EditorAction::Complete("ls".to_string()));
    }

    #[test]
    fn test_multiline_enter_inserts_newline() {
        let mut editor = InputEditor::new();
        editor.set_multi_line(true);
        editor.handle_event(key(KeyCode::Char('a')));
        let action = editor.handle_event(key(KeyCode::Enter));
        assert_eq!(action, EditorAction::Continue);
        assert_eq!(editor.buffer.line_count(), 2);
    }

    #[test]
    fn test_set_text_and_reset() {
        let mut editor = InputEditor::new();
        editor.set_text("hello world");
        assert_eq!(editor.text(), "hello world");

        editor.reset();
        assert_eq!(editor.text(), "");
        assert_eq!(editor.mode(), EditorMode::Insert);
    }

    #[test]
    fn test_status_line() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        assert_eq!(editor.status_line(), "-- NORMAL --");

        editor.handle_event(key(KeyCode::Char('i')));
        assert_eq!(editor.status_line(), "-- INSERT --");
    }

    #[test]
    fn test_gg_goes_to_buffer_start() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.buffer = EditorBuffer::with_text("first\nsecond\nthird");
        editor.buffer.set_cursor(2, 3);

        editor.handle_event(key(KeyCode::Char('g')));
        editor.handle_event(key(KeyCode::Char('g')));

        assert_eq!(editor.buffer.cursor_row(), 0);
        assert_eq!(editor.buffer.cursor_col(), 0);
    }

    #[test]
    fn test_uppercase_g_goes_to_buffer_end() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.buffer = EditorBuffer::with_text("first\nsecond\nthird");

        editor.handle_event(shift_key('G'));

        assert_eq!(editor.buffer.cursor_row(), 2);
    }

    #[test]
    fn test_undo_redo_through_editor() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.buffer = EditorBuffer::with_text("hello world");
        editor.buffer.set_cursor(0, 0);

        // dd to delete line
        editor.handle_event(key(KeyCode::Char('d')));
        editor.handle_event(key(KeyCode::Char('d')));
        assert_eq!(editor.buffer.contents(), "");

        // u to undo
        editor.handle_event(key(KeyCode::Char('u')));
        assert_eq!(editor.buffer.contents(), "hello world");

        // Ctrl+r to redo
        editor.handle_event(ctrl_key('r'));
        assert_eq!(editor.buffer.contents(), "");
    }

    #[test]
    fn test_o_opens_line_below_and_enters_insert() {
        let mut editor = InputEditor::with_mode(EditorMode::Normal);
        editor.buffer = EditorBuffer::with_text("hello");

        let action = editor.handle_event(key(KeyCode::Char('o')));
        assert!(matches!(action, EditorAction::ModeChanged(EditorMode::Insert)));
        assert_eq!(editor.buffer.line_count(), 2);
        assert_eq!(editor.buffer.cursor_row(), 1);
    }
}
