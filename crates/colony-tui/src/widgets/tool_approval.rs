//! Tool call approval widget.
//!
//! Displays tool name, arguments, and a Yes/No approval dialog.
//! Follows the same popup pattern as HistorySearch and CommandPalette.

use ratatui::layout::Rect;
use ratatui::Frame;

use super::WidgetResult;

/// Tool approval widget state (stub -- implemented in Task 3).
#[derive(Debug, Clone)]
pub struct ToolApprovalWidget {
    tool_name: String,
    approved: bool,
}

impl ToolApprovalWidget {
    /// Create a stub tool approval widget.
    pub fn new(tool_name: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            approved: true,
        }
    }

    /// Toggle between Approve and Deny.
    pub fn toggle(&mut self) {
        self.approved = !self.approved;
    }

    /// Accept the current selection.
    pub fn accept(&self) -> Option<WidgetResult> {
        if self.approved {
            Some(WidgetResult::Accept(self.tool_name.clone()))
        } else {
            Some(WidgetResult::Cancel)
        }
    }

    /// Render the tool approval popup (stub).
    pub fn render(&self, _frame: &mut Frame, _area: Rect) {}
}
