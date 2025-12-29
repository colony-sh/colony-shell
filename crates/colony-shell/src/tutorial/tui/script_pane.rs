use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::collections::HashSet;

const KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "in", "function", "select", "return", "exit", "break", "continue", "local", "export",
    "readonly", "declare", "typeset", "eval", "exec", "source", ".", "coproc", "time",
];

#[derive(Clone, Copy, PartialEq)]
enum TokenKind {
    Keyword,
    Comment,
    SingleQuote,
    DoubleQuote,
    Variable,
    Operator,
    Number,
    Normal,
}

struct HighlightedSpan {
    text: String,
    kind: TokenKind,
}

fn highlight_line(line: &str) -> Vec<HighlightedSpan> {
    let mut spans = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '#' {
            let comment: String = chars[i..].iter().collect();
            spans.push(HighlightedSpan {
                text: comment,
                kind: TokenKind::Comment,
            });
            break;
        }

        if ch == '\'' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(HighlightedSpan {
                text,
                kind: TokenKind::SingleQuote,
            });
            continue;
        }

        if ch == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < chars.len() {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(HighlightedSpan {
                text,
                kind: TokenKind::DoubleQuote,
            });
            continue;
        }

        if ch == '$' {
            let start = i;
            i += 1;
            if i < chars.len() {
                if chars[i] == '{' {
                    let mut depth = 1;
                    i += 1;
                    while i < chars.len() && depth > 0 {
                        if chars[i] == '{' {
                            depth += 1;
                        } else if chars[i] == '}' {
                            depth -= 1;
                        }
                        i += 1;
                    }
                } else if chars[i] == '(' {
                    if i + 1 < chars.len() && chars[i + 1] == '(' {
                        i += 2;
                        while i < chars.len()
                            && !(i + 1 < chars.len() && chars[i] == ')' && chars[i + 1] == ')')
                        {
                            i += 1;
                        }
                        if i + 1 < chars.len() {
                            i += 2;
                        }
                    } else {
                        let mut depth = 1;
                        i += 1;
                        while i < chars.len() && depth > 0 {
                            if chars[i] == '(' {
                                depth += 1;
                            } else if chars[i] == ')' {
                                depth -= 1;
                            }
                            i += 1;
                        }
                    }
                } else {
                    while i < chars.len()
                        && (chars[i].is_alphanumeric()
                            || chars[i] == '_'
                            || chars[i] == '?'
                            || chars[i] == '@'
                            || chars[i] == '*'
                            || chars[i] == '#'
                            || chars[i] == '$'
                            || chars[i] == '!'
                            || chars[i] == '-')
                    {
                        i += 1;
                        if i == start + 2
                            && !chars[start + 1].is_alphabetic()
                            && chars[start + 1] != '_'
                        {
                            break;
                        }
                    }
                }
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(HighlightedSpan {
                text,
                kind: TokenKind::Variable,
            });
            continue;
        }

        if ch == '|' || ch == '&' || ch == ';' || ch == '<' || ch == '>' {
            let start = i;
            i += 1;
            if i < chars.len()
                && ((ch == '|' && chars[i] == '|')
                    || (ch == '&' && chars[i] == '&')
                    || (ch == '>' && chars[i] == '>')
                    || (ch == '<' && chars[i] == '<'))
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(HighlightedSpan {
                text,
                kind: TokenKind::Operator,
            });
            continue;
        }

        if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-')
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let kind = if KEYWORDS.contains(&text.as_str()) {
                TokenKind::Keyword
            } else {
                TokenKind::Normal
            };
            spans.push(HighlightedSpan { text, kind });
            continue;
        }

        if ch.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(HighlightedSpan {
                text,
                kind: TokenKind::Number,
            });
            continue;
        }

        spans.push(HighlightedSpan {
            text: ch.to_string(),
            kind: TokenKind::Normal,
        });
        i += 1;
    }

    spans
}

fn spans_to_ratatui(highlighted: Vec<HighlightedSpan>, base_style: Style) -> Vec<Span<'static>> {
    highlighted
        .into_iter()
        .map(|hs| {
            let style = match hs.kind {
                TokenKind::Keyword => base_style.fg(Color::Yellow).add_modifier(Modifier::BOLD),
                TokenKind::Comment => base_style
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
                TokenKind::SingleQuote => base_style.fg(Color::Green),
                TokenKind::DoubleQuote => base_style.fg(Color::Green),
                TokenKind::Variable => base_style.fg(Color::Cyan),
                TokenKind::Operator => base_style.fg(Color::Magenta),
                TokenKind::Number => base_style.fg(Color::LightBlue),
                TokenKind::Normal => base_style,
            };
            Span::styled(hs.text, style)
        })
        .collect()
}

pub struct ScriptPane {
    lines: Vec<String>,
    current_line: u32,
    marked_lines: HashSet<u32>,
    scroll_offset: u32,
    visible_height: u32,
}

impl ScriptPane {
    pub fn new(source: String) -> Self {
        let lines: Vec<String> = source.lines().map(String::from).collect();
        Self {
            lines,
            current_line: 1,
            marked_lines: HashSet::new(),
            scroll_offset: 0,
            visible_height: 20,
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
        if line <= self.scroll_offset {
            self.scroll_offset = line.saturating_sub(1);
        } else if line > self.scroll_offset + self.visible_height {
            self.scroll_offset = line.saturating_sub(self.visible_height);
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let inner_height = area.height.saturating_sub(2) as u32;
        self.visible_height = inner_height;

        self.ensure_line_visible(self.current_line);

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

            let base_style = if is_current {
                Style::default().bg(Color::DarkGray)
            } else if is_marked {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            let highlighted = highlight_line(line_content);
            let mut spans = vec![Span::styled(
                format!("{} {:3} ", marker, line_num),
                line_num_style,
            )];
            spans.extend(spans_to_ratatui(highlighted, base_style));

            text_lines.push(Line::from(spans));
        }

        let block = Block::default()
            .title(" Script ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue));

        let paragraph = Paragraph::new(text_lines).block(block);

        frame.render_widget(paragraph, area);
    }
}
