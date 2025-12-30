use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Rgb(u8, u8, u8),
}

impl Color {
    pub fn to_ansi_fg(&self) -> String {
        match self {
            Color::Black => "30".to_string(),
            Color::Red => "31".to_string(),
            Color::Green => "32".to_string(),
            Color::Yellow => "33".to_string(),
            Color::Blue => "34".to_string(),
            Color::Magenta => "35".to_string(),
            Color::Cyan => "36".to_string(),
            Color::White => "37".to_string(),
            Color::BrightBlack => "90".to_string(),
            Color::BrightRed => "91".to_string(),
            Color::BrightGreen => "92".to_string(),
            Color::BrightYellow => "93".to_string(),
            Color::BrightBlue => "94".to_string(),
            Color::BrightMagenta => "95".to_string(),
            Color::BrightCyan => "96".to_string(),
            Color::BrightWhite => "97".to_string(),
            Color::Rgb(r, g, b) => format!("38;2;{};{};{}", r, g, b),
        }
    }

    pub fn to_ansi_bg(&self) -> String {
        match self {
            Color::Black => "40".to_string(),
            Color::Red => "41".to_string(),
            Color::Green => "42".to_string(),
            Color::Yellow => "43".to_string(),
            Color::Blue => "44".to_string(),
            Color::Magenta => "45".to_string(),
            Color::Cyan => "46".to_string(),
            Color::White => "47".to_string(),
            Color::BrightBlack => "100".to_string(),
            Color::BrightRed => "101".to_string(),
            Color::BrightGreen => "102".to_string(),
            Color::BrightYellow => "103".to_string(),
            Color::BrightBlue => "104".to_string(),
            Color::BrightMagenta => "105".to_string(),
            Color::BrightCyan => "106".to_string(),
            Color::BrightWhite => "107".to_string(),
            Color::Rgb(r, g, b) => format!("48;2;{};{};{}", r, g, b),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
}

impl Style {
    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

impl StyledSpan {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComponentOutput {
    pub spans: Vec<StyledSpan>,
    pub data: Option<serde_json::Value>,
    pub visible: bool,
}

impl ComponentOutput {
    pub fn visible(spans: Vec<StyledSpan>) -> Self {
        Self {
            spans,
            data: None,
            visible: true,
        }
    }

    pub fn with_data(spans: Vec<StyledSpan>, data: serde_json::Value) -> Self {
        Self {
            spans,
            data: Some(data),
            visible: true,
        }
    }

    pub fn hidden() -> Self {
        Self {
            spans: vec![],
            data: None,
            visible: false,
        }
    }
}

pub trait PromptComponent: Send + Sync {
    fn id(&self) -> &str;
    fn render(&self, ctx: &crate::context::ComponentContext) -> ComponentOutput;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_default() {
        let style = Style::default();
        assert_eq!(style.fg, None);
        assert_eq!(style.bg, None);
        assert!(!style.bold);
        assert!(!style.italic);
        assert!(!style.dim);
    }

    #[test]
    fn test_styled_span_creation() {
        let span = StyledSpan::new("hello", Style::default());
        assert_eq!(span.text, "hello");
    }

    #[test]
    fn test_styled_span_with_style() {
        let span = StyledSpan::new("hello", Style::default().fg(Color::Cyan).bold());
        assert_eq!(span.style.fg, Some(Color::Cyan));
        assert!(span.style.bold);
    }

    #[test]
    fn test_component_output_visible() {
        let output = ComponentOutput::visible(vec![StyledSpan::new("test", Style::default())]);
        assert!(output.visible);
        assert_eq!(output.spans.len(), 1);
    }

    #[test]
    fn test_component_output_hidden() {
        let output = ComponentOutput::hidden();
        assert!(!output.visible);
        assert!(output.spans.is_empty());
    }
}
