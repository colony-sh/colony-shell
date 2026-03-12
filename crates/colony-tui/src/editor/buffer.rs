//! Text buffer with cursor management and undo/redo.
//!
//! `EditorBuffer` stores multi-line text with a cursor position, scroll offset,
//! and a snapshot-based undo/redo stack.

use std::ops::Range;

/// A snapshot of the buffer state for undo/redo.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferSnapshot {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

/// Multi-line text buffer with cursor tracking and undo/redo support.
#[derive(Debug, Clone)]
pub struct EditorBuffer {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_offset: usize,
    /// Clipboard for yank/paste operations.
    clipboard: Clipboard,
    /// Undo stack (snapshots taken before significant edits).
    undo_stack: Vec<BufferSnapshot>,
    /// Redo stack (snapshots pushed when undoing).
    redo_stack: Vec<BufferSnapshot>,
}

/// Clipboard content for yank/paste.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
pub enum Clipboard {
    #[default]
    Empty,
    Line(String),
    Text(String),
}


impl Default for EditorBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorBuffer {
    /// Create a new empty buffer with a single empty line.
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            clipboard: Clipboard::Empty,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Create a buffer pre-populated with the given text.
    pub fn with_text(text: &str) -> Self {
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.lines().map(String::from).collect()
        };
        Self {
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            clipboard: Clipboard::Empty,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    // ── Accessors ─────────────────────────────────────────────

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn cursor_row(&self) -> usize {
        self.cursor_row
    }

    pub fn cursor_col(&self) -> usize {
        self.cursor_col
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    pub fn clipboard(&self) -> &Clipboard {
        &self.clipboard
    }

    /// Get the current line the cursor is on.
    pub fn current_line(&self) -> &str {
        &self.lines[self.cursor_row]
    }

    /// Get the full buffer contents as a single string with newline separators.
    pub fn contents(&self) -> String {
        self.lines.join("\n")
    }

    /// Returns true if the buffer contains only whitespace.
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.is_empty())
    }

