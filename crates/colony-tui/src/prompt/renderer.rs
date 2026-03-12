//! Render colony-ui-core components to ratatui

use colony_ui_core::{
    Color as UiColor, ComponentContext, ComponentOutput, PromptManager, Style as UiStyle,
};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Renders prompt components to ratatui widgets
pub struct PromptRenderer {
    manager: PromptManager,
}

impl PromptRenderer {
    pub fn new() -> Self {
        Self {
            manager: PromptManager::default(),
        }
    }

    pub fn with_layout(left: Vec<String>, right: Vec<String>) -> Self {
        Self {
            manager: PromptManager::new(left, right),
        }
    }

    /// Notify of a tuple space event for cache invalidation
    pub fn on_event(&mut self, event_type: &str) {
        self.manager.on_event(event_type);
    }

    /// Render the prompt as ratatui Line
    pub fn render(&self, ctx: &ComponentContext, width: u16) -> Line<'static> {
        let data = self.manager.render_data(ctx);

        let mut spans = Vec::new();

        // Render left components
        for (i, component) in data.left.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.extend(self.output_to_spans(&component.output));
        }

        // Add prompt indicator
        spans.push(Span::raw(" > "));

        // Calculate right side width
        let right_text: String = data
            .right
            .iter()
            .flat_map(|c| c.output.spans.iter().map(|s| s.text.as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        let right_width = right_text.len();

        // Add padding for right alignment
        let left_width: usize = spans.iter().map(|s| s.content.len()).sum();
        let padding = (width as usize).saturating_sub(left_width + right_width + 1);
        if padding > 0 {
            spans.push(Span::raw(" ".repeat(padding)));
        }

        // Render right components
        for (i, component) in data.right.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.extend(self.output_to_spans(&component.output));
        }

        Line::from(spans)
    }

    /// Convert ComponentOutput to ratatui Spans
    fn output_to_spans(&self, output: &ComponentOutput) -> Vec<Span<'static>> {
        output
            .spans
            .iter()
            .map(|span| {
                let style = self.convert_style(&span.style);
                Span::styled(span.text.clone(), style)
            })
            .collect()
    }

    /// Convert colony-ui-core style to ratatui style
    fn convert_style(&self, ui_style: &UiStyle) -> Style {
        let mut style = Style::default();

        if let Some(fg) = &ui_style.fg {
            style = style.fg(self.convert_color(fg));
        }
        if let Some(bg) = &ui_style.bg {
            style = style.bg(self.convert_color(bg));
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
    fn convert_color(&self, color: &UiColor) -> Color {
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

impl Default for PromptRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/home/test/project"),
            env: HashMap::from([("HOME".to_string(), "/home/test".to_string())]),
            ..Default::default()
        }
    }

    #[test]
    fn test_render_prompt() {
        let renderer = PromptRenderer::new();
        let ctx = test_context();
        let line = renderer.render(&ctx, 80);

        assert!(!line.spans.is_empty());
    }
}
