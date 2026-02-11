//! Styled text spans
//!
//! A `StyledSpan` represents a contiguous run of text with uniform styling.

use serde::{Deserialize, Serialize};

/// Terminal color
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    Default,
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
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// Parse from SGR color parameter
    pub fn from_sgr(code: u8) -> Self {
        match code {
            0 | 39 => Self::Default,
            30 => Self::Black,
            31 => Self::Red,
            32 => Self::Green,
            33 => Self::Yellow,
            34 => Self::Blue,
            35 => Self::Magenta,
            36 => Self::Cyan,
            37 => Self::White,
            90 => Self::BrightBlack,
            91 => Self::BrightRed,
            92 => Self::BrightGreen,
            93 => Self::BrightYellow,
            94 => Self::BrightBlue,
            95 => Self::BrightMagenta,
            96 => Self::BrightCyan,
            97 => Self::BrightWhite,
            _ => Self::Default,
        }
    }
}

/// Text style attributes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StyleFlags {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dim: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blink: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inverse: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strikethrough: bool,
}

/// A span of styled text within a buffer line
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyledSpan {
    pub text: String,
    #[serde(default = "default_color", skip_serializing_if = "is_default_color")]
    pub fg: Color,
    #[serde(default = "default_color", skip_serializing_if = "is_default_color")]
    pub bg: Color,
    #[serde(default, skip_serializing_if = "is_default_style")]
    pub style: StyleFlags,
}

fn default_color() -> Color {
    Color::Default
}

fn is_default_color(c: &Color) -> bool {
    *c == Color::Default
}

fn is_default_style(s: &StyleFlags) -> bool {
    *s == StyleFlags::default()
}

impl StyledSpan {
    /// Create a plain text span with no styling
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fg: Color::Default,
            bg: Color::Default,
            style: StyleFlags::default(),
        }
    }

    /// Create a styled span with foreground color
    pub fn colored(text: impl Into<String>, fg: Color) -> Self {
        Self {
            text: text.into(),
            fg,
            bg: Color::Default,
            style: StyleFlags::default(),
        }
    }

    /// Number of characters in this span
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Whether this span is empty
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_span() {
        let span = StyledSpan::plain("hello");
        assert_eq!(span.text, "hello");
        assert_eq!(span.fg, Color::Default);
        assert_eq!(span.bg, Color::Default);
        assert!(!span.style.bold);
    }

    #[test]
    fn test_colored_span() {
        let span = StyledSpan::colored("error", Color::Red);
        assert_eq!(span.fg, Color::Red);
        assert_eq!(span.bg, Color::Default);
    }

    #[test]
    fn test_span_len() {
        let span = StyledSpan::plain("hello");
        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());
    }

    #[test]
    fn test_empty_span() {
        let span = StyledSpan::plain("");
        assert!(span.is_empty());
    }

    #[test]
    fn test_color_from_sgr() {
        assert_eq!(Color::from_sgr(31), Color::Red);
        assert_eq!(Color::from_sgr(32), Color::Green);
        assert_eq!(Color::from_sgr(97), Color::BrightWhite);
        assert_eq!(Color::from_sgr(0), Color::Default);
    }

    #[test]
    fn test_span_serialization() {
        let span = StyledSpan::plain("test");
        let json = serde_json::to_string(&span).unwrap();
        assert!(json.contains("\"text\":\"test\""));
        assert!(!json.contains("\"fg\"")); // Default color should be skipped
    }

    #[test]
    fn test_styled_span_serialization() {
        let mut span = StyledSpan::colored("bold", Color::Green);
        span.style.bold = true;
        let json = serde_json::to_string(&span).unwrap();
        assert!(json.contains("\"fg\":\"green\""));
        assert!(json.contains("\"bold\":true"));
    }

    #[test]
    fn test_style_flags_default() {
        let flags = StyleFlags::default();
        assert!(!flags.bold);
        assert!(!flags.dim);
        assert!(!flags.italic);
        assert!(!flags.underline);
        assert!(!flags.blink);
        assert!(!flags.inverse);
        assert!(!flags.strikethrough);
    }
}
