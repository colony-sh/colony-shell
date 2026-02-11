//! Renders prompt components into ratatui widgets
//!
//! Takes `PromptManager` output (`PromptData`) and converts to
//! ratatui `Line`/`Span` types for display in the status line area.

use colony_ui_core::component::{Color as UiColor, ComponentOutput, Style as UiStyle};
use colony_ui_core::manager::PromptData;
use ratatui::style::{Color as RatColor, Modifier, Style as RatStyle};
use ratatui::text::{Line, Span};

/// Renders prompt component data into ratatui text primitives
pub struct PromptRenderer;

impl PromptRenderer {
    /// Convert a full PromptData (left + right) into a pair of ratatui Lines
    pub fn render(data: &PromptData) -> (Line<'static>, Line<'static>) {
        let left_spans: Vec<Span<'static>> = data
            .left
            .iter()
            .flat_map(|cd| render_component_output(&cd.output))
            .collect();

        let right_spans: Vec<Span<'static>> = data
            .right
            .iter()
            .flat_map(|cd| render_component_output(&cd.output))
            .collect();

        (Line::from(left_spans), Line::from(right_spans))
    }

    /// Render a single ComponentOutput into ratatui Spans
    pub fn render_single(output: &ComponentOutput) -> Vec<Span<'static>> {
        if !output.visible {
            return Vec::new();
        }
        render_component_output(output)
    }
}

fn render_component_output(output: &ComponentOutput) -> Vec<Span<'static>> {
    if !output.visible {
        return Vec::new();
    }

    let mut spans: Vec<Span<'static>> = output
        .spans
        .iter()
        .map(|styled_span| {
            let style = map_style(&styled_span.style);
            Span::styled(styled_span.text.clone(), style)
        })
        .collect();

    // Add separator after each component
    if !spans.is_empty() {
        spans.push(Span::raw("  "));
    }

    spans
}

/// Map a colony-ui-core Color to a ratatui Color
pub fn map_color(color: &UiColor) -> RatColor {
    match color {
        UiColor::Black => RatColor::Black,
        UiColor::Red => RatColor::Red,
        UiColor::Green => RatColor::Green,
        UiColor::Yellow => RatColor::Yellow,
        UiColor::Blue => RatColor::Blue,
        UiColor::Magenta => RatColor::Magenta,
        UiColor::Cyan => RatColor::Cyan,
        UiColor::White => RatColor::White,
        UiColor::BrightBlack => RatColor::DarkGray,
        UiColor::BrightRed => RatColor::LightRed,
        UiColor::BrightGreen => RatColor::LightGreen,
        UiColor::BrightYellow => RatColor::LightYellow,
        UiColor::BrightBlue => RatColor::LightBlue,
        UiColor::BrightMagenta => RatColor::LightMagenta,
        UiColor::BrightCyan => RatColor::LightCyan,
        UiColor::BrightWhite => RatColor::White,
        UiColor::Rgb(r, g, b) => RatColor::Rgb(*r, *g, *b),
    }
}

