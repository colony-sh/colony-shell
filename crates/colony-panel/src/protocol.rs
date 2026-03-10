//! Panel-as-process protocol types and frame parser.
//!
//! Panels are child processes that communicate with the host via a structured
//! JSON protocol on stdout/stdin. This module defines the wire types and an
//! async [`FrameParser`] that reads panel output and yields [`PanelMessage`]
//! variants.
//!
//! ## Sentinel lines
//!
//! - `---FRAME---` delimits rendered frames (opening and closing)
//! - `---CONFIG---` precedes a startup config JSON line
//! - `---EMIT---` precedes a tuple-space publication JSON line

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

// ---------------------------------------------------------------------------
// Sentinel constants
// ---------------------------------------------------------------------------

pub const SENTINEL_FRAME: &str = "---FRAME---";
pub const SENTINEL_CONFIG: &str = "---CONFIG---";
pub const SENTINEL_EMIT: &str = "---EMIT---";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur while parsing the panel protocol stream.
#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("unexpected EOF while reading protocol data")]
    UnexpectedEof,
}

// ---------------------------------------------------------------------------
// Style types
// ---------------------------------------------------------------------------

/// A parsed color value -- either a named color or an RGB hex triplet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleColor {
    Named(String),
    Hex(u8, u8, u8),
}

/// Parsed visual style specification.
///
/// A style string like `"bold,blue,italic"` is parsed into the individual
/// modifier flags and foreground color.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleSpec {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub fg: Option<StyleColor>,
}

impl StyleSpec {
    /// Parse a comma-separated style string.
    ///
    /// Recognized tokens:
    /// - `bold`, `dim`, `italic`, `underline` -- set modifier flags
    /// - `#rrggbb` -- set foreground to hex color
    /// - anything else -- set foreground to a named color
    pub fn parse(input: &str) -> Self {
        let mut spec = Self::default();
        for token in input.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            match token {
                "bold" => spec.bold = true,
                "dim" => spec.dim = true,
                "italic" => spec.italic = true,
                "underline" => spec.underline = true,
                hex if hex.starts_with('#') && hex.len() == 7 => {
                    let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(0);
                    let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(0);
                    let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(0);
                    spec.fg = Some(StyleColor::Hex(r, g, b));
                }
                named => {
                    spec.fg = Some(StyleColor::Named(named.to_owned()));
                }
            }
        }
        spec
    }

    /// Convert to a [`ratatui::style::Style`].
    pub fn to_ratatui_style(&self) -> ratatui::style::Style {
        use ratatui::style::{Color, Modifier, Style};

        let mut style = Style::default();

        if let Some(ref color) = self.fg {
            style = style.fg(match color {
                StyleColor::Named(name) => match name.as_str() {
                    "black" => Color::Black,
                    "red" => Color::Red,
                    "green" => Color::Green,
                    "yellow" => Color::Yellow,
                    "blue" => Color::Blue,
                    "magenta" => Color::Magenta,
                    "cyan" => Color::Cyan,
                    "white" => Color::White,
                    "gray" | "grey" => Color::Gray,
                    "dark_gray" | "dark_grey" => Color::DarkGray,
                    "light_red" => Color::LightRed,
                    "light_green" => Color::LightGreen,
                    "light_yellow" => Color::LightYellow,
                    "light_blue" => Color::LightBlue,
                    "light_magenta" => Color::LightMagenta,
                    "light_cyan" => Color::LightCyan,
                    _ => Color::Reset,
                },
                StyleColor::Hex(r, g, b) => Color::Rgb(*r, *g, *b),
            });
        }

        let mut modifiers = Modifier::empty();
        if self.bold {
            modifiers |= Modifier::BOLD;
        }
        if self.dim {
            modifiers |= Modifier::DIM;
        }
        if self.italic {
            modifiers |= Modifier::ITALIC;
        }
        if self.underline {
            modifiers |= Modifier::UNDERLINED;
        }
        if !modifiers.is_empty() {
            style = style.add_modifier(modifiers);
        }

        style
    }
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// A single line in a rendered frame.
///
/// Only `text` is required; all other fields are optional and skipped when
/// absent from JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrameLine {
    pub text: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent: Option<u16>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expandable: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
}

/// An input event sent from the host to a panel process via stdin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum InputEvent {
    Key {
        key: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modifiers: Vec<String>,
    },
    Resize {
        width: u16,
        height: u16,
    },
    Focus {
        focused: bool,
    },
    Tuple {
        data: serde_json::Value,
    },
    Shutdown,
}