    /// Replace entire buffer contents and reset cursor.
    pub fn set_contents(&mut self, text: &str) {
        self.save_snapshot();
        self.lines = if text.is_empty() {
            vec![String::new()]
        } else {
            text.lines().map(String::from).collect()
        };
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    /// Clear the buffer entirely.
    pub fn clear(&mut self) {
        self.save_snapshot();
        self.lines = vec![String::new()];
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    // ── Cursor movement ───────────────────────────────────────

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    pub fn move_right(&mut self) {
        let line_len = self.lines[self.cursor_row].len();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_cursor_col();
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.clamp_cursor_col();
        }
    }

    pub fn move_to_line_start(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_to_line_end(&mut self) {
        self.cursor_col = self.lines[self.cursor_row].len();
    }

    pub fn move_to_first_non_blank(&mut self) {
        let line = &self.lines[self.cursor_row];
        self.cursor_col = line
            .chars()
            .position(|c| !c.is_whitespace())
            .unwrap_or(0);
    }

    pub fn move_to_buffer_start(&mut self) {
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    pub fn move_to_buffer_end(&mut self) {
        self.cursor_row = self.lines.len().saturating_sub(1);
        self.cursor_col = self.lines[self.cursor_row].len();
    }

    // ── Word motions ──────────────────────────────────────────

    /// Move cursor to the start of the next word (vim `w`).
    pub fn move_word_forward(&mut self) {
        let line = &self.lines[self.cursor_row];
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        if self.cursor_col >= len {
            // Move to next line if available
            if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                self.cursor_col = 0;
                // Skip to first non-blank on new line
                let new_line = &self.lines[self.cursor_row];
                self.cursor_col = new_line
                    .chars()
                    .position(|c| !c.is_whitespace())
                    .unwrap_or(0);
            }
            return;
        }

        let mut col = self.cursor_col;
        let start_class = char_class(chars[col]);

        // Skip current word (same char class)
        while col < len && char_class(chars[col]) == start_class {
            col += 1;
        }

        // Skip whitespace
        while col < len && chars[col].is_whitespace() {
            col += 1;
        }

        if col >= len {
            // Wrap to next line
            if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                let new_line = &self.lines[self.cursor_row];
                self.cursor_col = new_line
                    .chars()
                    .position(|c| !c.is_whitespace())
                    .unwrap_or(0);
            } else {
                self.cursor_col = len.saturating_sub(1).max(0);
            }
        } else {
            self.cursor_col = col;
        }
    }

    /// Move cursor to the start of the previous word (vim `b`).
    pub fn move_word_backward(&mut self) {
        if self.cursor_col == 0 {
            if self.cursor_row > 0 {
                self.cursor_row -= 1;
                self.cursor_col = self.lines[self.cursor_row].len().saturating_sub(1);
            }
            return;
        }

        let line = &self.lines[self.cursor_row];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col.min(chars.len()).saturating_sub(1);

        // Skip whitespace backward
        while col > 0 && chars[col].is_whitespace() {
            col -= 1;
        }

        let target_class = char_class(chars[col]);

        // Skip same char class backward
        while col > 0 && char_class(chars[col - 1]) == target_class {
            col -= 1;
        }

        self.cursor_col = col;
    }

    /// Move cursor to the end of the current/next word (vim `e`).
    pub fn move_word_end(&mut self) {
        let line = &self.lines[self.cursor_row];
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        if len == 0 || self.cursor_col >= len.saturating_sub(1) {
            if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                let new_line = &self.lines[self.cursor_row];
                let new_chars: Vec<char> = new_line.chars().collect();
                let mut col = 0;
                // Skip whitespace
                while col < new_chars.len() && new_chars[col].is_whitespace() {
                    col += 1;
                }
                if col < new_chars.len() {
                    let target_class = char_class(new_chars[col]);
                    while col + 1 < new_chars.len()
                        && char_class(new_chars[col + 1]) == target_class
                    {
                        col += 1;
                    }
                }
                self.cursor_col = col;
            }
            return;
        }

        let mut col = self.cursor_col + 1;

        // Skip whitespace
        while col < len && chars[col].is_whitespace() {
            col += 1;
        }

        if col >= len {
            self.cursor_col = len.saturating_sub(1);
            return;
        }

        let target_class = char_class(chars[col]);
        while col + 1 < len && char_class(chars[col + 1]) == target_class {
            col += 1;
        }

        self.cursor_col = col;
    }

    /// Find the range of the "inner word" under the cursor (for `ciw`/`diw`).
    pub fn inner_word_range(&self) -> Range<usize> {
        let line = &self.lines[self.cursor_row];
        let chars: Vec<char> = line.chars().collect();

        if chars.is_empty() {
            return 0..0;
        }

        let col = self.cursor_col.min(chars.len().saturating_sub(1));
        let target_class = char_class(chars[col]);

        let mut start = col;
        while start > 0 && char_class(chars[start - 1]) == target_class {
            start -= 1;
        }

        let mut end = col;
        while end + 1 < chars.len() && char_class(chars[end + 1]) == target_class {
            end += 1;
        }

        start..(end + 1)
    }