/// Map a colony-ui-core Style to a ratatui Style
pub fn map_style(style: &UiStyle) -> RatStyle {
    let mut rat_style = RatStyle::default();

    if let Some(ref fg) = style.fg {
        rat_style = rat_style.fg(map_color(fg));
    }
    if let Some(ref bg) = style.bg {
        rat_style = rat_style.bg(map_color(bg));
    }
    if style.bold {
        rat_style = rat_style.add_modifier(Modifier::BOLD);
    }
    if style.italic {
        rat_style = rat_style.add_modifier(Modifier::ITALIC);
    }
    if style.dim {
        rat_style = rat_style.add_modifier(Modifier::DIM);
    }

    rat_style
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_ui_core::component::{ComponentOutput, StyledSpan};

    #[test]
    fn test_map_color_basic() {
        assert_eq!(map_color(&UiColor::Red), RatColor::Red);
        assert_eq!(map_color(&UiColor::Green), RatColor::Green);
        assert_eq!(map_color(&UiColor::Blue), RatColor::Blue);
        assert_eq!(map_color(&UiColor::Cyan), RatColor::Cyan);
        assert_eq!(map_color(&UiColor::Yellow), RatColor::Yellow);
        assert_eq!(map_color(&UiColor::Magenta), RatColor::Magenta);
        assert_eq!(map_color(&UiColor::White), RatColor::White);
        assert_eq!(map_color(&UiColor::Black), RatColor::Black);
    }

    #[test]
    fn test_map_color_bright() {
        assert_eq!(map_color(&UiColor::BrightBlack), RatColor::DarkGray);
        assert_eq!(map_color(&UiColor::BrightRed), RatColor::LightRed);
        assert_eq!(map_color(&UiColor::BrightGreen), RatColor::LightGreen);
        assert_eq!(map_color(&UiColor::BrightYellow), RatColor::LightYellow);
        assert_eq!(map_color(&UiColor::BrightBlue), RatColor::LightBlue);
        assert_eq!(map_color(&UiColor::BrightMagenta), RatColor::LightMagenta);
        assert_eq!(map_color(&UiColor::BrightCyan), RatColor::LightCyan);
        assert_eq!(map_color(&UiColor::BrightWhite), RatColor::White);
    }

    #[test]
    fn test_map_color_rgb() {
        assert_eq!(map_color(&UiColor::Rgb(128, 64, 32)), RatColor::Rgb(128, 64, 32));
    }

    #[test]
    fn test_map_style_empty() {
        let ui_style = UiStyle::default();
        let rat_style = map_style(&ui_style);
        assert_eq!(rat_style, RatStyle::default());
    }

    #[test]
    fn test_map_style_fg_color() {
        let ui_style = UiStyle::default().fg(UiColor::Cyan);
        let rat_style = map_style(&ui_style);
        assert_eq!(rat_style.fg, Some(RatColor::Cyan));
    }

    #[test]
    fn test_map_style_bg_color() {
        let ui_style = UiStyle::default().bg(UiColor::Red);
        let rat_style = map_style(&ui_style);
        assert_eq!(rat_style.bg, Some(RatColor::Red));
    }

    #[test]
    fn test_map_style_bold() {
        let ui_style = UiStyle::default().bold();
        let rat_style = map_style(&ui_style);
        assert!(rat_style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_map_style_italic() {
        let ui_style = UiStyle::default().italic();
        let rat_style = map_style(&ui_style);
        assert!(rat_style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_map_style_dim() {
        let ui_style = UiStyle::default().dim();
        let rat_style = map_style(&ui_style);
        assert!(rat_style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn test_map_style_combined() {
        let ui_style = UiStyle::default()
            .fg(UiColor::Green)
            .bg(UiColor::Black)
            .bold()
            .italic();
        let rat_style = map_style(&ui_style);
        assert_eq!(rat_style.fg, Some(RatColor::Green));
        assert_eq!(rat_style.bg, Some(RatColor::Black));
        assert!(rat_style.add_modifier.contains(Modifier::BOLD));
        assert!(rat_style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_render_single_hidden() {
        let output = ComponentOutput::hidden();
        let spans = PromptRenderer::render_single(&output);
        assert!(spans.is_empty());
    }

    #[test]
    fn test_render_single_visible() {
        let output = ComponentOutput::visible(vec![
            StyledSpan::new("hello", UiStyle::default().fg(UiColor::Cyan)),
        ]);
        let spans = PromptRenderer::render_single(&output);
        // 1 content span + 1 separator
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn test_render_prompt_data() {
        let data = PromptData {
            left: vec![colony_ui_core::manager::ComponentData {
                id: "test".to_string(),
                output: ComponentOutput::visible(vec![StyledSpan::plain("left")]),
            }],
            right: vec![colony_ui_core::manager::ComponentData {
                id: "test2".to_string(),
                output: ComponentOutput::visible(vec![StyledSpan::plain("right")]),
            }],
        };

        let (left_line, right_line) = PromptRenderer::render(&data);
        assert!(!left_line.spans.is_empty());
        assert!(!right_line.spans.is_empty());
    }
}
