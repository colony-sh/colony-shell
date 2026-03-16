//! Tool call approval widget.
//!
//! Displays tool name, arguments, and a Yes/No approval dialog.
//! Used when an LLM agent requests to execute a tool that requires user approval.
//! Follows the same popup pattern as HistorySearch and CommandPalette:
//! `active: bool`, `accept() -> Option<WidgetResult>`, ratatui `Clear` overlay.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::WidgetResult;

/// Tool approval widget state.
#[derive(Debug, Clone)]
pub struct ToolApprovalWidget {
    /// Name of the tool being requested.
    tool_name: String,
    /// Formatted arguments string.
    arguments: String,
    /// Component or agent ID that requested the tool.
    component_id: String,
    /// Whether "Approve" is currently selected (vs "Deny").
    approved: bool,
}

impl ToolApprovalWidget {
    /// Create a new tool approval widget.
    pub fn new(
        tool_name: impl Into<String>,
        arguments: impl Into<String>,
        component_id: impl Into<String>,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            arguments: arguments.into(),
            component_id: component_id.into(),
            approved: true,
        }
    }

    /// Toggle between Approve and Deny.
    pub fn toggle(&mut self) {
        self.approved = !self.approved;
    }

    /// Accept the current selection.
    /// Returns `Accept(tool_name)` if approved, `Cancel` if denied.
    pub fn accept(&self) -> Option<WidgetResult> {
        if self.approved {
            Some(WidgetResult::Accept(self.tool_name.clone()))
        } else {
            Some(WidgetResult::Cancel)
        }
    }

    /// Render the tool approval dialog as a centered popup overlay.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let width = 60u16.min(area.width.saturating_sub(4));
        // Height depends on whether we have arguments
        let has_args = !self.arguments.is_empty();
        let height: u16 = if has_args { 10 } else { 7 };
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup);

        let approve_style = if self.approved {
            Style::default().fg(Color::Black).bg(Color::Green).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let deny_style = if self.approved {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Black).bg(Color::Red).bold()
        };

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Tool Approval Required",
                Style::default().fg(Color::Yellow).bold(),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Tool: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&self.tool_name, Style::default().fg(Color::Cyan).bold()),
            ]),
        ];

        if has_args {
            // Truncate long arguments for display
            let display_args = if self.arguments.len() > (width as usize - 12) {
                format!(
                    "{}...",
                    &self.arguments[..width as usize - 15]
                )
            } else {
                self.arguments.clone()
            };
            lines.push(Line::from(vec![
                Span::styled("Args: ", Style::default().fg(Color::DarkGray)),
                Span::styled(display_args, Style::default().fg(Color::White)),
            ]));
        }

        if !self.component_id.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("From: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&self.component_id, Style::default().fg(Color::DarkGray)),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(" Approve ", approve_style),
            Span::raw("  "),
            Span::styled(" Deny ", deny_style),
        ]));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Tool Approval ");

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, popup);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_approval_defaults_to_approved() {
        let w = ToolApprovalWidget::new("bash", "echo hello", "agent-1");
        assert_eq!(
            w.accept(),
            Some(WidgetResult::Accept("bash".to_string()))
        );
    }

    #[test]
    fn test_tool_approval_toggle_denies() {
        let mut w = ToolApprovalWidget::new("bash", "rm -rf /", "agent-1");
        w.toggle();
        assert_eq!(w.accept(), Some(WidgetResult::Cancel));
    }

    #[test]
    fn test_tool_approval_toggle_back_approves() {
        let mut w = ToolApprovalWidget::new("bash", "", "agent-1");
        w.toggle();
        w.toggle();
        assert_eq!(
            w.accept(),
            Some(WidgetResult::Accept("bash".to_string()))
        );
    }

    #[test]
    fn test_tool_approval_empty_args() {
        let w = ToolApprovalWidget::new("list_files", "", "");
        assert_eq!(
            w.accept(),
            Some(WidgetResult::Accept("list_files".to_string()))
        );
    }
}