/// Startup configuration sent by a panel after launch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PanelConfig {
    #[serde(default)]
    pub subscriptions: Vec<String>,
}

/// A tuple-space publication emitted by a panel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TupleEmit {
    pub key: String,
    pub value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Panel message
// ---------------------------------------------------------------------------

/// A parsed message from a panel process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelMessage {
    /// Startup configuration.
    Config(PanelConfig),
    /// A complete rendered frame (list of lines).
    Frame(Vec<FrameLine>),
    /// A tuple-space publication.
    Emit(TupleEmit),
}

// ---------------------------------------------------------------------------
// Frame parser
// ---------------------------------------------------------------------------

/// Async reader that consumes a panel's stdout and yields [`PanelMessage`]s.
///
/// The parser is a simple state machine that watches for sentinel lines and
/// accumulates frame lines between matching `---FRAME---` pairs.
pub struct FrameParser<R> {
    reader: R,
    line_buf: String,
    frame_lines: Vec<FrameLine>,
    in_frame: bool,
}

impl<R: AsyncBufRead + Unpin> FrameParser<R> {
    /// Create a new parser wrapping the given reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line_buf: String::new(),
            frame_lines: Vec::new(),
            in_frame: false,
        }
    }

    /// Read the next message from the panel, or `None` on EOF.
    pub async fn next_message(&mut self) -> Result<Option<PanelMessage>, ProtocolError> {
        loop {
            self.line_buf.clear();
            let bytes_read = self.reader.read_line(&mut self.line_buf).await?;
            if bytes_read == 0 {
                // EOF
                return Ok(None);
            }

            let line = self.line_buf.trim_end();

            match line {
                SENTINEL_FRAME => {
                    if self.in_frame {
                        // Closing frame sentinel -- yield accumulated frame
                        self.in_frame = false;
                        let frame = std::mem::take(&mut self.frame_lines);
                        return Ok(Some(PanelMessage::Frame(frame)));
                    }
                    // Opening frame sentinel -- start accumulating
                    self.in_frame = true;
                    self.frame_lines.clear();
                }
                SENTINEL_CONFIG => {
                    // Next line is config JSON
                    self.line_buf.clear();
                    let bytes_read = self.reader.read_line(&mut self.line_buf).await?;
                    if bytes_read == 0 {
                        return Err(ProtocolError::UnexpectedEof);
                    }
                    let config: PanelConfig = serde_json::from_str(self.line_buf.trim_end())?;
                    return Ok(Some(PanelMessage::Config(config)));
                }
                SENTINEL_EMIT => {
                    // Next line is emit JSON
                    self.line_buf.clear();
                    let bytes_read = self.reader.read_line(&mut self.line_buf).await?;
                    if bytes_read == 0 {
                        return Err(ProtocolError::UnexpectedEof);
                    }
                    let emit: TupleEmit = serde_json::from_str(self.line_buf.trim_end())?;
                    return Ok(Some(PanelMessage::Emit(emit)));
                }
                other => {
                    if self.in_frame {
                        let frame_line: FrameLine = serde_json::from_str(other)?;
                        self.frame_lines.push(frame_line);
                    }
                    // Lines outside a frame are ignored (debug output)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Serialization / deserialization tests --

    #[test]
    fn deserialize_minimal_frame_line() {
        let json = r#"{"text":"hello"}"#;
        let line: FrameLine = serde_json::from_str(json).unwrap();
        assert_eq!(line.text, "hello");
        assert_eq!(line.icon, None);
        assert_eq!(line.style, None);
        assert_eq!(line.indent, None);
        assert_eq!(line.action, None);
        assert_eq!(line.expandable, None);
        assert_eq!(line.expanded, None);
    }

    #[test]
    fn deserialize_full_frame_line() {
        let json = r#"{
            "text": "src/main.rs",
            "icon": "\uf0f6",
            "style": "bold,blue",
            "indent": 2,
            "action": "open:src/main.rs",
            "expandable": true,
            "expanded": false
        }"#;
        let line: FrameLine = serde_json::from_str(json).unwrap();
        assert_eq!(line.text, "src/main.rs");
        assert_eq!(line.icon.as_deref(), Some("\u{f0f6}")); // U+F0F6
        assert_eq!(line.style.as_deref(), Some("bold,blue"));
        assert_eq!(line.indent, Some(2));
        assert_eq!(line.action.as_deref(), Some("open:src/main.rs"));
        assert_eq!(line.expandable, Some(true));
        assert_eq!(line.expanded, Some(false));
    }

    #[test]
    fn serialize_input_event_key() {
        let event = InputEvent::Key {
            key: "Enter".into(),
            modifiers: vec!["ctrl".into()],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"key""#));
        assert!(json.contains(r#""key":"Enter""#));
        assert!(json.contains(r#""modifiers":["ctrl"]"#));
    }

    #[test]
    fn serialize_input_event_resize() {
        let event = InputEvent::Resize {
            width: 80,
            height: 24,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"resize""#));
        assert!(json.contains(r#""width":80"#));
        assert!(json.contains(r#""height":24"#));
    }

    #[test]
    fn deserialize_panel_config() {
        let json = r#"{"subscriptions":["git_status","file_change"]}"#;
        let config: PanelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.subscriptions, vec!["git_status", "file_change"]);
    }

    #[test]
    fn deserialize_tuple_emit() {
        let json = r#"{"key":"git_status","value":{"branch":"main","dirty":true}}"#;
        let emit: TupleEmit = serde_json::from_str(json).unwrap();
        assert_eq!(emit.key, "git_status");
        assert_eq!(emit.value["branch"], "main");
        assert_eq!(emit.value["dirty"], true);
    }

    // -- Style parsing tests --

    #[test]
    fn parse_style_bold_blue_italic() {
        let spec = StyleSpec::parse("bold,blue,italic");
        assert!(spec.bold);
        assert!(spec.italic);
        assert!(!spec.dim);
        assert!(!spec.underline);
        assert_eq!(spec.fg, Some(StyleColor::Named("blue".into())));
    }

    #[test]
    fn parse_style_hex_dim() {
        let spec = StyleSpec::parse("#ff0000,dim");
        assert!(spec.dim);
        assert!(!spec.bold);
        assert_eq!(spec.fg, Some(StyleColor::Hex(255, 0, 0)));
    }

    #[test]
    fn style_spec_to_ratatui() {
        use ratatui::style::{Color, Modifier};

        let spec = StyleSpec::parse("bold,blue");
        let style = spec.to_ratatui_style();
        assert_eq!(style.fg, Some(Color::Blue));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    // -- Async frame parser tests --

    fn make_reader(input: &str) -> tokio::io::BufReader<&[u8]> {
        tokio::io::BufReader::new(input.as_bytes())
    }

    #[tokio::test]
    async fn parse_single_frame() {
        let input = "---FRAME---\n{\"text\":\"hello\"}\n{\"text\":\"world\"}\n---FRAME---\n";
        let mut parser = FrameParser::new(make_reader(input));

        let msg = parser.next_message().await.unwrap().unwrap();
        let PanelMessage::Frame(lines) = msg else {
            panic!("expected Frame, got {msg:?}");
        };
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "hello");
        assert_eq!(lines[1].text, "world");
    }

    #[tokio::test]
    async fn parse_config_then_frame() {
        let input = "---CONFIG---\n{\"subscriptions\":[\"git\"]}\n---FRAME---\n{\"text\":\"line1\"}\n---FRAME---\n";
        let mut parser = FrameParser::new(make_reader(input));

        let msg1 = parser.next_message().await.unwrap().unwrap();
        let PanelMessage::Config(config) = msg1 else {
            panic!("expected Config, got {msg1:?}");
        };
        assert_eq!(config.subscriptions, vec!["git"]);

        let msg2 = parser.next_message().await.unwrap().unwrap();
        let PanelMessage::Frame(lines) = msg2 else {
            panic!("expected Frame, got {msg2:?}");
        };
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "line1");
    }

    #[tokio::test]
    async fn parse_emit() {
        let input = "---EMIT---\n{\"key\":\"status\",\"value\":42}\n";
        let mut parser = FrameParser::new(make_reader(input));

        let msg = parser.next_message().await.unwrap().unwrap();
        let PanelMessage::Emit(emit) = msg else {
            panic!("expected Emit, got {msg:?}");
        };
        assert_eq!(emit.key, "status");
        assert_eq!(emit.value, serde_json::json!(42));
    }

    #[tokio::test]
    async fn eof_returns_none() {
        let input = "";
        let mut parser = FrameParser::new(make_reader(input));

        let msg = parser.next_message().await.unwrap();
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn debug_lines_outside_frame_are_ignored() {
        let input = "some debug output\nanother debug line\n---FRAME---\n{\"text\":\"ok\"}\n---FRAME---\n";
        let mut parser = FrameParser::new(make_reader(input));

        let msg = parser.next_message().await.unwrap().unwrap();
        let PanelMessage::Frame(lines) = msg else {
            panic!("expected Frame, got {msg:?}");
        };
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "ok");
    }
}