    // ── Editing operations ────────────────────────────────────

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, ch: char) {
        let line = &mut self.lines[self.cursor_row];
        // Convert col to byte index
        let byte_idx = char_col_to_byte(line, self.cursor_col);
        line.insert(byte_idx, ch);
        self.cursor_col += 1;
        self.redo_stack.clear();
    }

    /// Delete the character before the cursor (backspace).
    /// Returns true if a character was deleted.
    pub fn delete_char_backward(&mut self) -> bool {
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let byte_idx = char_col_to_byte(line, self.cursor_col - 1);
            let next_byte = char_col_to_byte(line, self.cursor_col);
            line.drain(byte_idx..next_byte);
            self.cursor_col -= 1;
            true
        } else if self.cursor_row > 0 {
            // Join with previous line
            let current_line = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
            self.lines[self.cursor_row].push_str(&current_line);
            true
        } else {
            false
        }
    }

    /// Delete the character at the cursor position (vim `x`, Delete key).
    /// Returns true if a character was deleted.
    pub fn delete_char_forward(&mut self) -> bool {
        let line = &self.lines[self.cursor_row];
        let char_len = line.chars().count();

        if self.cursor_col < char_len {
            let line = &mut self.lines[self.cursor_row];
            let byte_idx = char_col_to_byte(line, self.cursor_col);
            let next_byte = char_col_to_byte(line, self.cursor_col + 1);
            line.drain(byte_idx..next_byte);
            self.clamp_cursor_col();
            true
        } else if self.cursor_row + 1 < self.lines.len() {
            // Join with next line
            let next_line = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next_line);
            true
        } else {
            false
        }
    }

    /// Insert a newline at the cursor, splitting the current line.
    pub fn insert_newline(&mut self) {
        let line = &self.lines[self.cursor_row];
        let byte_idx = char_col_to_byte(line, self.cursor_col);
        let rest = self.lines[self.cursor_row][byte_idx..].to_string();
        self.lines[self.cursor_row].truncate(byte_idx);
        self.cursor_row += 1;
        self.lines.insert(self.cursor_row, rest);
        self.cursor_col = 0;
    }

    /// Delete the entire current line (vim `dd`). Returns the deleted line.
    pub fn delete_line(&mut self) -> String {
        self.save_snapshot();
        let deleted = if self.lines.len() == 1 {
            let line = std::mem::take(&mut self.lines[0]);
            self.cursor_col = 0;
            line
        } else {
            let line = self.lines.remove(self.cursor_row);
            if self.cursor_row >= self.lines.len() {
                self.cursor_row = self.lines.len().saturating_sub(1);
            }
            self.clamp_cursor_col();
            line
        };
        self.clipboard = Clipboard::Line(deleted.clone());
        self.redo_stack.clear();
        deleted
    }

    /// Delete the current line contents and enter insert mode position (vim `cc`).
    /// Returns the deleted text.
    pub fn change_line(&mut self) -> String {
        self.save_snapshot();
        let deleted = std::mem::take(&mut self.lines[self.cursor_row]);
        self.cursor_col = 0;
        self.redo_stack.clear();
        deleted
    }

    /// Delete the inner word under the cursor (vim `diw`). Returns the deleted text.
    pub fn delete_inner_word(&mut self) -> String {
        self.save_snapshot();
        let range = self.inner_word_range();
        let line = &self.lines[self.cursor_row];
        let byte_start = char_col_to_byte(line, range.start);
        let byte_end = char_col_to_byte(line, range.end);
        let deleted = self.lines[self.cursor_row][byte_start..byte_end].to_string();
        self.lines[self.cursor_row].drain(byte_start..byte_end);
        self.cursor_col = range.start;
        self.clamp_cursor_col();
        self.clipboard = Clipboard::Text(deleted.clone());
        self.redo_stack.clear();
        deleted
    }

    /// Change the inner word under the cursor (vim `ciw`). Returns the deleted text.
    /// Cursor is positioned for insert mode.
    pub fn change_inner_word(&mut self) -> String {
        self.save_snapshot();
        let range = self.inner_word_range();
        let line = &self.lines[self.cursor_row];
        let byte_start = char_col_to_byte(line, range.start);
        let byte_end = char_col_to_byte(line, range.end);
        let deleted = self.lines[self.cursor_row][byte_start..byte_end].to_string();
        self.lines[self.cursor_row].drain(byte_start..byte_end);
        self.cursor_col = range.start;
        self.redo_stack.clear();
        deleted
    }

    /// Delete from cursor to the start of the line (Ctrl+u in insert mode).
    pub fn delete_to_line_start(&mut self) {
        let line = &self.lines[self.cursor_row];
        let byte_idx = char_col_to_byte(line, self.cursor_col);
        self.lines[self.cursor_row].drain(..byte_idx);
        self.cursor_col = 0;
    }

    /// Delete the word before the cursor (Ctrl+w in insert mode).
    pub fn delete_word_backward(&mut self) {
        if self.cursor_col == 0 {
            return;
        }

        let line = &self.lines[self.cursor_row];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col;

        // Skip whitespace backward
        while col > 0 && chars[col - 1].is_whitespace() {
            col -= 1;
        }

        // Skip word characters backward
        if col > 0 {
            let target_class = char_class(chars[col - 1]);
            while col > 0 && char_class(chars[col - 1]) == target_class {
                col -= 1;
            }
        }

        let byte_start = char_col_to_byte(&self.lines[self.cursor_row], col);
        let byte_end = char_col_to_byte(&self.lines[self.cursor_row], self.cursor_col);
        self.lines[self.cursor_row].drain(byte_start..byte_end);
        self.cursor_col = col;
    }

    /// Open a new line below the current one (vim `o`).
    pub fn open_line_below(&mut self) {
        self.save_snapshot();
        self.cursor_row += 1;
        self.lines.insert(self.cursor_row, String::new());
        self.cursor_col = 0;
        self.redo_stack.clear();
    }

    /// Open a new line above the current one (vim `O`).
    pub fn open_line_above(&mut self) {
        self.save_snapshot();
        self.lines.insert(self.cursor_row, String::new());
        self.cursor_col = 0;
        self.redo_stack.clear();
    }

    /// Paste after cursor (vim `p`).
    pub fn paste_after(&mut self) {
        self.save_snapshot();
        match &self.clipboard.clone() {
            Clipboard::Empty => {}
            Clipboard::Line(line) => {
                self.cursor_row += 1;
                self.lines.insert(self.cursor_row, line.clone());
                self.cursor_col = 0;
            }
            Clipboard::Text(text) => {
                let line = &mut self.lines[self.cursor_row];
                let byte_idx = char_col_to_byte(line, self.cursor_col + 1);
                line.insert_str(byte_idx.min(line.len()), text);
                self.cursor_col += 1;
            }
        }
        self.redo_stack.clear();
    }

    /// Paste before cursor (vim `P`).
    pub fn paste_before(&mut self) {
        self.save_snapshot();
        match &self.clipboard.clone() {
            Clipboard::Empty => {}
            Clipboard::Line(line) => {
                self.lines.insert(self.cursor_row, line.clone());
                self.cursor_col = 0;
            }
            Clipboard::Text(text) => {
                let line = &mut self.lines[self.cursor_row];
                let byte_idx = char_col_to_byte(line, self.cursor_col);
                line.insert_str(byte_idx, text);
            }
        }
        self.redo_stack.clear();
    }

    // ── Undo / Redo ──────────────────────────────────────────

    /// Save a snapshot of the current state for undo.
    pub fn save_snapshot(&mut self) {
        self.undo_stack.push(BufferSnapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });
    }

    /// Undo the last significant edit.
    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo_stack.pop() else {
            return false;
        };

        self.redo_stack.push(BufferSnapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });

        self.lines = snapshot.lines;
        self.cursor_row = snapshot.cursor_row;
        self.cursor_col = snapshot.cursor_col;
        true
    }

    /// Redo a previously undone edit.
    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo_stack.pop() else {
            return false;
        };

        self.undo_stack.push(BufferSnapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });

        self.lines = snapshot.lines;
        self.cursor_row = snapshot.cursor_row;
        self.cursor_col = snapshot.cursor_col;
        true
    }

    // ── Helpers ───────────────────────────────────────────────

    /// Clamp cursor column to valid range for the current line.
    fn clamp_cursor_col(&mut self) {
        let max_col = self.lines[self.cursor_row].chars().count();
        if self.cursor_col > max_col {
            self.cursor_col = max_col;
        }
    }

    /// Set cursor position directly (used by history navigation, etc.).
    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(self.lines.len().saturating_sub(1));
        self.clamp_cursor_col();
        self.cursor_col = col;
        self.clamp_cursor_col();
    }
}

