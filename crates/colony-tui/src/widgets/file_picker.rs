//! Interactive file/directory picker with fuzzy search.
//!
//! Scans the current working directory and provides fuzzy-filtered selection.
//! Follows the same popup pattern as HistorySearch and CommandPalette:
//! `active: bool`, `accept() -> Option<WidgetResult>`, ratatui `Clear` overlay.

use std::fs;
use std::path::PathBuf;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use super::WidgetResult;

/// A file/directory entry in the picker list.
#[derive(Debug, Clone)]
struct FileEntry {
    /// Display name (filename only).
    name: String,
    /// Full path.
    path: PathBuf,
    /// Whether this entry is a directory.
    is_dir: bool,
}

/// Interactive file/directory picker with fuzzy search.
#[derive(Debug, Clone)]
pub struct FilePickerWidget {
    /// Current search query for filtering.
    query: String,
    /// All entries in the current directory.
    entries: Vec<FileEntry>,
    /// Indices into `entries` that match the current query.
    filtered: Vec<usize>,
    /// Currently selected index within `filtered`.
    selected: usize,
    /// Working directory being browsed.
    cwd: PathBuf,
    /// Whether to show hidden files (dotfiles).
    show_hidden: bool,
}

impl FilePickerWidget {
    /// Create a new file picker rooted at `cwd`.
    pub fn new(cwd: PathBuf) -> Self {
        let mut picker = Self {
            query: String::new(),
            entries: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            cwd,
            show_hidden: false,
        };
        picker.scan_directory();
        picker.update_filter();
        picker
    }

    /// Insert a character into the search query.
    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filter();
        self.selected = 0;
    }

    /// Delete the last character from the search query.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_filter();
        self.selected = 0;
    }

    /// Move selection to the next entry (wraps around).
    pub fn next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    /// Move selection to the previous entry (wraps around).
    pub fn prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
        }
    }

    /// Accept the current selection, returning its path as a `WidgetResult`.
    pub fn accept(&self) -> Option<WidgetResult> {
        let idx = *self.filtered.get(self.selected)?;
        let entry = &self.entries[idx];
        Some(WidgetResult::Accept(
            entry.path.to_string_lossy().to_string(),
        ))
    }

    /// Render the file picker as a centered popup overlay.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let width = 60u16.min(area.width.saturating_sub(4));
        let height = 20u16.min(area.height.saturating_sub(4));
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(popup);

        // Search input
        let input = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::raw(&self.query),
            Span::styled(
                "_",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" Files: {} ", self.cwd.display())),
        );
        frame.render_widget(input, chunks[0]);

        // File list
        let visible_count = height.saturating_sub(4) as usize;
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .enumerate()
            .skip(self.scroll_offset(visible_count))
            .take(visible_count)
            .map(|(i, &idx)| {
                let entry = &self.entries[idx];
                let icon = if entry.is_dir { "/" } else { " " };
                let style = if i == self.selected {
                    Style::default().bg(Color::DarkGray).fg(Color::White).bold()
                } else if entry.is_dir {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default()
                };
                ListItem::new(format!(" {icon} {}", entry.name)).style(style)
            })
            .collect();

        let count_info = format!(" {}/{} ", self.filtered.len(), self.entries.len());
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(count_info),
        );
        frame.render_widget(list, chunks[1]);
    }

    /// Calculate scroll offset so the selected item is always visible.
    fn scroll_offset(&self, visible_count: usize) -> usize {
        if visible_count == 0 {
            return 0;
        }
        if self.selected >= visible_count {
            self.selected - visible_count + 1
        } else {
            0
        }
    }

    /// Scan the current directory and populate entries.
    fn scan_directory(&mut self) {
        self.entries.clear();

        // Parent directory entry
        if let Some(parent) = self.cwd.parent() {
            self.entries.push(FileEntry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
                is_dir: true,
            });
        }

        let Ok(read_dir) = fs::read_dir(&self.cwd) else {
            return;
        };

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !self.show_hidden && name.starts_with('.') {
                continue;
            }
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let fe = FileEntry {
                name,
                path: entry.path(),
                is_dir,
            };
            if is_dir {
                dirs.push(fe);
            } else {
                files.push(fe);
            }
        }

        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        files.sort_by(|a, b| a.name.cmp(&b.name));

        self.entries.extend(dirs);
        self.entries.extend(files);
    }

    /// Update the filtered index list based on the current query.
    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            let query_lower = self.query.to_lowercase();
            self.filtered = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.name.to_lowercase().contains(&query_lower))
                .map(|(i, _)| i)
                .collect();
        }
    }
}

impl Default for FilePickerWidget {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_file_picker_scans_temp_dir() {
        let dir = std::env::temp_dir().join("colony_test_file_picker");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("alpha.txt"), "").unwrap();
        fs::write(dir.join("beta.rs"), "").unwrap();
        fs::create_dir_all(dir.join("subdir")).unwrap();

        let picker = FilePickerWidget::new(dir.clone());
        // Should have "..", "subdir", "alpha.txt", "beta.rs"
        assert!(picker.entries.len() >= 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_picker_filter() {
        let dir = std::env::temp_dir().join("colony_test_file_picker_filter");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("alpha.txt"), "").unwrap();
        fs::write(dir.join("beta.rs"), "").unwrap();

        let mut picker = FilePickerWidget::new(dir.clone());
        picker.insert_char('a');
        picker.insert_char('l');

        // Only entries containing "al" should match
        assert!(picker
            .filtered
            .iter()
            .all(|&idx| { picker.entries[idx].name.to_lowercase().contains("al") }));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_picker_navigation() {
        let dir = std::env::temp_dir().join("colony_test_file_picker_nav");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "").unwrap();
        fs::write(dir.join("b.txt"), "").unwrap();

        let mut picker = FilePickerWidget::new(dir.clone());
        let count = picker.filtered.len();
        assert!(count >= 2);

        assert_eq!(picker.selected, 0);
        picker.next();
        assert_eq!(picker.selected, 1);
        picker.prev();
        assert_eq!(picker.selected, 0);

        // Wrap around backwards
        picker.prev();
        assert_eq!(picker.selected, count - 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_picker_accept() {
        let dir = std::env::temp_dir().join("colony_test_file_picker_accept");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("target.txt"), "").unwrap();

        let mut picker = FilePickerWidget::new(dir.clone());
        // Filter to just "target"
        picker.insert_char('t');
        picker.insert_char('a');
        picker.insert_char('r');

        let result = picker.accept();
        assert!(result.is_some());
        if let Some(WidgetResult::Accept(path)) = result {
            assert!(path.contains("target.txt"));
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_picker_backspace() {
        let dir = std::env::temp_dir().join("colony_test_file_picker_bs");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("alpha.txt"), "").unwrap();
        fs::write(dir.join("beta.rs"), "").unwrap();

        let mut picker = FilePickerWidget::new(dir.clone());
        let all_count = picker.filtered.len();

        picker.insert_char('a');
        picker.insert_char('l');
        let filtered_count = picker.filtered.len();
        assert!(filtered_count < all_count);

        picker.backspace();
        picker.backspace();
        assert_eq!(picker.filtered.len(), all_count);

        let _ = fs::remove_dir_all(&dir);
    }
}
