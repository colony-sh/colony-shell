use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::collections::HashSet;

pub struct ScriptPane {
    lines: Vec<String>,
    current_line: u32,
    marked_lines: HashSet<u32>,
    scroll_offset: u32,
}

impl ScriptPane {
    pub fn new(source: String) -> Self {
        let lines: Vec<String> = source.lines().map(String::from).collect();
        Self {
            lines,
            current_line: 1,
            marked_lines: HashSet::new(),
            scroll_offset: 0,
        }
    }

    pub fn current_line(&self) -> u32 {
        self.current_line
    }

    pub fn set_current_line(&mut self, line: u32) {
        self.current_line = line;
        self.ensure_line_visible(line);
    }

    pub fn mark_line(&mut self, line: u32) {
        self.marked_lines.insert(line);
    }

    pub fn unmark_line(&mut self, line: u32) {
        self.marked_lines.remove(&line);
    }

    pub fn is_line_marked(&self, line: u32) -> bool {
        self.marked_lines.contains(&line)
    }

    fn ensure_line_visible(&mut self, line: u32) {
        if line < self.scroll_offset + 1 {
            self.scroll_offset = line.saturating_sub(1);
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let inner_height = area.height.saturating_sub(2) as u32;

        if self.current_line > self.scroll_offset + inner_height {
            let mut scroll = self.scroll_offset;
            while self.current_line > scroll + inner_height {
                scroll += 1;
            }
        }

        let mut text_lines: Vec<Line> = Vec::new();

        for (i, line_content) in self.lines.iter().enumerate() {
            let line_num = (i + 1) as u32;

            if line_num <= self.scroll_offset {
                continue;
            }
            if line_num > self.scroll_offset + inner_height {
                break;
            }

            let is_current = line_num == self.current_line;
            let is_marked = self.marked_lines.contains(&line_num);

            let marker = if is_current {
                "▸"
            } else if is_marked {
                "●"
            } else {
                " "
            };

            let line_num_style = if is_current {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let content_style = if is_current {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else if is_marked {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            text_lines.push(Line::from(vec![
                Span::styled(
                    format!("{} {:3} ", marker, line_num),
                    line_num_style,
                ),
                Span::styled(line_content.clone(), content_style),
            ]));
        }

        let block = Block::default()
            .title(" Script ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue));

        let paragraph = Paragraph::new(text_lines).block(block);

        frame.render_widget(paragraph, area);
    }
}
