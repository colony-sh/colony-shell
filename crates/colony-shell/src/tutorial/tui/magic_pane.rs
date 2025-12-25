use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tutorial::events::{BranchType, LoopType, TraceEvent};
use crate::tutorial::tracer::TraceDepth;

pub struct MagicSection {
    pub title: String,
    pub content: Vec<Line<'static>>,
    pub style: SectionStyle,
}

pub enum SectionStyle {
    Parse,
    Expand,
    Execute,
    Assign,
    Concept,
    Info,
}

pub struct MagicPane {
    sections: Vec<MagicSection>,
    scroll_offset: u16,
    total_lines: u16,
    viewport_height: u16,
    ast_expanded: bool,
}

impl MagicPane {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            scroll_offset: 0,
            total_lines: 0,
            viewport_height: 0,
            ast_expanded: false,
        }
    }

    pub fn clear(&mut self) {
        self.sections.clear();
        self.scroll_offset = 0;
        self.total_lines = 0;
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        let max_scroll = self.total_lines.saturating_sub(self.viewport_height);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    pub fn toggle_ast_expand(&mut self) {
        self.ast_expanded = !self.ast_expanded;
    }

    pub fn is_ast_expanded(&self) -> bool {
        self.ast_expanded
    }

    pub fn handle_event(&mut self, event: TraceEvent, depth: TraceDepth) {
        match event {
            TraceEvent::Parse {
                ast_summary,
                ast_full,
                ..
            } => {
                let mut content: Vec<Line> = ast_summary
                    .lines()
                    .map(|line| Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(Color::Cyan),
                    )))
                    .collect();

                if depth == TraceDepth::Internals {
                    if let Some(ref full) = ast_full {
                        let expand_indicator = if self.ast_expanded { "▼" } else { "▶" };
                        content.push(Line::from(Span::styled(
                            format!("{} Full AST (press 'e' to toggle)", expand_indicator),
                            Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
                        )));
                        
                        if self.ast_expanded {
                            let formatted = serde_json::to_string_pretty(full).unwrap_or_default();
                            for line in formatted.lines() {
                                content.push(Line::from(Span::styled(
                                    format!("  {}", line),
                                    Style::default().fg(Color::DarkGray),
                                )));
                            }
                        }
                    }
                }

                self.sections.push(MagicSection {
                    title: "PARSE".to_string(),
                    content,
                    style: SectionStyle::Parse,
                });
            }

            TraceEvent::Expand {
                phase,
                input,
                output,
                explanation,
                ..
            } => {
                let content = vec![
                    Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(input.clone(), Style::default().fg(Color::Red)),
                    ]),
                    Line::from(vec![
                        Span::styled("  → ", Style::default().fg(Color::Green)),
                        Span::styled(output.clone(), Style::default().fg(Color::Green)),
                    ]),
                    Line::from(Span::styled(
                        format!("  {}", explanation),
                        Style::default().fg(Color::DarkGray),
                    )),
                ];

                self.sections.push(MagicSection {
                    title: format!("EXPAND ({})", phase.name()),
                    content,
                    style: SectionStyle::Expand,
                });
            }

            TraceEvent::Assign {
                name,
                value,
                previous,
                ..
            } => {
                let mut content = vec![Line::from(vec![
                    Span::styled(format!("  {} = ", name), Style::default().fg(Color::Blue)),
                    Span::styled(format!("\"{}\"", value), Style::default().fg(Color::Green)),
                ])];

                if let Some(prev) = previous {
                    content.push(Line::from(Span::styled(
                        format!("  (was: \"{}\")", prev),
                        Style::default().fg(Color::DarkGray),
                    )));
                }

                self.sections.push(MagicSection {
                    title: "ASSIGN".to_string(),
                    content,
                    style: SectionStyle::Assign,
                });
            }

            TraceEvent::Resolve {
                command, resolved, ..
            } => {
                let content = vec![Line::from(vec![
                    Span::styled(format!("  {} → ", command), Style::default()),
                    Span::styled(
                        resolved.display().to_string(),
                        Style::default().fg(Color::Cyan),
                    ),
                ])];

                self.sections.push(MagicSection {
                    title: "RESOLVE".to_string(),
                    content,
                    style: SectionStyle::Execute,
                });
            }

            TraceEvent::Execute {
                command, args, env, ..
            } => {
                let mut content = vec![Line::from(Span::styled(
                    format!("  {} {}", command, args.join(" ")),
                    Style::default().fg(Color::Yellow),
                ))];

                if !env.is_empty() && depth == TraceDepth::Internals {
                    content.push(Line::from(Span::styled(
                        "  Environment:",
                        Style::default().fg(Color::DarkGray),
                    )));
                    for (k, v) in env.iter().take(3) {
                        content.push(Line::from(Span::styled(
                            format!("    {}={}", k, v),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }

                self.sections.push(MagicSection {
                    title: "EXECUTE".to_string(),
                    content,
                    style: SectionStyle::Execute,
                });
            }

            TraceEvent::ExitStatus { code, signal, .. } => {
                let status_color = if code == 0 { Color::Green } else { Color::Red };
                let status_text = if let Some(sig) = signal {
                    format!("  Exit: {} ({})", code, sig)
                } else {
                    format!("  Exit: {}", code)
                };

                let content = vec![Line::from(Span::styled(
                    status_text,
                    Style::default().fg(status_color),
                ))];

                self.sections.push(MagicSection {
                    title: "STATUS".to_string(),
                    content,
                    style: SectionStyle::Info,
                });
            }

            TraceEvent::Concept {
                name, explanation, ..
            } => {
                let mut content = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        format!("  {}", name),
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                ];

                for line in explanation.lines() {
                    content.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(Color::White),
                    )));
                }

                self.sections.push(MagicSection {
                    title: "💡 CONCEPT".to_string(),
                    content,
                    style: SectionStyle::Concept,
                });
            }

            TraceEvent::CommandHint { command, hint, .. } => {
                let content = vec![
                    Line::from(Span::styled(
                        format!("  💡 {}", command),
                        Style::default().fg(Color::Cyan),
                    )),
                    Line::from(Span::styled(
                        format!("  {}", hint),
                        Style::default().fg(Color::DarkGray),
                    )),
                ];

                self.sections.push(MagicSection {
                    title: "HINT".to_string(),
                    content,
                    style: SectionStyle::Info,
                });
            }

            TraceEvent::LoopEnter {
                loop_type,
                variable,
                values,
                ..
            } => {
                let loop_name = match loop_type {
                    LoopType::For => "for",
                    LoopType::While => "while",
                    LoopType::Until => "until",
                    LoopType::Select => "select",
                };
                let mut content = vec![Line::from(Span::styled(
                    format!("  {} loop", loop_name),
                    Style::default().fg(Color::Yellow),
                ))];

                if let Some(var) = variable {
                    content.push(Line::from(Span::styled(
                        format!("  Variable: {}", var),
                        Style::default().fg(Color::Cyan),
                    )));
                }

                if let Some(vals) = values {
                    let preview: String = vals.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
                    let suffix = if vals.len() > 5 { "..." } else { "" };
                    content.push(Line::from(Span::styled(
                        format!("  Values: [{}{}]", preview, suffix),
                        Style::default().fg(Color::Green),
                    )));
                }

                self.sections.push(MagicSection {
                    title: "LOOP".to_string(),
                    content,
                    style: SectionStyle::Info,
                });
            }

            TraceEvent::LoopIteration {
                iteration, value, ..
            } => {
                let mut content = vec![Line::from(Span::styled(
                    format!("  Iteration {}", iteration),
                    Style::default().fg(Color::Yellow),
                ))];

                if let Some(val) = value {
                    content.push(Line::from(Span::styled(
                        format!("  = \"{}\"", val),
                        Style::default().fg(Color::Green),
                    )));
                }

                self.sections.push(MagicSection {
                    title: "ITERATION".to_string(),
                    content,
                    style: SectionStyle::Info,
                });
            }

            TraceEvent::Branch {
                branch_type,
                taken,
                condition,
                result,
                ..
            } => {
                let branch_name = match branch_type {
                    BranchType::If => "if",
                    BranchType::Elif => "elif",
                    BranchType::Else => "else",
                    BranchType::Case => "case",
                    BranchType::And => "&&",
                    BranchType::Or => "||",
                };
                let result_color = if result { Color::Green } else { Color::Red };
                let content = vec![
                    Line::from(Span::styled(
                        format!("  {}: {}", branch_name, condition),
                        Style::default().fg(Color::Yellow),
                    )),
                    Line::from(Span::styled(
                        format!("  -> {} ({})", taken, if result { "true" } else { "false" }),
                        Style::default().fg(result_color),
                    )),
                ];

                self.sections.push(MagicSection {
                    title: "BRANCH".to_string(),
                    content,
                    style: SectionStyle::Info,
                });
            }

            _ => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let mut all_lines: Vec<Line> = Vec::new();

        for section in &self.sections {
            let title_style = match section.style {
                SectionStyle::Parse => Style::default().fg(Color::Blue),
                SectionStyle::Expand => Style::default().fg(Color::Green),
                SectionStyle::Execute => Style::default().fg(Color::Yellow),
                SectionStyle::Assign => Style::default().fg(Color::Cyan),
                SectionStyle::Concept => Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
                SectionStyle::Info => Style::default().fg(Color::White),
            };

            all_lines.push(Line::from(Span::styled(
                format!("─── {} ───", section.title),
                title_style,
            )));

            for line in &section.content {
                all_lines.push(line.clone());
            }

            all_lines.push(Line::from(""));
        }

        self.total_lines = all_lines.len() as u16;
        self.viewport_height = area.height.saturating_sub(2);

        let scroll_info = if self.total_lines > self.viewport_height {
            let current_pos = self.scroll_offset + 1;
            let max_pos = self.total_lines.saturating_sub(self.viewport_height) + 1;
            format!(" Magic [{}/{}] ", current_pos, max_pos)
        } else {
            " Magic ".to_string()
        };

        let block = Block::default()
            .title(scroll_info)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));

        let paragraph = Paragraph::new(all_lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset, 0));

        frame.render_widget(paragraph, area);
    }
}

impl Default for MagicPane {
    fn default() -> Self {
        Self::new()
    }
}