// ── Character classification for word motions ───────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Word,
    Punctuation,
    Whitespace,
}

fn char_class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

/// Convert a character-column index to a byte index in a string.
fn char_col_to_byte(s: &str, col: usize) -> usize {
    s.char_indices()
        .nth(col)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer_is_empty() {
        let buf = EditorBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.cursor_row(), 0);
        assert_eq!(buf.cursor_col(), 0);
    }

    #[test]
    fn test_with_text() {
        let buf = EditorBuffer::with_text("hello\nworld");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.lines()[0], "hello");
        assert_eq!(buf.lines()[1], "world");
    }

    #[test]
    fn test_insert_char() {
        let mut buf = EditorBuffer::new();
        buf.insert_char('h');
        buf.insert_char('i');
        assert_eq!(buf.contents(), "hi");
        assert_eq!(buf.cursor_col(), 2);
    }

    #[test]
    fn test_delete_char_backward() {
        let mut buf = EditorBuffer::with_text("hello");
        buf.set_cursor(0, 5);
        assert!(buf.delete_char_backward());
        assert_eq!(buf.contents(), "hell");
        assert_eq!(buf.cursor_col(), 4);
    }

    #[test]
    fn test_delete_char_forward() {
        let mut buf = EditorBuffer::with_text("hello");
        buf.set_cursor(0, 0);
        assert!(buf.delete_char_forward());
        assert_eq!(buf.contents(), "ello");
    }

    #[test]
    fn test_cursor_movement() {
        let mut buf = EditorBuffer::with_text("hello\nworld");
        buf.set_cursor(0, 2);

        buf.move_left();
        assert_eq!(buf.cursor_col(), 1);

        buf.move_right();
        assert_eq!(buf.cursor_col(), 2);

        buf.move_down();
        assert_eq!(buf.cursor_row(), 1);
        assert_eq!(buf.cursor_col(), 2);

        buf.move_up();
        assert_eq!(buf.cursor_row(), 0);
    }

    #[test]
    fn test_line_start_end() {
        let mut buf = EditorBuffer::with_text("  hello world");
        buf.set_cursor(0, 5);

        buf.move_to_line_start();
        assert_eq!(buf.cursor_col(), 0);

        buf.move_to_line_end();
        assert_eq!(buf.cursor_col(), 13);

        buf.move_to_first_non_blank();
        assert_eq!(buf.cursor_col(), 2);
    }

    #[test]
    fn test_word_forward() {
        let mut buf = EditorBuffer::with_text("hello world foo");
        buf.set_cursor(0, 0);

        buf.move_word_forward();
        assert_eq!(buf.cursor_col(), 6); // start of "world"

        buf.move_word_forward();
        assert_eq!(buf.cursor_col(), 12); // start of "foo"
    }

    #[test]
    fn test_word_backward() {
        let mut buf = EditorBuffer::with_text("hello world");
        buf.set_cursor(0, 8);

        buf.move_word_backward();
        assert_eq!(buf.cursor_col(), 6); // start of "world"

        buf.move_word_backward();
        assert_eq!(buf.cursor_col(), 0); // start of "hello"
    }

    #[test]
    fn test_word_end() {
        let mut buf = EditorBuffer::with_text("hello world");
        buf.set_cursor(0, 0);

        buf.move_word_end();
        assert_eq!(buf.cursor_col(), 4); // end of "hello"
    }

    #[test]
    fn test_inner_word_range() {
        let buf = EditorBuffer::with_text("hello world");
        // Cursor on 'w' (col 6)
        let mut buf_mut = buf;
        buf_mut.set_cursor(0, 6);
        let range = buf_mut.inner_word_range();
        assert_eq!(range, 6..11);
    }

    #[test]
    fn test_delete_line() {
        let mut buf = EditorBuffer::with_text("first\nsecond\nthird");
        buf.set_cursor(1, 0);
        let deleted = buf.delete_line();
        assert_eq!(deleted, "second");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.lines()[0], "first");
        assert_eq!(buf.lines()[1], "third");
    }

    #[test]
    fn test_change_line() {
        let mut buf = EditorBuffer::with_text("hello world");
        buf.set_cursor(0, 3);
        let deleted = buf.change_line();
        assert_eq!(deleted, "hello world");
        assert_eq!(buf.current_line(), "");
        assert_eq!(buf.cursor_col(), 0);
    }

    #[test]
    fn test_delete_inner_word() {
        let mut buf = EditorBuffer::with_text("hello world");
        buf.set_cursor(0, 7); // on 'r' in "world"
        let deleted = buf.delete_inner_word();
        assert_eq!(deleted, "world");
        assert_eq!(buf.contents(), "hello ");
    }

    #[test]
    fn test_insert_newline() {
        let mut buf = EditorBuffer::with_text("helloworld");
        buf.set_cursor(0, 5);
        buf.insert_newline();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.lines()[0], "hello");
        assert_eq!(buf.lines()[1], "world");
        assert_eq!(buf.cursor_row(), 1);
        assert_eq!(buf.cursor_col(), 0);
    }

    #[test]
    fn test_open_line_below() {
        let mut buf = EditorBuffer::with_text("hello");
        buf.open_line_below();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.cursor_row(), 1);
        assert_eq!(buf.current_line(), "");
    }

    #[test]
    fn test_open_line_above() {
        let mut buf = EditorBuffer::with_text("hello");
        buf.open_line_above();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.cursor_row(), 0);
        assert_eq!(buf.current_line(), "");
    }

    #[test]
    fn test_undo_redo() {
        let mut buf = EditorBuffer::new();
        buf.save_snapshot();
        buf.insert_char('a');
        buf.insert_char('b');
        buf.insert_char('c');
        assert_eq!(buf.contents(), "abc");

        assert!(buf.undo());
        assert_eq!(buf.contents(), "");

        assert!(buf.redo());
        assert_eq!(buf.contents(), "abc");
    }

    #[test]
    fn test_delete_word_backward() {
        let mut buf = EditorBuffer::with_text("hello world");
        buf.set_cursor(0, 11);
        buf.delete_word_backward();
        assert_eq!(buf.contents(), "hello ");
    }

    #[test]
    fn test_delete_to_line_start() {
        let mut buf = EditorBuffer::with_text("hello world");
        buf.set_cursor(0, 6);
        buf.delete_to_line_start();
        assert_eq!(buf.contents(), "world");
        assert_eq!(buf.cursor_col(), 0);
    }

    #[test]
    fn test_paste_line() {
        let mut buf = EditorBuffer::with_text("first\nthird");
        buf.set_cursor(0, 0);
        buf.delete_line(); // deletes "first", clipboard = Line("first")
        // Now buffer is just "third" and clipboard has "first"
        buf.paste_after();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.lines()[1], "first");
    }

    #[test]
    fn test_buffer_start_end() {
        let mut buf = EditorBuffer::with_text("first\nsecond\nthird");
        buf.move_to_buffer_end();
        assert_eq!(buf.cursor_row(), 2);

        buf.move_to_buffer_start();
        assert_eq!(buf.cursor_row(), 0);
        assert_eq!(buf.cursor_col(), 0);
    }

    #[test]
    fn test_set_contents() {
        let mut buf = EditorBuffer::with_text("old");
        buf.set_contents("new\ntext");
        assert_eq!(buf.contents(), "new\ntext");
        assert_eq!(buf.cursor_row(), 0);
        assert_eq!(buf.cursor_col(), 0);
    }

    #[test]
    fn test_char_class() {
        assert_eq!(char_class('a'), CharClass::Word);
        assert_eq!(char_class('_'), CharClass::Word);
        assert_eq!(char_class('5'), CharClass::Word);
        assert_eq!(char_class(' '), CharClass::Whitespace);
        assert_eq!(char_class('.'), CharClass::Punctuation);
        assert_eq!(char_class('+'), CharClass::Punctuation);
    }
}
