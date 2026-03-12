//! ANSI escape sequence parser
//!
//! Converts raw terminal output containing ANSI escape sequences into
//! `StyledSpan` sequences suitable for `StyledBuffer` lines.

use super::buffer::BufferLine;
use super::span::{Color, StyleFlags, StyledSpan};

/// Current style state tracked across ANSI parsing
#[derive(Debug, Clone, Default)]
struct StyleState {
    fg: Color,
    bg: Color,
    flags: StyleFlags,
}


impl StyleState {
    fn reset(&mut self) {
        self.fg = Color::Default;
        self.bg = Color::Default;
        self.flags = StyleFlags::default();
    }

    fn apply_sgr(&mut self, params: &[u8]) {
        if params.is_empty() {
            self.reset();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.reset(),
                1 => self.flags.bold = true,
                2 => self.flags.dim = true,
                3 => self.flags.italic = true,
                4 => self.flags.underline = true,
                5 => self.flags.blink = true,
                7 => self.flags.inverse = true,
                9 => self.flags.strikethrough = true,
                22 => {
                    self.flags.bold = false;
                    self.flags.dim = false;
                }
                23 => self.flags.italic = false,
                24 => self.flags.underline = false,
                25 => self.flags.blink = false,
                27 => self.flags.inverse = false,
                29 => self.flags.strikethrough = false,
                30..=37 => self.fg = Color::from_sgr(params[i]),
                38 => {
                    // Extended foreground color
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.fg = color;
                    }
                }
                39 => self.fg = Color::Default,
                40..=47 => self.bg = Color::from_sgr(params[i] - 10),
                48 => {
                    // Extended background color
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.bg = color;
                    }
                }
                49 => self.bg = Color::Default,
                90..=97 => self.fg = Color::from_sgr(params[i]),
                100..=107 => self.bg = Color::from_sgr(params[i] - 10),
                _ => {} // Ignore unknown SGR codes
            }
            i += 1;
        }
    }

    fn to_span(&self, text: String) -> StyledSpan {
        StyledSpan {
            text,
            fg: self.fg,
            bg: self.bg,
            style: self.flags,
        }
    }
}

