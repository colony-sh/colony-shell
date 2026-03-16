//! Git branch picker with fuzzy search.
//!
//! Reads branches via `git branch` and provides fuzzy-filtered selection.
//! Follows the same popup pattern as HistorySearch and CommandPalette:
//! `active: bool`, `accept() -> Option<WidgetResult>`, ratatui `Clear` overlay.

use std::process::Command;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use super::WidgetResult;

/// A git branch entry.
#[derive(Debug, Clone)]
struct BranchEntry {
    /// Branch name (e.g. "main", "feature/foo").
    name: String,
    /// Whether this is the currently checked-out branch.
    is_current: bool,
}

/// Interactive git branch picker with fuzzy search.
#[derive(Debug, Clone)]
pub struct BranchPickerWidget {
    /// Current search query for filtering.
    query: String,
    /// All branches.
    branches: Vec<BranchEntry>,
    /// Indices into `branches` that match the current query.
    filtered: Vec<usize>,
    /// Currently selected index within `filtered`.
    selected: usize,
    /// Name of the currently checked-out branch (if known).
    current_branch: Option<String>,
}

impl BranchPickerWidget {
    /// Create a new branch picker, reading branches from `git branch`.
    pub fn new() -> Self {
        let mut picker = Self {
            query: String::new(),
            branches: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            current_branch: None,
        };
        picker.load_branches();
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

    /// Accept the current selection, returning the branch name.
    pub fn accept(&self) -> Option<WidgetResult> {
        let idx = *self.filtered.get(self.selected)?;
        let branch = &self.branches[idx];
        Some(WidgetResult::Accept(branch.name.clone()))
    }

    /// Render the branch picker as a centered popup overlay.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let width = 50u16.min(area.width.saturating_sub(4));
        let height = 18u16.min(area.height.saturating_sub(4));
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
            Span::styled("> ", Style::default().fg(Color::Magenta)),
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
                .border_style(Style::default().fg(Color::Magenta))
                .title(" Git Branches "),
        );
        frame.render_widget(input, chunks[0]);

        // Branch list
        let visible_count = height.saturating_sub(4) as usize;
        let scroll_offset = self.scroll_offset(visible_count);
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_count)
            .map(|(i, &idx)| {
                let branch = &self.branches[idx];
                let marker = if branch.is_current { "* " } else { "  " };
                let style = if i == self.selected {
                    Style::default().bg(Color::DarkGray).fg(Color::White).bold()
                } else if branch.is_current {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                ListItem::new(format!(" {marker}{}", branch.name)).style(style)
            })
            .collect();

        let count_info = format!(
            " {}/{} ",
            self.filtered.len(),
            self.branches.len()
        );
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

    /// Load branches from `git branch`.
    fn load_branches(&mut self) {
        self.branches.clear();
        self.current_branch = None;

        let Ok(output) = Command::new("git")
            .args(["branch", "--format=%(HEAD) %(refname:short)"])
            .output()
        else {
            return;
        };

        if !output.status.success() {
            return;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let is_current = line.starts_with("* ");
            let name = if is_current {
                line.strip_prefix("* ").unwrap_or(line)
            } else {
                line.strip_prefix("  ").unwrap_or(line)
            };

            let name = name.to_string();
            if is_current {
                self.current_branch = Some(name.clone());
            }

            self.branches.push(BranchEntry { name, is_current });
        }
    }

    /// Update the filtered index list based on the current query.
    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.branches.len()).collect();
        } else {
            let query_lower = self.query.to_lowercase();
            self.filtered = self
                .branches
                .iter()
                .enumerate()
                .filter(|(_, b)| b.name.to_lowercase().contains(&query_lower))
                .map(|(i, _)| i)
                .collect();
        }
    }
}

impl Default for BranchPickerWidget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_picker_default() {
        // In a test environment we may or may not be in a git repo,
        // so just verify the struct initializes without panicking.
        let picker = BranchPickerWidget::new();
        assert_eq!(picker.query, "");
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn test_branch_picker_insert_char() {
        let mut picker = BranchPickerWidget::new();
        picker.insert_char('m');
        assert_eq!(picker.query, "m");
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn test_branch_picker_backspace() {
        let mut picker = BranchPickerWidget::new();
        picker.insert_char('m');
        picker.insert_char('a');
        picker.backspace();
        assert_eq!(picker.query, "m");
    }

    #[test]
    fn test_branch_picker_navigation_empty() {
        let mut picker = BranchPickerWidget {
            query: String::new(),
            branches: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            current_branch: None,
        };
        // Should not panic on empty
        picker.next();
        picker.prev();
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn test_branch_picker_accept_empty() {
        let picker = BranchPickerWidget {
            query: String::new(),
            branches: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            current_branch: None,
        };
        assert_eq!(picker.accept(), None);
    }
}
