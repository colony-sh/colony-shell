//! Yes/No confirmation dialog.
//!
//! Renders a centered popup with a prompt and Yes/No buttons.
//! Uses the same popup pattern as HistorySearch and CommandPalette:
//! `active: bool`, `accept() -> Option<WidgetResult>`, ratatui `Clear` overlay.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::WidgetResult;

/// Confirmation dialog state.
#[derive(Debug, Clone)]
pub struct ConfirmWidget {
    /// The prompt question shown to the user.
    pub prompt: String,
    /// Optional detail line displayed below the prompt.
    pub detail: Option<String>,
    /// Whether "Yes" is currently selected.
    selected_yes: bool,
}

impl ConfirmWidget {
    /// Create a new confirmation dialog with the given prompt.
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            detail: None,
            selected_yes: true,
        }
    }

    /// Add an optional detail line below the prompt.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Toggle between Yes and No.
    pub fn toggle(&mut self) {
        self.selected_yes = !self.selected_yes;
    }

    /// Accept the current selection.
    /// Returns `Accept("yes")` if Yes is selected, `Cancel` if No.
    pub fn accept(&self) -> Option<WidgetResult> {
        if self.selected_yes {
            Some(WidgetResult::Accept("yes".to_string()))
        } else {
            Some(WidgetResult::Cancel)
        }
    }

    /// Render the confirmation dialog as a centered popup overlay.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let width = 50u16.min(area.width.saturating_sub(4));
        let height = if self.detail.is_some() { 7 } else { 5 };
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup);

        let yes_style = if self.selected_yes {
            Style::default().fg(Color::Black).bg(Color::Green).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let no_style = if self.selected_yes {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Black).bg(Color::Red).bold()
        };

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(&self.prompt, Style::default().bold())),
        ];

        if let Some(ref detail) = self.detail {
            lines.push(Line::from(Span::styled(
                detail.as_str(),
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(" Yes ", yes_style),
            Span::raw("  "),
            Span::styled(" No ", no_style),
        ]));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Confirm ");

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
    fn test_confirm_new_defaults_to_yes() {
        let w = ConfirmWidget::new("Delete file?");
        assert_eq!(w.prompt, "Delete file?");
        assert!(w.detail.is_none());
        assert_eq!(
            w.accept(),
            Some(WidgetResult::Accept("yes".to_string()))
        );
    }

    #[test]
    fn test_confirm_toggle_selects_no() {
        let mut w = ConfirmWidget::new("Continue?");
        w.toggle();
        assert_eq!(w.accept(), Some(WidgetResult::Cancel));
    }

    #[test]
    fn test_confirm_toggle_back_to_yes() {
        let mut w = ConfirmWidget::new("Continue?");
        w.toggle();
        w.toggle();
        assert_eq!(
            w.accept(),
            Some(WidgetResult::Accept("yes".to_string()))
        );
    }

    #[test]
    fn test_confirm_with_detail() {
        let w = ConfirmWidget::new("Delete?")
            .with_detail("This cannot be undone");
        assert_eq!(w.detail.as_deref(), Some("This cannot be undone"));
    }
}
