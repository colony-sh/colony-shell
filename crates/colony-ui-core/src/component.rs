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
pub struct InteractiveConfig {
    pub click: String,
    pub hover: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComponentOutput {
    pub spans: Vec<StyledSpan>,
    pub data: Option<serde_json::Value>,
    pub visible: bool,
    pub interactive: Option<InteractiveConfig>,
}

impl ComponentOutput {
    pub fn visible(spans: Vec<StyledSpan>) -> Self {
        Self {
            spans,
            data: None,
            visible: true,
            interactive: None,
        }
    }

    pub fn with_data(spans: Vec<StyledSpan>, data: serde_json::Value) -> Self {
        Self {
            spans,
            data: Some(data),
            visible: true,
            interactive: None,
        }
    }

    pub fn hidden() -> Self {
        Self {
            spans: vec![],
            data: None,
            visible: false,
            interactive: None,
        }
    }

    pub fn with_interactive(mut self, click: impl Into<String>, hover: Option<String>) -> Self {
        self.interactive = Some(InteractiveConfig {
            click: click.into(),
            hover,
        });
        self
    }
}

use colony_tuple::Value;

/// Tuple patterns a component subscribes to for cache invalidation
pub type TuplePattern = Vec<Value>;

pub trait PromptComponent: Send + Sync {
    fn id(&self) -> &str;
    fn subscriptions(&self) -> Vec<TuplePattern>;
    fn render(&self, ctx: &crate::context::ComponentContext) -> ComponentOutput;
}

/// Position of a sidebar panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidebarPosition {
    Left,
    Right,
}

/// A single line in sidebar output with optional icon, styled text, and metadata
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarLine {
    pub icon: Option<StyledSpan>,
    pub text: StyledSpan,
    pub metadata: Option<StyledSpan>,
    pub indent: u16,
}

impl SidebarLine {
    pub fn new(text: StyledSpan) -> Self {
        Self {
            icon: None,
            text,
            metadata: None,
            indent: 0,
        }
    }

    pub fn with_icon(mut self, icon: StyledSpan) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_metadata(mut self, metadata: StyledSpan) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_indent(mut self, indent: u16) -> Self {
        self.indent = indent;
        self
    }
}

/// Output from a sidebar component: structured lines
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SidebarOutput {
    pub lines: Vec<SidebarLine>,
    pub visible: bool,
    pub data: Option<serde_json::Value>,
}

impl SidebarOutput {
    pub fn visible(lines: Vec<SidebarLine>) -> Self {
        Self {
            lines,
            visible: true,
            data: None,
        }
    }

    pub fn with_data(lines: Vec<SidebarLine>, data: serde_json::Value) -> Self {
        Self {
            lines,
            visible: true,
            data: Some(data),
        }
    }

    pub fn hidden() -> Self {
        Self {
            lines: vec![],
            visible: false,
            data: None,
        }
    }
}

/// Trait for sidebar panel components
pub trait SidebarComponent: Send + Sync {
    /// Unique identifier for this sidebar panel
    fn id(&self) -> &str;

    /// Display title for the panel header
    fn title(&self) -> &str;

    /// Preferred sidebar position
    fn position(&self) -> SidebarPosition {
        SidebarPosition::Right
    }

    /// Whether this panel can be collapsed
    fn collapsible(&self) -> bool {
        true
    }

    /// Tuple patterns this component subscribes to for cache invalidation
    fn subscriptions(&self) -> Vec<TuplePattern>;

    /// Render the sidebar panel content
    fn render(&self, ctx: &crate::context::ComponentContext) -> SidebarOutput;
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

    #[test]
    fn test_sidebar_line_construction() {
        let line = SidebarLine::new(StyledSpan::plain("file.rs"))
            .with_icon(StyledSpan::new("M", Style::default().fg(Color::Yellow)))
            .with_metadata(StyledSpan::new("+3 -1", Style::default().dim()))
            .with_indent(2);

        assert_eq!(line.text.text, "file.rs");
        assert!(line.icon.is_some());
        assert!(line.metadata.is_some());
        assert_eq!(line.indent, 2);
    }

    #[test]
    fn test_sidebar_output_visible() {
        let lines = vec![
            SidebarLine::new(StyledSpan::plain("line 1")),
            SidebarLine::new(StyledSpan::plain("line 2")),
        ];
        let output = SidebarOutput::visible(lines);
        assert!(output.visible);
        assert_eq!(output.lines.len(), 2);
    }

    #[test]
    fn test_sidebar_output_hidden() {
        let output = SidebarOutput::hidden();
        assert!(!output.visible);
        assert!(output.lines.is_empty());
    }

    #[test]
    fn test_sidebar_output_with_data() {
        let lines = vec![SidebarLine::new(StyledSpan::plain("test"))];
        let output = SidebarOutput::with_data(lines, serde_json::json!({"key": "value"}));
        assert!(output.visible);
        assert!(output.data.is_some());
    }

    #[test]
    fn test_sidebar_position_variants() {
        assert_eq!(SidebarPosition::Left, SidebarPosition::Left);
        assert_ne!(SidebarPosition::Left, SidebarPosition::Right);
    }
}
