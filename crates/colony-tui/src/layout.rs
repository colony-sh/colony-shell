//! Layout calculations for the TUI
//!
//! Three-column layout: optional left sidebar | main content | optional right sidebar
//! Main area splits into: content area (top) + input area (bottom, 3 lines min)
//! Status bar at very bottom (1 line)

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Default sidebar width in columns
pub const DEFAULT_SIDEBAR_WIDTH: u16 = 30;

/// Minimum width for the main content area
pub const MIN_MAIN_WIDTH: u16 = 40;

/// Minimum height for the input area
pub const MIN_INPUT_HEIGHT: u16 = 3;

/// Height of the status bar
pub const STATUS_BAR_HEIGHT: u16 = 1;

/// Tracks sidebar visibility and width
#[derive(Debug, Clone)]
pub struct LayoutState {
    pub left_sidebar_visible: bool,
    pub right_sidebar_visible: bool,
    pub left_sidebar_width: u16,
    pub right_sidebar_width: u16,
}

impl LayoutState {
    pub fn new() -> Self {
        Self {
            left_sidebar_visible: false,
            right_sidebar_visible: false,
            left_sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            right_sidebar_width: DEFAULT_SIDEBAR_WIDTH,
        }
    }

    pub fn toggle_left_sidebar(&mut self) {
        self.left_sidebar_visible = !self.left_sidebar_visible;
    }

    pub fn toggle_right_sidebar(&mut self) {
        self.right_sidebar_visible = !self.right_sidebar_visible;
    }
}

