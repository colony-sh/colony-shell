//! Render colony-ui-core sidebars to ratatui widgets

use colony_ui_core::{
    Color as UiColor, ComponentLine, SidebarData, SidebarOutput, SidebarPanelData, Style as UiStyle,
};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Renders sidebar panels to ratatui widgets
pub struct SidebarRenderer;

impl SidebarRenderer {
    /// Render a complete sidebar
    ///
    /// `selected_index` highlights the line at that index when focused.
    /// `scroll_offset` controls horizontal text scrolling for the highlighted line
    /// when its content is wider than the panel.
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        data: &SidebarData,
        focused: bool,
        selected_index: Option<usize>,
        scroll_offset: usize,
    ) {
        if !data.visible || area.width == 0 {
            return;
        }

        let border_style = if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Main sidebar block with opaque background
        let bg = Color::Rgb(20, 20, 35);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(Style::default().bg(bg));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Render panels
        let mut y_offset = 0u16;
        for (panel_idx, panel) in data.panels.iter().enumerate() {
            if y_offset >= inner.height {
                break;
            }

            let panel_height = Self::calculate_panel_height(panel, inner.height - y_offset);
            let panel_area = Rect {
                x: inner.x,
                y: inner.y + y_offset,
                width: inner.width,
                height: panel_height,
            };

            let panel_selected = if panel_idx == 0 && focused {
                selected_index
            } else {
                None
            };
            Self::render_panel(frame, panel_area, panel, panel_selected, scroll_offset);
            y_offset += panel_height;
        }
    }

    /// Calculate height needed for a panel
    fn calculate_panel_height(panel: &SidebarPanelData, max_height: u16) -> u16 {
        if panel.collapsed {
            1 // Just the header
        } else {
            let content_lines = panel.output.lines.len() as u16;
            (1 + content_lines).min(max_height) // Header + content
        }
    }

    /// Render a single panel
    fn render_panel(
        frame: &mut Frame,
        area: Rect,
        panel: &SidebarPanelData,
        selected_index: Option<usize>,
        scroll_offset: usize,
    ) {
        if area.height == 0 {
            return;
        }

        // Panel header
        let collapse_icon = if panel.collapsed { " " } else { " " };
        let header = Line::from(vec![
            Span::styled(collapse_icon, Style::default().fg(Color::DarkGray)),
            Span::styled(
                panel.title.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let header_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(vec![header]), header_area);

        // Panel content (if not collapsed)
        if !panel.collapsed && area.height > 1 {
            let content_area = Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: area.height - 1,
            };

            Self::render_panel_content(frame, content_area, &panel.output, selected_index, scroll_offset);
        }
    }

    /// Render panel content lines
    fn render_panel_content(
        frame: &mut Frame,
        area: Rect,
        output: &SidebarOutput,
        selected_index: Option<usize>,
        scroll_offset: usize,
    ) {
        let lines: Vec<Line> = output
            .lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let highlighted = selected_index == Some(i);
                let offset = if highlighted { scroll_offset } else { 0 };
                Self::component_line_to_ratatui(line, area.width as usize, highlighted, offset)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    /// Convert a ComponentLine to a ratatui Line
    ///
    /// When `highlighted` is true, all spans get the REVERSED modifier for cursor visibility.
    /// `scroll_offset` shifts the visible window right for highlighted lines that overflow,
    /// creating a horizontal scroll effect for long names.
    fn component_line_to_ratatui(
        line: &ComponentLine,
        max_width: usize,
        highlighted: bool,
        scroll_offset: usize,
    ) -> Line<'static> {
        let mut spans = Vec::new();

        // Indentation
        if line.indent > 0 {
            spans.push(Span::raw("  ".repeat(line.indent)));
        }

        // Icon
        if let Some(icon) = &line.icon {
            spans.push(Span::styled(
                format!("{} ", icon),
                Style::default().fg(Color::DarkGray),
            ));
        }

        // Expand/collapse indicator for tree nodes
        if line.expandable {
            let indicator = if line.expanded { " " } else { " " };
            spans.push(Span::styled(
                indicator,
                Style::default().fg(Color::DarkGray),
            ));
        }

        // Content spans
        for span in &line.spans {
            spans.push(Span::styled(
                span.text.clone(),
                Self::convert_style(&span.style),
            ));
        }

        // Check if truncation needed
        let total_len: usize = spans.iter().map(|s| s.content.len()).sum();
        if total_len > max_width {
            if highlighted && scroll_offset > 0 {
                // Scrolling mode: shift the visible window right
                return Self::scroll_spans(spans, max_width, scroll_offset, total_len);
            }

            // Static truncation for non-highlighted or offset=0
            let mut remaining = max_width.saturating_sub(3); // Reserve for "..."
            let mut truncated = Vec::new();

            for span in spans {
                if remaining == 0 {
                    break;
                }
                let len = span.content.len();
                if len <= remaining {
                    truncated.push(span);
                    remaining -= len;
                } else {
                    let truncated_text: String = span.content.chars().take(remaining).collect();
                    truncated.push(Span::styled(truncated_text, span.style));
                    remaining = 0;
                }
            }

            truncated.push(Span::styled("...", Style::default().fg(Color::DarkGray)));
            if highlighted {
                truncated = truncated
                    .into_iter()
                    .map(|s| Span::styled(s.content, s.style.add_modifier(Modifier::REVERSED)))
                    .collect();
            }
            return Line::from(truncated);
        }

        if highlighted {
            // Pad to full width so the REVERSED highlight fills the line
            let pad = max_width.saturating_sub(total_len);
            let mut result: Vec<Span<'static>> = spans
                .into_iter()
                .map(|s| Span::styled(s.content, s.style.add_modifier(Modifier::REVERSED)))
                .collect();
            if pad > 0 {
                result.push(Span::styled(
                    " ".repeat(pad),
                    Style::default().add_modifier(Modifier::REVERSED),
                ));
            }
            return Line::from(result);
        }

        Line::from(spans)
    }

    /// Apply horizontal scroll offset to spans, showing a sliding window of the text.
    fn scroll_spans(
        spans: Vec<Span<'static>>,
        max_width: usize,
        scroll_offset: usize,
        total_len: usize,
    ) -> Line<'static> {
        let max_scroll = total_len.saturating_sub(max_width);
        let offset = scroll_offset.min(max_scroll);
        let visible_end = offset + max_width;

        let mut result = Vec::new();
        let mut pos = 0usize; // current position in the flattened text

        for span in spans {
            let span_len = span.content.len();
            let span_end = pos + span_len;

            // Skip spans entirely before the visible window
            if span_end <= offset {
                pos = span_end;
                continue;
            }
            // Stop if we're past the visible window
            if pos >= visible_end {
                break;
            }

            // Calculate the visible slice of this span
            let slice_start = if offset > pos { offset - pos } else { 0 };
            let slice_end = if visible_end < span_end {
                visible_end - pos
            } else {
                span_len
            };

            let chars: Vec<char> = span.content.chars().collect();
            let sliced: String = chars[slice_start..slice_end.min(chars.len())]
                .iter()
                .collect();
            if !sliced.is_empty() {
                result.push(Span::styled(
                    sliced,
                    span.style.add_modifier(Modifier::REVERSED),
                ));
            }

            pos = span_end;
        }

        Line::from(result)
    }

    /// Convert colony-ui-core style to ratatui style
    fn convert_style(ui_style: &UiStyle) -> Style {
        let mut style = Style::default();

        if let Some(fg) = &ui_style.fg {
            style = style.fg(Self::convert_color(fg));
        }
        if let Some(bg) = &ui_style.bg {
            style = style.bg(Self::convert_color(bg));
        }
        if ui_style.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if ui_style.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if ui_style.dim {
            style = style.add_modifier(Modifier::DIM);
        }

        style
    }

    /// Convert colony-ui-core color to ratatui color
    fn convert_color(color: &UiColor) -> Color {
        match color {
            UiColor::Black => Color::Black,
            UiColor::Red => Color::Red,
            UiColor::Green => Color::Green,
            UiColor::Yellow => Color::Yellow,
            UiColor::Blue => Color::Blue,
            UiColor::Magenta => Color::Magenta,
            UiColor::Cyan => Color::Cyan,
            UiColor::White => Color::White,
            UiColor::BrightBlack => Color::DarkGray,
            UiColor::BrightRed => Color::LightRed,
            UiColor::BrightGreen => Color::LightGreen,
            UiColor::BrightYellow => Color::LightYellow,
            UiColor::BrightBlue => Color::LightBlue,
            UiColor::BrightMagenta => Color::LightMagenta,
            UiColor::BrightCyan => Color::LightCyan,
            UiColor::BrightWhite => Color::White,
            UiColor::Rgb(r, g, b) => Color::Rgb(*r, *g, *b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_ui_core::{ComponentLine, StyledSpan};

    #[test]
    fn test_component_line_to_ratatui() {
        let line = ComponentLine::new(vec![StyledSpan::plain("hello")]);
        let ratatui_line = SidebarRenderer::component_line_to_ratatui(&line, 80, false, 0);
        assert!(!ratatui_line.spans.is_empty());
    }

    #[test]
    fn test_component_line_with_indent() {
        let line = ComponentLine::new(vec![StyledSpan::plain("indented")]).with_indent(2);
        let ratatui_line = SidebarRenderer::component_line_to_ratatui(&line, 80, false, 0);
        // Should have indent + content
        assert!(ratatui_line.spans.len() >= 1);
    }

    #[test]
    fn test_component_line_highlight() {
        let line = ComponentLine::new(vec![StyledSpan::plain("selected")]);
        let normal = SidebarRenderer::component_line_to_ratatui(&line, 80, false, 0);
        let highlighted = SidebarRenderer::component_line_to_ratatui(&line, 80, true, 0);

        // Highlighted line should have REVERSED modifier
        let has_reverse = highlighted
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::REVERSED));
        assert!(has_reverse, "highlighted line should have REVERSED modifier");

        // Normal line should not
        let has_reverse_normal = normal
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::REVERSED));
        assert!(
            !has_reverse_normal,
            "normal line should not have REVERSED modifier"
        );
    }
}
