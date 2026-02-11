//! Scrollable buffer view
//!
//! `BufferView` provides a scrollable window into a `StyledBuffer`,
//! tracking the current scroll offset and visible line count.

use super::buffer::{BufferLine, StyledBuffer};
use serde::{Deserialize, Serialize};

/// A scrollable window into a `StyledBuffer`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferView {
    offset: usize,
    visible_lines: usize,
}

impl BufferView {
    /// Create a new view with the given visible height
    pub fn new(visible_lines: usize) -> Self {
        Self {
            offset: 0,
            visible_lines,
        }
    }

    /// Get the current scroll offset
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Get the number of visible lines
    pub fn visible_lines(&self) -> usize {
        self.visible_lines
    }

    /// Set the number of visible lines (e.g., on terminal resize)
    pub fn set_visible_lines(&mut self, lines: usize) {
        self.visible_lines = lines;
    }

    /// Get the visible lines from the buffer
    pub fn visible<'a>(&self, buffer: &'a StyledBuffer) -> &'a [BufferLine] {
        buffer.lines_range(self.offset, self.visible_lines)
    }

    /// Scroll up by the given number of lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.offset = self.offset.saturating_sub(lines);
    }

    /// Scroll down by the given number of lines, clamped to buffer size
    pub fn scroll_down(&mut self, lines: usize, buffer_line_count: usize) {
        let max_offset = buffer_line_count.saturating_sub(self.visible_lines);
        self.offset = (self.offset + lines).min(max_offset);
    }

    /// Scroll to the top of the buffer
    pub fn scroll_to_top(&mut self) {
        self.offset = 0;
    }

    /// Scroll to the bottom of the buffer
    pub fn scroll_to_bottom(&mut self, buffer_line_count: usize) {
        self.offset = buffer_line_count.saturating_sub(self.visible_lines);
    }

    /// Scroll by a page (visible_lines)
    pub fn page_up(&mut self) {
        self.scroll_up(self.visible_lines);
    }

    /// Scroll down by a page
    pub fn page_down(&mut self, buffer_line_count: usize) {
        self.scroll_down(self.visible_lines, buffer_line_count);
    }

    /// Whether the view is scrolled to the bottom
    pub fn at_bottom(&self, buffer_line_count: usize) -> bool {
        self.offset + self.visible_lines >= buffer_line_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buffer(lines: usize) -> StyledBuffer {
        let mut buf = StyledBuffer::new();
        for i in 0..lines {
            buf.push_text(&format!("line {i}"));
        }
        buf
    }

    #[test]
    fn test_new_view() {
        let view = BufferView::new(10);
        assert_eq!(view.offset(), 0);
        assert_eq!(view.visible_lines(), 10);
    }

    #[test]
    fn test_visible_lines() {
        let buf = make_buffer(20);
        let view = BufferView::new(5);
        let visible = view.visible(&buf);
        assert_eq!(visible.len(), 5);
        assert_eq!(visible[0].text(), "line 0");
    }

    #[test]
    fn test_scroll_down() {
        let buf = make_buffer(20);
        let mut view = BufferView::new(5);
        view.scroll_down(3, buf.line_count());
        assert_eq!(view.offset(), 3);
        let visible = view.visible(&buf);
        assert_eq!(visible[0].text(), "line 3");
    }

    #[test]
    fn test_scroll_up() {
        let buf = make_buffer(20);
        let mut view = BufferView::new(5);
        view.scroll_down(10, buf.line_count());
        view.scroll_up(3);
        assert_eq!(view.offset(), 7);
    }

    #[test]
    fn test_scroll_up_clamped() {
        let mut view = BufferView::new(5);
        view.scroll_up(100);
        assert_eq!(view.offset(), 0);
    }

    #[test]
    fn test_scroll_down_clamped() {
        let buf = make_buffer(10);
        let mut view = BufferView::new(5);
        view.scroll_down(100, buf.line_count());
        assert_eq!(view.offset(), 5);
    }

    #[test]
    fn test_scroll_to_top() {
        let buf = make_buffer(20);
        let mut view = BufferView::new(5);
        view.scroll_down(10, buf.line_count());
        view.scroll_to_top();
        assert_eq!(view.offset(), 0);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let buf = make_buffer(20);
        let mut view = BufferView::new(5);
        view.scroll_to_bottom(buf.line_count());
        assert_eq!(view.offset(), 15);
    }

    #[test]
    fn test_page_up_down() {
        let buf = make_buffer(30);
        let mut view = BufferView::new(10);
        view.page_down(buf.line_count());
        assert_eq!(view.offset(), 10);
        view.page_up();
        assert_eq!(view.offset(), 0);
    }

    #[test]
    fn test_at_bottom() {
        let buf = make_buffer(10);
        let mut view = BufferView::new(10);
        assert!(view.at_bottom(buf.line_count()));

        let buf2 = make_buffer(20);
        assert!(!view.at_bottom(buf2.line_count()));
        view.scroll_to_bottom(buf2.line_count());
        assert!(view.at_bottom(buf2.line_count()));
    }

    #[test]
    fn test_set_visible_lines() {
        let mut view = BufferView::new(10);
        view.set_visible_lines(20);
        assert_eq!(view.visible_lines(), 20);
    }

    #[test]
    fn test_small_buffer() {
        let buf = make_buffer(3);
        let view = BufferView::new(10);
        let visible = view.visible(&buf);
        assert_eq!(visible.len(), 3);
    }
}
