//! Interactive file/directory picker with fuzzy search.
//!
//! Scans the current working directory and provides fuzzy-filtered selection.
//! Follows the same popup pattern as HistorySearch and CommandPalette.

use ratatui::layout::Rect;
use ratatui::Frame;

use super::WidgetResult;

/// File picker widget state (stub -- implemented in Task 2).
#[derive(Debug, Clone)]
pub struct FilePickerWidget {
    query: String,
}

impl FilePickerWidget {
    /// Create a stub file picker.
    pub fn new() -> Self {
        Self {
            query: String::new(),
        }
    }

    /// Insert a character into the search query.
    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
    }

    /// Delete the last character from the search query.
    pub fn backspace(&mut self) {
        self.query.pop();
    }

    /// Move selection to the next entry.
    pub fn next(&mut self) {}

    /// Move selection to the previous entry.
    pub fn prev(&mut self) {}

    /// Accept the current selection.
    pub fn accept(&self) -> Option<WidgetResult> {
        None
    }

    /// Render the file picker popup (stub).
    pub fn render(&self, _frame: &mut Frame, _area: Rect) {}
}

impl Default for FilePickerWidget {
    fn default() -> Self {
        Self::new()
    }
}