fn parse_extended_color(params: &[u8], i: &mut usize) -> Option<Color> {
    if *i + 1 >= params.len() {
        return None;
    }
    *i += 1;
    match params[*i] {
        5 => {
            // 256-color: ESC[38;5;Nm
            if *i + 1 < params.len() {
                *i += 1;
                Some(Color::Indexed(params[*i]))
            } else {
                None
            }
        }
        2 => {
            // RGB: ESC[38;2;R;G;Bm
            if *i + 3 < params.len() {
                let r = params[*i + 1];
                let g = params[*i + 2];
                let b = params[*i + 3];
                *i += 3;
                Some(Color::Rgb(r, g, b))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Parse a string containing ANSI escape sequences into a `BufferLine`
pub fn parse_ansi_line(input: &str) -> BufferLine {
    let mut line = BufferLine::new();
    let mut state = StyleState::default();
    let mut current_text = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // Flush current text
            if !current_text.is_empty() {
                line.push(state.to_span(std::mem::take(&mut current_text)));
            }

            // Parse CSI sequence
            i += 2; // Skip ESC [
            let mut params: Vec<u8> = Vec::new();
            let mut param_str = String::new();

            while i < bytes.len() {
                let ch = bytes[i];
                if ch.is_ascii_digit() || ch == b';' {
                    param_str.push(ch as char);
                    i += 1;
                } else {
                    // Final character
                    if ch == b'm' {
                        // SGR sequence
                        if param_str.is_empty() {
                            params.push(0);
                        } else {
                            for p in param_str.split(';') {
                                if let Ok(n) = p.parse::<u8>() {
                                    params.push(n);
                                }
                            }
                        }
                        state.apply_sgr(&params);
                    }
                    // Skip other CSI sequences (cursor movement, etc.)
                    i += 1;
                    break;
                }
            }
        } else {
            current_text.push(bytes[i] as char);
            i += 1;
        }
    }

    // Flush remaining text
    if !current_text.is_empty() {
        line.push(state.to_span(current_text));
    }

    line
}

/// Parse multiple lines of ANSI-encoded text
pub fn parse_ansi(input: &str) -> Vec<BufferLine> {
    input.split('\n').map(parse_ansi_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let line = parse_ansi_line("hello world");
        assert_eq!(line.text(), "hello world");
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].fg, Color::Default);
    }

    #[test]
    fn test_colored_text() {
        let line = parse_ansi_line("\x1b[31mred text\x1b[0m");
        assert_eq!(line.text(), "red text");
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].fg, Color::Red);
    }

    #[test]
    fn test_mixed_colors() {
        let line = parse_ansi_line("normal \x1b[32mgreen\x1b[0m end");
        assert_eq!(line.text(), "normal green end");
        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[0].fg, Color::Default);
        assert_eq!(line.spans[1].fg, Color::Green);
        assert_eq!(line.spans[2].fg, Color::Default);
    }

    #[test]
    fn test_bold() {
        let line = parse_ansi_line("\x1b[1mbold\x1b[0m");
        assert_eq!(line.text(), "bold");
        assert!(line.spans[0].style.bold);
    }

    #[test]
    fn test_multiple_attributes() {
        let line = parse_ansi_line("\x1b[1;3;31mbold italic red\x1b[0m");
        assert_eq!(line.text(), "bold italic red");
        let span = &line.spans[0];
        assert!(span.style.bold);
        assert!(span.style.italic);
        assert_eq!(span.fg, Color::Red);
    }

    #[test]
    fn test_reset_clears_all() {
        let line = parse_ansi_line("\x1b[1;31mbold red\x1b[0mnormal");
        assert_eq!(line.spans.len(), 2);
        assert!(line.spans[0].style.bold);
        assert_eq!(line.spans[0].fg, Color::Red);
        assert!(!line.spans[1].style.bold);
        assert_eq!(line.spans[1].fg, Color::Default);
    }

    #[test]
    fn test_background_color() {
        let line = parse_ansi_line("\x1b[42mgreen bg\x1b[0m");
        assert_eq!(line.spans[0].bg, Color::Green);
    }

    #[test]
    fn test_256_color() {
        let line = parse_ansi_line("\x1b[38;5;196mcolor\x1b[0m");
        assert_eq!(line.text(), "color");
        assert_eq!(line.spans[0].fg, Color::Indexed(196));
    }

    #[test]
    fn test_rgb_color() {
        let line = parse_ansi_line("\x1b[38;2;255;128;0mrgb\x1b[0m");
        assert_eq!(line.spans[0].fg, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn test_empty_input() {
        let line = parse_ansi_line("");
        assert!(line.is_empty());
    }

    #[test]
    fn test_parse_multiline() {
        let lines = parse_ansi("line1\nline2\nline3");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text(), "line1");
        assert_eq!(lines[2].text(), "line3");
    }

    #[test]
    fn test_bright_colors() {
        let line = parse_ansi_line("\x1b[91mbright red\x1b[0m");
        assert_eq!(line.spans[0].fg, Color::BrightRed);
    }

    #[test]
    fn test_dim_italic_underline() {
        let line = parse_ansi_line("\x1b[2;3;4mstyled\x1b[0m");
        let span = &line.spans[0];
        assert!(span.style.dim);
        assert!(span.style.italic);
        assert!(span.style.underline);
    }

    #[test]
    fn test_sgr_reset_specific() {
        let line = parse_ansi_line("\x1b[1;3mbold italic\x1b[23mnot italic\x1b[0m");
        assert_eq!(line.spans.len(), 2);
        assert!(line.spans[0].style.bold);
        assert!(line.spans[0].style.italic);
        assert!(line.spans[1].style.bold);
        assert!(!line.spans[1].style.italic);
    }
}
