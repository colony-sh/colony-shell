//! Source pane -- displays script source with line numbers, current line
//! highlight, and breakpoint markers.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use std::collections::HashSet;

/// Left pane: script source with line markers.
pub struct SourcePane {
    lines: Vec<String>,
    current_line: u32,
    breakpoint_lines: HashSet<u32>,
    scroll_offset: u32,
}

impl SourcePane {
    /// Create a new source pane from script source text.
    pub fn new(source: &str) -> Self {
        Self {
            lines: source.lines().map(String::from).collect(),
            current_line: 0,
            breakpoint_lines: HashSet::new(),
            scroll_offset: 0,
        }
    }

    /// Update the current line and auto-scroll to keep it visible.
    pub fn set_current_line(&mut self, line: u32) {
        self.current_line = line;
        // Auto-scroll to keep current line visible
        let visible_height = 20u32; // approximate; updated during render
        if line > self.scroll_offset + visible_height.saturating_sub(3) {
            self.scroll_offset = line.saturating_sub(visible_height / 2);
        } else if line < self.scroll_offset + 2 {
            self.scroll_offset = line.saturating_sub(2);
        }
    }

    /// Set the breakpoint lines from the breakpoint manager.
    pub fn set_breakpoints(&mut self, lines: HashSet<u32>) {
        self.breakpoint_lines = lines;
    }

    /// Total number of source lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Render the source pane into the given area.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let visible_height = area.height.saturating_sub(2) as u32;
        let start = self.scroll_offset as usize;
        let end = (start + visible_height as usize).min(self.lines.len());

        let mut text_lines: Vec<Line> = Vec::with_capacity(end - start);

        for (idx, line_content) in self.lines[start..end].iter().enumerate() {
            let line_num = start + idx + 1;
            let is_current = line_num as u32 == self.current_line;
            let is_breakpoint = self.breakpoint_lines.contains(&(line_num as u32));

            let marker = if is_current && is_breakpoint {
                "B>"
            } else if is_current {
                " >"
            } else if is_breakpoint {
                "B "
            } else {
                "  "
            };

            let marker_style = if is_breakpoint {
                Style::default().fg(Color::Red)
            } else if is_current {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let num_style = Style::default().fg(Color::DarkGray);
            let content_style = if is_current {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            text_lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), marker_style),
                Span::styled(format!("{line_num:>4} "), num_style),
                Span::styled(line_content.as_str(), content_style),
            ]));
        }

        let block = Block::default()
            .title(" Source ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let paragraph = Paragraph::new(text_lines).block(block);
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_from_source() {
        let pane = SourcePane::new("echo hello\necho world");
        assert_eq!(pane.line_count(), 2);
    }

    #[test]
    fn set_current_line_updates() {
        let mut pane = SourcePane::new("line1\nline2\nline3");
        pane.set_current_line(2);
        assert_eq!(pane.current_line, 2);
    }

    #[test]
    fn auto_scroll_keeps_visible() {
        // With 20 visible lines, setting current to line 30 should scroll
        let source = (1..=50).map(|i| format!("echo {i}")).collect::<Vec<_>>().join("\n");
        let mut pane = SourcePane::new(&source);
        pane.set_current_line(30);
        assert!(pane.scroll_offset > 0);
    }

    #[test]
    fn set_breakpoints() {
        let mut pane = SourcePane::new("a\nb\nc");
        let mut bps = HashSet::new();
        bps.insert(2);
        pane.set_breakpoints(bps);
        assert!(pane.breakpoint_lines.contains(&2));
    }

    #[test]
    fn empty_source() {
        let pane = SourcePane::new("");
        // "".lines() produces zero elements
        assert_eq!(pane.line_count(), 0);
    }
}
