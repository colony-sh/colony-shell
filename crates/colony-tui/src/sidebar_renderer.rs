//! Renders sidebar components into ratatui widgets
//!
//! Takes `SidebarOutput` and renders as ratatui `List` or `Paragraph` widget
//! with scrolling support and title bar.

use crate::prompt_renderer::map_style;
use colony_ui_core::component::{ComponentLine, SidebarOutput};
use ratatui::layout::Rect;
use ratatui::style::{Color as RatColor, Modifier, Style as RatStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

/// Manages rendering and scroll state for a sidebar panel
pub struct SidebarRenderer {
    scroll_offset: usize,
    title: String,
}

impl SidebarRenderer {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            scroll_offset: 0,
            title: title.into(),
        }
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self, total_lines: usize, visible_height: usize) {
        let max_offset = total_lines.saturating_sub(visible_height);
        if self.scroll_offset < max_offset {
            self.scroll_offset += 1;
        }
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Reset scroll to top
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    /// Get the current scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Render a SidebarOutput into the given area
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, output: &SidebarOutput) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                format!(" {} ", self.title),
                RatStyle::default()
                    .fg(RatColor::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_style(RatStyle::default().fg(RatColor::DarkGray));

        if !output.visible || output.lines.is_empty() {
            let empty_list = List::new(Vec::<ListItem<'static>>::new()).block(block);
            frame.render_widget(empty_list, area);
            return;
        }

        let items: Vec<ListItem<'static>> = output
            .lines
            .iter()
            .map(|line| sidebar_line_to_list_item(line))
            .collect();

        let mut list_state = ListState::default();
        list_state.select(None);

        let list = List::new(items).block(block);

        frame.render_stateful_widget(list, area, &mut list_state);
    }

    /// Convert SidebarOutput lines to ratatui Lines (useful for non-List rendering)
    pub fn to_lines(output: &SidebarOutput) -> Vec<Line<'static>> {
        output
            .lines
            .iter()
            .map(sidebar_line_to_ratatui_line)
            .collect()
    }
}

/// Convert a ComponentLine to a ratatui ListItem
fn sidebar_line_to_list_item(line: &ComponentLine) -> ListItem<'static> {
    let ratatui_line = sidebar_line_to_ratatui_line(line);
    ListItem::new(ratatui_line)
}

/// Convert a ComponentLine to a ratatui Line
fn sidebar_line_to_ratatui_line(line: &ComponentLine) -> Line<'static> {
    let mut spans = Vec::new();

    // Add indentation
    if line.indent > 0 {
        let indent_str = "  ".repeat(line.indent);
        spans.push(Span::raw(indent_str));
    }

    // Add icon if present
    if let Some(ref icon) = line.icon {
        spans.push(Span::raw(icon.clone()));
        spans.push(Span::raw(" "));
    }

    // Add styled spans
    for span in &line.spans {
        spans.push(Span::styled(span.text.clone(), map_style(&span.style)));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_ui_core::component::StyledSpan;

    fn make_line(text: &str) -> ComponentLine {
        ComponentLine::new(vec![StyledSpan::plain(text.to_string())])
    }

    #[test]
    fn test_sidebar_renderer_new() {
        let renderer = SidebarRenderer::new("Test Panel");
        assert_eq!(renderer.scroll_offset(), 0);
        assert_eq!(renderer.title, "Test Panel");
    }

    #[test]
    fn test_scroll_down() {
        let mut renderer = SidebarRenderer::new("Test");
        renderer.scroll_down(20, 10);
        assert_eq!(renderer.scroll_offset(), 1);

        // Scroll to max
        for _ in 0..20 {
            renderer.scroll_down(20, 10);
        }
        assert_eq!(renderer.scroll_offset(), 10); // max = 20 - 10
    }

    #[test]
    fn test_scroll_up() {
        let mut renderer = SidebarRenderer::new("Test");
        renderer.scroll_down(20, 10);
        renderer.scroll_down(20, 10);
        assert_eq!(renderer.scroll_offset(), 2);

        renderer.scroll_up();
        assert_eq!(renderer.scroll_offset(), 1);

        renderer.scroll_up();
        assert_eq!(renderer.scroll_offset(), 0);

        // Should not underflow
        renderer.scroll_up();
        assert_eq!(renderer.scroll_offset(), 0);
    }

    #[test]
    fn test_scroll_to_top() {
        let mut renderer = SidebarRenderer::new("Test");
        renderer.scroll_down(20, 10);
        renderer.scroll_down(20, 10);
        renderer.scroll_down(20, 10);
        assert_eq!(renderer.scroll_offset(), 3);

        renderer.scroll_to_top();
        assert_eq!(renderer.scroll_offset(), 0);
    }

    #[test]
    fn test_to_lines_empty() {
        let output = SidebarOutput::hidden();
        let lines = SidebarRenderer::to_lines(&output);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_to_lines_with_content() {
        let output = SidebarOutput::visible(vec![
            make_line("first"),
            make_line("second"),
        ]);
        let lines = SidebarRenderer::to_lines(&output);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_sidebar_line_to_ratatui_line_plain() {
        let line = make_line("hello");
        let rat_line = sidebar_line_to_ratatui_line(&line);
        // Should have just the text span (no icon, no metadata, no indent)
        assert_eq!(rat_line.spans.len(), 1);
    }

    #[test]
    fn test_sidebar_line_to_ratatui_line_with_icon() {
        let line = SidebarLine::new(StyledSpan::plain("file.rs".to_string()))
            .with_icon(StyledSpan::new("M", UiStyle::default().fg(UiColor::Yellow)));
        let rat_line = sidebar_line_to_ratatui_line(&line);
        // icon + space + text = 3 spans
        assert_eq!(rat_line.spans.len(), 3);
    }

    #[test]
    fn test_sidebar_line_to_ratatui_line_with_metadata() {
        let line = SidebarLine::new(StyledSpan::plain("server".to_string()))
            .with_metadata(StyledSpan::new("3 tools", UiStyle::default().dim()));
        let rat_line = sidebar_line_to_ratatui_line(&line);
        // text + space + metadata = 3 spans
        assert_eq!(rat_line.spans.len(), 3);
    }

    #[test]
    fn test_sidebar_line_to_ratatui_line_with_indent() {
        let line = SidebarLine::new(StyledSpan::plain("indented".to_string()))
            .with_indent(2);
        let rat_line = sidebar_line_to_ratatui_line(&line);
        // indent + text = 2 spans
        assert_eq!(rat_line.spans.len(), 2);
        // Indent should be 4 spaces (2 levels * 2 spaces)
        assert_eq!(rat_line.spans[0].content.as_ref(), "    ");
    }

    #[test]
    fn test_sidebar_line_to_ratatui_line_full() {
        let line = SidebarLine::new(StyledSpan::plain("file.rs".to_string()))
            .with_icon(StyledSpan::new("M", UiStyle::default().fg(UiColor::Yellow)))
            .with_metadata(StyledSpan::new("+3", UiStyle::default().fg(UiColor::Green)))
            .with_indent(1);
        let rat_line = sidebar_line_to_ratatui_line(&line);
        // indent + icon + space + text + space + metadata = 6 spans
        assert_eq!(rat_line.spans.len(), 6);
    }

    #[test]
    fn test_sidebar_line_style_mapping() {
        let line = SidebarLine::new(StyledSpan::new(
            "styled".to_string(),
            UiStyle::default().fg(UiColor::Cyan).bold(),
        ));
        let rat_line = sidebar_line_to_ratatui_line(&line);
        let span = &rat_line.spans[0];
        assert_eq!(span.style.fg, Some(RatColor::Cyan));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }
}
