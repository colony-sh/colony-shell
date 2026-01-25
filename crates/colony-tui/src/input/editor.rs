//! Text editor buffer for shell input

/// Editor buffer for shell input
#[derive(Debug, Clone)]
pub struct Editor {
    /// Current text content
    buffer: String,
    /// Cursor position (byte offset)
    cursor: usize,
    /// History of commands
    history: Vec<String>,
    /// Current position in history (None = editing new command)
    history_pos: Option<usize>,
    /// Saved buffer when navigating history
    saved_buffer: Option<String>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_pos: None,
            saved_buffer: None,
        }
    }

    /// Get the current buffer content
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// Get cursor position (byte offset)
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Get cursor position as (line, column) for multi-line
    pub fn cursor_pos(&self) -> (usize, usize) {
        let before_cursor = &self.buffer[..self.cursor];
        let line = before_cursor.matches('\n').count();
        let last_newline = before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = self.cursor - last_newline;
        (line, col)
    }

    /// Insert a character at cursor
    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Insert a string at cursor
    pub fn insert_str(&mut self, s: &str) {
        self.buffer.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete character at cursor
    pub fn delete_char(&mut self) {
        if self.cursor < self.buffer.len() {
            let next_char_boundary = self.buffer[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.buffer.len());
            self.buffer.drain(self.cursor..next_char_boundary);
        }
    }

    /// Delete character before cursor (backspace)
    pub fn delete_char_backward(&mut self) {
        if self.cursor > 0 {
            let prev_char_boundary = self.buffer[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.buffer.drain(prev_char_boundary..self.cursor);
            self.cursor = prev_char_boundary;
        }
    }

    /// Delete word backward
    pub fn delete_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }

        // Skip trailing whitespace
        let mut pos = self.cursor;
        while pos > 0 && self.buffer[..pos].ends_with(char::is_whitespace) {
            pos = self.buffer[..pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }

        // Delete word characters
        while pos > 0 {
            let prev = self.buffer[..pos]
                .char_indices()
                .last()
                .map(|(i, c)| (i, c));
            match prev {
                Some((i, c)) if !c.is_whitespace() => pos = i,
                _ => break,
            }
        }

        self.buffer.drain(pos..self.cursor);
        self.cursor = pos;
    }

    /// Delete to line start
    pub fn delete_to_line_start(&mut self) {
        let line_start = self.buffer[..self.cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        self.buffer.drain(line_start..self.cursor);
        self.cursor = line_start;
    }

    /// Delete to line end
    pub fn delete_to_line_end(&mut self) {
        let line_end = self.buffer[self.cursor..]
            .find('\n')
            .map(|i| self.cursor + i)
            .unwrap_or(self.buffer.len());
        self.buffer.drain(self.cursor..line_end);
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.buffer[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor = self.buffer[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.buffer.len());
        }
    }

    /// Move cursor to line start
    pub fn cursor_line_start(&mut self) {
        self.cursor = self.buffer[..self.cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
    }

    /// Move cursor to line end
    pub fn cursor_line_end(&mut self) {
        self.cursor = self.buffer[self.cursor..]
            .find('\n')
            .map(|i| self.cursor + i)
            .unwrap_or(self.buffer.len());
    }

    /// Move cursor forward one word
    pub fn cursor_word_forward(&mut self) {
        // Skip current word
        while self.cursor < self.buffer.len() {
            let c = self.buffer[self.cursor..].chars().next().unwrap();
            if c.is_whitespace() {
                break;
            }
            self.cursor += c.len_utf8();
        }
        // Skip whitespace
        while self.cursor < self.buffer.len() {
            let c = self.buffer[self.cursor..].chars().next().unwrap();
            if !c.is_whitespace() {
                break;
            }
            self.cursor += c.len_utf8();
        }
    }

    /// Move cursor backward one word
    pub fn cursor_word_backward(&mut self) {
        // Skip whitespace
        while self.cursor > 0 {
            let c = self.buffer[..self.cursor].chars().last().unwrap();
            if !c.is_whitespace() {
                break;
            }
            self.cursor -= c.len_utf8();
        }
        // Skip word
        while self.cursor > 0 {
            let c = self.buffer[..self.cursor].chars().last().unwrap();
            if c.is_whitespace() {
                break;
            }
            self.cursor -= c.len_utf8();
        }
    }

    /// Move cursor to end of word
    pub fn cursor_word_end(&mut self) {
        // Skip current char
        if self.cursor < self.buffer.len() {
            self.cursor += self.buffer[self.cursor..]
                .chars()
                .next()
                .unwrap()
                .len_utf8();
        }
        // Skip whitespace
        while self.cursor < self.buffer.len() {
            let c = self.buffer[self.cursor..].chars().next().unwrap();
            if !c.is_whitespace() {
                break;
            }
            self.cursor += c.len_utf8();
        }
        // Go to end of word
        while self.cursor < self.buffer.len() {
            let next = self.buffer[self.cursor..].chars().next().unwrap();
            if next.is_whitespace() {
                break;
            }
            self.cursor += next.len_utf8();
        }
        // Back one char (vim behavior)
        if self.cursor > 0 {
            self.cursor -= self.buffer[..self.cursor]
                .chars()
                .last()
                .unwrap()
                .len_utf8();
        }
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    /// Set the buffer content
    pub fn set_buffer(&mut self, content: &str) {
        self.buffer = content.to_string();
        self.cursor = self.buffer.len();
    }

    /// Take the buffer content and reset
    pub fn take(&mut self) -> String {
        let content = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        self.history_pos = None;
        self.saved_buffer = None;
        content
    }

    /// Add command to history
    pub fn add_to_history(&mut self, cmd: &str) {
        if !cmd.trim().is_empty() {
            // Don't add duplicates of the last command
            if self.history.last().map(|s| s.as_str()) != Some(cmd) {
                self.history.push(cmd.to_string());
            }
        }
    }

    /// Navigate to previous history entry
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_pos {
            None => {
                // Save current buffer and go to last history entry
                self.saved_buffer = Some(self.buffer.clone());
                self.history_pos = Some(self.history.len() - 1);
                self.buffer = self.history[self.history.len() - 1].clone();
            }
            Some(0) => {
                // Already at oldest entry
            }
            Some(pos) => {
                self.history_pos = Some(pos - 1);
                self.buffer = self.history[pos - 1].clone();
            }
        }
        self.cursor = self.buffer.len();
    }

    /// Navigate to next history entry
    pub fn history_next(&mut self) {
        match self.history_pos {
            None => {
                // Not in history
            }
            Some(pos) if pos >= self.history.len() - 1 => {
                // Go back to saved buffer
                self.history_pos = None;
                self.buffer = self.saved_buffer.take().unwrap_or_default();
            }
            Some(pos) => {
                self.history_pos = Some(pos + 1);
                self.buffer = self.history[pos + 1].clone();
            }
        }
        self.cursor = self.buffer.len();
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get number of lines
    pub fn line_count(&self) -> usize {
        self.buffer.matches('\n').count() + 1
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_char() {
        let mut editor = Editor::new();
        editor.insert_char('h');
        editor.insert_char('i');
        assert_eq!(editor.buffer(), "hi");
        assert_eq!(editor.cursor(), 2);
    }

    #[test]
    fn test_delete_backward() {
        let mut editor = Editor::new();
        editor.set_buffer("hello");
        editor.delete_char_backward();
        assert_eq!(editor.buffer(), "hell");
    }

    #[test]
    fn test_cursor_movement() {
        let mut editor = Editor::new();
        editor.set_buffer("hello world");
        editor.cursor_line_start();
        assert_eq!(editor.cursor(), 0);
        editor.cursor_word_forward();
        assert_eq!(editor.cursor(), 6); // After "hello "
    }

    #[test]
    fn test_history() {
        let mut editor = Editor::new();
        editor.add_to_history("ls");
        editor.add_to_history("pwd");
        editor.add_to_history("echo hi");

        editor.set_buffer("new");
        editor.history_prev();
        assert_eq!(editor.buffer(), "echo hi");
        editor.history_prev();
        assert_eq!(editor.buffer(), "pwd");
        editor.history_next();
        assert_eq!(editor.buffer(), "echo hi");
        editor.history_next();
        assert_eq!(editor.buffer(), "new");
    }
}
