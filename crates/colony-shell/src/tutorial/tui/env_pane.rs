use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::collections::{HashMap, HashSet};

pub struct EnvPane {
    variables: HashMap<String, String>,
    changed: HashSet<String>,
    scroll_offset: u16,
    total_lines: u16,
    viewport_height: u16,
}

impl EnvPane {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            changed: HashSet::new(),
            scroll_offset: 0,
            total_lines: 0,
            viewport_height: 0,
        }
    }

    pub fn update(&mut self, variables: HashMap<String, String>, changed: Vec<String>) {
        self.variables = variables;
        self.changed = changed.into_iter().collect();
    }

    pub fn clear_changed(&mut self) {
        self.changed.clear();
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        let max_scroll = self.total_lines.saturating_sub(self.viewport_height);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        let mut sorted_vars: Vec<_> = self.variables.iter().collect();
        sorted_vars.sort_by_key(|(k, _)| *k);

        for (name, value) in sorted_vars {
            let is_changed = self.changed.contains(name);
            let name_style = if is_changed {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };

            let display_value = if value.len() > 30 {
                format!("{}...", &value[..27])
            } else {
                value.clone()
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{}", name), name_style),
                Span::styled("=", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("\"{}\"", display_value), Style::default().fg(Color::Green)),
            ]));
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "(no variables)",
                Style::default().fg(Color::DarkGray),
            )));
        }

        self.total_lines = lines.len() as u16;
        self.viewport_height = area.height.saturating_sub(2);

        let scroll_info = if self.total_lines > self.viewport_height {
            format!(" Env [{}/{}] ", self.scroll_offset + 1, self.total_lines.saturating_sub(self.viewport_height) + 1)
        } else {
            format!(" Env ({}) ", self.variables.len())
        };

        let block = Block::default()
            .title(scroll_info)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: true })
            .scroll((self.scroll_offset, 0));

        frame.render_widget(paragraph, area);
    }
}

impl Default for EnvPane {
    fn default() -> Self {
        Self::new()
    }
}
