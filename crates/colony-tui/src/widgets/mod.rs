//! Interactive prompt widgets rendered as TUI popups.
//!
//! Each widget follows the same pattern as `HistorySearch` and `CommandPalette`:
//! - Struct with internal state
//! - `accept()` returns `Option<WidgetResult>` and deactivates
//! - `render()` draws a centered popup overlay using ratatui `Clear`
//! - Activated via `Mode::PromptWidget` in the keybinding system

pub mod branch_picker;
pub mod confirm;
pub mod file_picker;
pub mod tool_approval;

pub use branch_picker::BranchPickerWidget;
pub use confirm::ConfirmWidget;
pub use file_picker::FilePickerWidget;
pub use tool_approval::ToolApprovalWidget;

use ratatui::layout::Rect;
use ratatui::Frame;

/// Result returned when a prompt widget completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetResult {
    /// User accepted with a value.
    Accept(String),
    /// User cancelled.
    Cancel,
}

/// The kind of prompt widget currently active.
#[derive(Debug, Clone)]
pub enum ActiveWidget {
    /// Yes/No confirmation dialog.
    Confirm(ConfirmWidget),
    /// File/directory picker with fuzzy search.
    FilePicker(FilePickerWidget),
    /// Git branch picker with fuzzy search.
    BranchPicker(BranchPickerWidget),
    /// Tool call approval dialog.
    ToolApproval(ToolApprovalWidget),
}

/// Container for the currently active prompt widget (if any).
///
/// Stored as a field on `App`, following the same pattern as
/// `history_search: HistorySearch` and `command_palette: CommandPalette`.
#[derive(Debug, Default)]
pub struct PromptWidgetState {
    active: Option<ActiveWidget>,
    last_result: Option<WidgetResult>,
}

impl PromptWidgetState {
    /// Whether any prompt widget is currently active.
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Activate a specific widget, clearing any previous result.
    pub fn activate(&mut self, widget: ActiveWidget) {
        self.active = Some(widget);
        self.last_result = None;
    }

    /// Dismiss the active widget with a Cancel result.
    pub fn dismiss(&mut self) {
        self.last_result = Some(WidgetResult::Cancel);
        self.active = None;
    }

    /// Take the last result, if any.
    pub fn take_result(&mut self) -> Option<WidgetResult> {
        self.last_result.take()
    }

    /// Borrow the active widget (immutable).
    pub fn active_widget(&self) -> Option<&ActiveWidget> {
        self.active.as_ref()
    }

    /// Borrow the active widget (mutable).
    pub fn active_widget_mut(&mut self) -> Option<&mut ActiveWidget> {
        self.active.as_mut()
    }

    /// Route a character input to the active widget.
    pub fn insert_char(&mut self, c: char) {
        match self.active.as_mut() {
            Some(ActiveWidget::FilePicker(w)) => w.insert_char(c),
            Some(ActiveWidget::BranchPicker(w)) => w.insert_char(c),
            Some(ActiveWidget::ToolApproval(_) | ActiveWidget::Confirm(_)) | None => {}
        }
    }

    /// Route a backspace to the active widget.
    pub fn delete_char(&mut self) {
        match self.active.as_mut() {
            Some(ActiveWidget::FilePicker(w)) => w.backspace(),
            Some(ActiveWidget::BranchPicker(w)) => w.backspace(),
            Some(ActiveWidget::ToolApproval(_) | ActiveWidget::Confirm(_)) | None => {}
        }
    }

    /// Accept the current selection, returning the result.
    /// Deactivates the widget on success.
    pub fn accept(&mut self) -> Option<WidgetResult> {
        let result = match self.active.as_mut() {
            Some(ActiveWidget::Confirm(w)) => w.accept(),
            Some(ActiveWidget::FilePicker(w)) => w.accept(),
            Some(ActiveWidget::BranchPicker(w)) => w.accept(),
            Some(ActiveWidget::ToolApproval(w)) => w.accept(),
            None => None,
        };

        if let Some(ref r) = result {
            self.last_result = Some(r.clone());
            self.active = None;
        }
        result
    }

    /// Move selection up / toggle to previous option.
    pub fn prev(&mut self) {
        match self.active.as_mut() {
            Some(ActiveWidget::FilePicker(w)) => w.prev(),
            Some(ActiveWidget::BranchPicker(w)) => w.prev(),
            Some(ActiveWidget::Confirm(w)) => w.toggle(),
            Some(ActiveWidget::ToolApproval(w)) => w.toggle(),
            None => {}
        }
    }

    /// Move selection down / toggle to next option.
    pub fn next(&mut self) {
        match self.active.as_mut() {
            Some(ActiveWidget::FilePicker(w)) => w.next(),
            Some(ActiveWidget::BranchPicker(w)) => w.next(),
            Some(ActiveWidget::Confirm(w)) => w.toggle(),
            Some(ActiveWidget::ToolApproval(w)) => w.toggle(),
            None => {}
        }
    }

    /// Render the active widget as a popup overlay.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        match self.active.as_ref() {
            Some(ActiveWidget::Confirm(w)) => w.render(frame, area),
            Some(ActiveWidget::FilePicker(w)) => w.render(frame, area),
            Some(ActiveWidget::BranchPicker(w)) => w.render(frame, area),
            Some(ActiveWidget::ToolApproval(w)) => w.render(frame, area),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_widget_state_default_inactive() {
        let state = PromptWidgetState::default();
        assert!(!state.is_active());
        assert!(state.active_widget().is_none());
    }

    #[test]
    fn test_activate_makes_active() {
        let mut state = PromptWidgetState::default();
        state.activate(ActiveWidget::Confirm(ConfirmWidget::new("test?")));
        assert!(state.is_active());
        assert!(state.active_widget().is_some());
    }

    #[test]
    fn test_dismiss_deactivates_with_cancel() {
        let mut state = PromptWidgetState::default();
        state.activate(ActiveWidget::Confirm(ConfirmWidget::new("test?")));
        state.dismiss();
        assert!(!state.is_active());
        assert_eq!(state.take_result(), Some(WidgetResult::Cancel));
    }

    #[test]
    fn test_accept_deactivates_with_result() {
        let mut state = PromptWidgetState::default();
        state.activate(ActiveWidget::Confirm(ConfirmWidget::new("test?")));
        let result = state.accept();
        assert_eq!(result, Some(WidgetResult::Accept("yes".to_string())));
        assert!(!state.is_active());
        assert_eq!(
            state.take_result(),
            Some(WidgetResult::Accept("yes".to_string()))
        );
    }

    #[test]
    fn test_accept_on_empty_returns_none() {
        let mut state = PromptWidgetState::default();
        assert_eq!(state.accept(), None);
    }

    #[test]
    fn test_prev_next_toggle_confirm() {
        let mut state = PromptWidgetState::default();
        state.activate(ActiveWidget::Confirm(ConfirmWidget::new("test?")));

        // Default is Yes, toggle to No
        state.next();
        let result = state.accept();
        assert_eq!(result, Some(WidgetResult::Cancel));
    }
}
