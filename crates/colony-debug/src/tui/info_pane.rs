//! Info pane -- annotations (X-Ray) or variable inspector (Debug).
//!
//! Right pane of the split TUI. In X-Ray mode it shows expansion
//! annotations, control flow decisions, and concept explanations.
//! In Debug mode it also shows the variable inspector.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::collections::HashMap;

use crate::controller::DebugMode;
use crate::events::TraceEvent;

/// A rendered section in the info pane.
struct InfoSection {
    title: String,
    lines: Vec<Line<'static>>,
}

/// Right pane: annotations and variable inspector.
pub struct InfoPane {
    sections: Vec<InfoSection>,
    mode: DebugMode,
}

impl InfoPane {
    /// Create a new info pane for the given mode.
    pub fn new(mode: DebugMode) -> Self {
        Self {
            sections: Vec::new(),
            mode,
        }
    }

    /// Switch the display mode.
    pub fn set_mode(&mut self, mode: DebugMode) {
        self.mode = mode;
    }

    /// Clear all sections for a new statement.
    pub fn clear(&mut self) {
        self.sections.clear();
    }

    /// Number of sections currently displayed.
    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    /// Add a trace event as a rendered section.
    pub fn push_event(&mut self, event: &TraceEvent) {
        match event {
            TraceEvent::Parse {
                node_kind, summary, ..
            } => {
                self.sections.push(InfoSection {
                    title: "PARSE".to_string(),
                    lines: vec![Line::from(Span::styled(
                        format!("{node_kind}: {summary}"),
                        Style::default().fg(Color::Cyan),
                    ))],
                });
            }
            TraceEvent::Expand {
                phase,
                input,
                output,
                explanation,
                ..
            } => {
                self.sections.push(InfoSection {
                    title: format!("EXPAND ({phase})"),
                    lines: vec![
                        Line::from(vec![
                            Span::styled(input.clone(), Style::default().fg(Color::Yellow)),
                            Span::raw(" -> "),
                            Span::styled(
                                format!("\"{output}\""),
                                Style::default().fg(Color::Green),
                            ),
                        ]),
                        Line::from(Span::styled(
                            explanation.clone(),
                            Style::default().fg(Color::DarkGray).italic(),
                        )),
                    ],
                });
            }
            TraceEvent::Assign { name, value, .. } => {
                self.sections.push(InfoSection {
                    title: "ASSIGN".to_string(),
                    lines: vec![Line::from(vec![
                        Span::styled(name.clone(), Style::default().fg(Color::Magenta)),
                        Span::raw("="),
                        Span::styled(
                            format!("\"{value}\""),
                            Style::default().fg(Color::Green),
                        ),
                    ])],
                });
            }
            TraceEvent::Execute {
                command,
                resolved_path,
                args,
                ..
            } => {
                self.sections.push(InfoSection {
                    title: "EXECUTE".to_string(),
                    lines: vec![
                        Line::from(Span::styled(
                            format!("{command} -> {resolved_path}"),
                            Style::default().fg(Color::Blue),
                        )),
                        Line::from(Span::styled(
                            format!("args: {args:?}"),
                            Style::default().fg(Color::DarkGray),
                        )),
                    ],
                });
            }
            TraceEvent::ControlFlow {
                kind,
                decision,
                detail,
                ..
            } => {
                self.sections.push(InfoSection {
                    title: format!("FLOW ({kind})"),
                    lines: vec![
                        Line::from(Span::styled(
                            decision.clone(),
                            Style::default().fg(Color::Yellow),
                        )),
                        Line::from(Span::styled(
                            detail.clone(),
                            Style::default().fg(Color::DarkGray),
                        )),
                    ],
                });
            }
            TraceEvent::LoopIteration {
                variable,
                value,
                iteration,
                total,
                ..
            } => {
                let total_str = total
                    .map(|t| format!("/{t}"))
                    .unwrap_or_default();
                self.sections.push(InfoSection {
                    title: "LOOP".to_string(),
                    lines: vec![Line::from(Span::styled(
                        format!("{variable}={value} (iteration {iteration}{total_str})"),
                        Style::default().fg(Color::Yellow),
                    ))],
                });
            }
            TraceEvent::PipelineSetup {
                commands,
                stage_count,
                ..
            } => {
                self.sections.push(InfoSection {
                    title: "PIPELINE".to_string(),
                    lines: vec![Line::from(Span::styled(
                        format!("{stage_count} stages: {}", commands.join(" | ")),
                        Style::default().fg(Color::Blue),
                    ))],
                });
            }
            TraceEvent::Concept { name, explanation } => {
                self.sections.push(InfoSection {
                    title: format!("CONCEPT: {name}"),
                    lines: explanation
                        .lines()
                        .map(|l| {
                            Line::from(Span::styled(
                                l.to_string(),
                                Style::default().fg(Color::White),
                            ))
                        })
                        .collect(),
                });
            }
            TraceEvent::BreakpointHit { id, line, .. } => {
                self.sections.push(InfoSection {
                    title: "BREAKPOINT".to_string(),
                    lines: vec![Line::from(Span::styled(
                        format!("Hit breakpoint #{id} at line {line}"),
                        Style::default().fg(Color::Red).bold(),
                    ))],
                });
            }
            TraceEvent::ExitCode {
                command,
                code,
                duration_us,
                ..
            } => {
                let style = if *code == 0 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                };
                self.sections.push(InfoSection {
                    title: "EXIT".to_string(),
                    lines: vec![Line::from(Span::styled(
                        format!("{command} exited {code} ({duration_us}us)"),
                        style,
                    ))],
                });
            }
            TraceEvent::Redirect { kind, target, .. } => {
                self.sections.push(InfoSection {
                    title: "REDIRECT".to_string(),
                    lines: vec![Line::from(Span::styled(
                        format!("{kind} -> {target}"),
                        Style::default().fg(Color::DarkGray),
                    ))],
                });
            }
            TraceEvent::WatchTriggered {
                expression,
                old_value,
                new_value,
            } => {
                self.sections.push(InfoSection {
                    title: "WATCH".to_string(),
                    lines: vec![Line::from(vec![
                        Span::styled(expression.clone(), Style::default().fg(Color::Magenta)),
                        Span::raw(": "),
                        Span::styled(
                            format!("\"{old_value}\""),
                            Style::default().fg(Color::Red),
                        ),
                        Span::raw(" -> "),
                        Span::styled(
                            format!("\"{new_value}\""),
                            Style::default().fg(Color::Green),
                        ),
                    ])],
                });
            }
            // LineEnter and LineExit are handled by the TUI loop, not rendered here
            TraceEvent::LineEnter { .. } | TraceEvent::LineExit { .. } => {}
        }
    }

    /// Render the variables panel (Debug mode).
    pub fn push_variables(&mut self, variables: &HashMap<String, String>) {
        if variables.is_empty() {
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut sorted: Vec<_> = variables.iter().collect();
        sorted.sort_by_key(|(k, _)| (*k).clone());
        for (name, value) in sorted {
            lines.push(Line::from(vec![
                Span::styled(name.clone(), Style::default().fg(Color::Magenta)),
                Span::raw("="),
                Span::styled(
                    format!("\"{value}\""),
                    Style::default().fg(Color::Green),
                ),
            ]));
        }
        self.sections.push(InfoSection {
            title: "VARIABLES".to_string(),
            lines,
        });
    }

    /// Render the info pane into the given area.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mut all_lines: Vec<Line> = Vec::new();

        for section in &self.sections {
            all_lines.push(Line::from(Span::styled(
                format!("--- {} ---", section.title),
                Style::default().fg(Color::Cyan).bold(),
            )));
            all_lines.extend(section.lines.iter().cloned());
            all_lines.push(Line::raw(""));
        }

        if all_lines.is_empty() {
            all_lines.push(Line::from(Span::styled(
                match self.mode {
                    DebugMode::XRay => "Waiting for execution...",
                    DebugMode::Debug => "Paused. Press [s]tep or [c]ontinue.",
                },
                Style::default().fg(Color::DarkGray).italic(),
            )));
        }

        let title = match self.mode {
            DebugMode::XRay => " X-Ray ",
            DebugMode::Debug => " Debug ",
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let paragraph = Paragraph::new(all_lines)
            .block(block)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_empty() {
        let pane = InfoPane::new(DebugMode::XRay);
        assert_eq!(pane.section_count(), 0);
    }

    #[test]
    fn push_parse_event() {
        let mut pane = InfoPane::new(DebugMode::XRay);
        pane.push_event(&TraceEvent::Parse {
            line: 1,
            node_kind: "SimpleCommand".to_string(),
            summary: "echo hello".to_string(),
            ast_detail: None,
        });
        assert_eq!(pane.section_count(), 1);
    }

    #[test]
    fn push_expand_event() {
        let mut pane = InfoPane::new(DebugMode::XRay);
        pane.push_event(&TraceEvent::Expand {
            line: 1,
            phase: "parameter".to_string(),
            input: "${x:-val}".to_string(),
            output: "val".to_string(),
            explanation: "default used".to_string(),
        });
        assert_eq!(pane.section_count(), 1);
    }

    #[test]
    fn push_breakpoint_event() {
        let mut pane = InfoPane::new(DebugMode::Debug);
        pane.push_event(&TraceEvent::BreakpointHit {
            id: 1,
            line: 5,
            condition: None,
        });
        assert_eq!(pane.section_count(), 1);
    }

    #[test]
    fn clear_resets() {
        let mut pane = InfoPane::new(DebugMode::XRay);
        pane.push_event(&TraceEvent::Assign {
            line: 1,
            name: "x".to_string(),
            value: "1".to_string(),
        });
        assert_eq!(pane.section_count(), 1);
        pane.clear();
        assert_eq!(pane.section_count(), 0);
    }

    #[test]
    fn push_variables() {
        let mut pane = InfoPane::new(DebugMode::Debug);
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "1".to_string());
        vars.insert("y".to_string(), "2".to_string());
        pane.push_variables(&vars);
        assert_eq!(pane.section_count(), 1);
    }

    #[test]
    fn push_variables_empty_is_noop() {
        let mut pane = InfoPane::new(DebugMode::Debug);
        pane.push_variables(&HashMap::new());
        assert_eq!(pane.section_count(), 0);
    }

    #[test]
    fn set_mode_changes() {
        let mut pane = InfoPane::new(DebugMode::XRay);
        pane.set_mode(DebugMode::Debug);
        assert!(matches!(pane.mode, DebugMode::Debug));
    }

    #[test]
    fn line_enter_not_rendered() {
        let mut pane = InfoPane::new(DebugMode::XRay);
        pane.push_event(&TraceEvent::LineEnter {
            line: 1,
            column: 1,
            source: "echo hi".to_string(),
        });
        assert_eq!(pane.section_count(), 0);
    }

    #[test]
    fn line_exit_not_rendered() {
        let mut pane = InfoPane::new(DebugMode::XRay);
        pane.push_event(&TraceEvent::LineExit {
            line: 1,
            duration_us: 100,
        });
        assert_eq!(pane.section_count(), 0);
    }

    #[test]
    fn push_execute_event() {
        let mut pane = InfoPane::new(DebugMode::XRay);
        pane.push_event(&TraceEvent::Execute {
            line: 1,
            command: "ls".to_string(),
            resolved_path: "/bin/ls".to_string(),
            args: vec!["-la".to_string()],
        });
        assert_eq!(pane.section_count(), 1);
    }

    #[test]
    fn push_concept_event() {
        let mut pane = InfoPane::new(DebugMode::XRay);
        pane.push_event(&TraceEvent::Concept {
            name: "parameter_default".to_string(),
            explanation: "Line 1\nLine 2".to_string(),
        });
        assert_eq!(pane.section_count(), 1);
    }
}