impl Default for LayoutState {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of layout calculation — the computed rectangles for each area
#[derive(Debug, Clone)]
pub struct TuiLayout {
    pub left_sidebar: Option<Rect>,
    pub main_content: Rect,
    pub right_sidebar: Option<Rect>,
    pub input_area: Rect,
    pub status_bar: Rect,
}

impl TuiLayout {
    /// Compute the layout from the total area and the current layout state
    pub fn compute(area: Rect, state: &LayoutState) -> Self {
        // First split: main body vs status bar at bottom
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),                    // body
                Constraint::Length(STATUS_BAR_HEIGHT), // status bar
            ])
            .split(area);

        let body = vertical[0];
        let status_bar = vertical[1];

        // Split body into content + input area
        let body_vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),                   // content + sidebars
                Constraint::Length(MIN_INPUT_HEIGHT), // input area
            ])
            .split(body);

        let content_region = body_vertical[0];
        let input_area = body_vertical[1];

        // Horizontal split for sidebars
        let mut constraints = Vec::new();

        let left_visible =
            state.left_sidebar_visible && area.width > MIN_MAIN_WIDTH + state.left_sidebar_width;
        let right_visible =
            state.right_sidebar_visible && area.width > MIN_MAIN_WIDTH + state.right_sidebar_width;

        if left_visible {
            constraints.push(Constraint::Length(state.left_sidebar_width));
        }
        constraints.push(Constraint::Min(MIN_MAIN_WIDTH));
        if right_visible {
            constraints.push(Constraint::Length(state.right_sidebar_width));
        }

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(content_region);

        let (left_sidebar, main_content, right_sidebar) = match (left_visible, right_visible) {
            (true, true) => (Some(horizontal[0]), horizontal[1], Some(horizontal[2])),
            (true, false) => (Some(horizontal[0]), horizontal[1], None),
            (false, true) => (None, horizontal[0], Some(horizontal[1])),
            (false, false) => (None, horizontal[0], None),
        };

        Self {
            left_sidebar,
            main_content,
            right_sidebar,
            input_area,
            status_bar,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_area(width: u16, height: u16) -> Rect {
        Rect::new(0, 0, width, height)
    }

    #[test]
    fn test_layout_state_default() {
        let state = LayoutState::new();
        assert!(!state.left_sidebar_visible);
        assert!(!state.right_sidebar_visible);
        assert_eq!(state.left_sidebar_width, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(state.right_sidebar_width, DEFAULT_SIDEBAR_WIDTH);
    }

    #[test]
    fn test_toggle_left_sidebar() {
        let mut state = LayoutState::new();
        assert!(!state.left_sidebar_visible);
        state.toggle_left_sidebar();
        assert!(state.left_sidebar_visible);
        state.toggle_left_sidebar();
        assert!(!state.left_sidebar_visible);
    }

    #[test]
    fn test_toggle_right_sidebar() {
        let mut state = LayoutState::new();
        assert!(!state.right_sidebar_visible);
        state.toggle_right_sidebar();
        assert!(state.right_sidebar_visible);
    }

    #[test]
    fn test_layout_no_sidebars() {
        let state = LayoutState::new();
        let area = test_area(120, 40);
        let layout = TuiLayout::compute(area, &state);

        assert!(layout.left_sidebar.is_none());
        assert!(layout.right_sidebar.is_none());
        assert_eq!(layout.main_content.width, 120);
        assert_eq!(layout.status_bar.height, STATUS_BAR_HEIGHT);
        assert_eq!(layout.input_area.height, MIN_INPUT_HEIGHT);
    }

    #[test]
    fn test_layout_left_sidebar_visible() {
        let mut state = LayoutState::new();
        state.left_sidebar_visible = true;
        let area = test_area(120, 40);
        let layout = TuiLayout::compute(area, &state);

        assert!(layout.left_sidebar.is_some());
        assert!(layout.right_sidebar.is_none());

        let left = layout.left_sidebar.expect("left sidebar should be present");
        assert_eq!(left.width, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(layout.main_content.width, 120 - DEFAULT_SIDEBAR_WIDTH);
    }

    #[test]
    fn test_layout_right_sidebar_visible() {
        let mut state = LayoutState::new();
        state.right_sidebar_visible = true;
        let area = test_area(120, 40);
        let layout = TuiLayout::compute(area, &state);

        assert!(layout.left_sidebar.is_none());
        assert!(layout.right_sidebar.is_some());

        let right = layout
            .right_sidebar
            .expect("right sidebar should be present");
        assert_eq!(right.width, DEFAULT_SIDEBAR_WIDTH);
    }

    #[test]
    fn test_layout_both_sidebars() {
        let mut state = LayoutState::new();
        state.left_sidebar_visible = true;
        state.right_sidebar_visible = true;
        let area = test_area(120, 40);
        let layout = TuiLayout::compute(area, &state);

        assert!(layout.left_sidebar.is_some());
        assert!(layout.right_sidebar.is_some());

        let left = layout.left_sidebar.expect("left sidebar");
        let right = layout.right_sidebar.expect("right sidebar");
        assert_eq!(left.width, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(right.width, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(
            layout.main_content.width,
            120 - DEFAULT_SIDEBAR_WIDTH - DEFAULT_SIDEBAR_WIDTH
        );
    }

    #[test]
    fn test_layout_sidebar_hidden_when_too_narrow() {
        let mut state = LayoutState::new();
        state.left_sidebar_visible = true;
        // Area is too narrow to fit both sidebar and min main width
        let area = test_area(50, 40);
        let layout = TuiLayout::compute(area, &state);

        // Sidebar shouldn't appear because 50 - 30 = 20 < MIN_MAIN_WIDTH (40)
        assert!(layout.left_sidebar.is_none());
        assert_eq!(layout.main_content.width, 50);
    }

    #[test]
    fn test_layout_status_bar_position() {
        let state = LayoutState::new();
        let area = test_area(120, 40);
        let layout = TuiLayout::compute(area, &state);

        assert_eq!(layout.status_bar.y, 39); // bottom of 40-line area
        assert_eq!(layout.status_bar.height, 1);
        assert_eq!(layout.status_bar.width, 120);
    }

    #[test]
    fn test_layout_input_area_above_status_bar() {
        let state = LayoutState::new();
        let area = test_area(120, 40);
        let layout = TuiLayout::compute(area, &state);

        assert_eq!(layout.input_area.height, MIN_INPUT_HEIGHT);
        // Input should end right before the status bar
        assert_eq!(
            layout.input_area.y + layout.input_area.height,
            layout.status_bar.y
        );
    }

    #[test]
    fn test_layout_custom_sidebar_widths() {
        let mut state = LayoutState::new();
        state.left_sidebar_visible = true;
        state.left_sidebar_width = 25;
        state.right_sidebar_visible = true;
        state.right_sidebar_width = 35;

        let area = test_area(140, 40);
        let layout = TuiLayout::compute(area, &state);

        let left = layout.left_sidebar.expect("left sidebar");
        let right = layout.right_sidebar.expect("right sidebar");
        assert_eq!(left.width, 25);
        assert_eq!(right.width, 35);
    }
}
