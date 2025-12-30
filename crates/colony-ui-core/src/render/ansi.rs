use crate::{ComponentOutput, StyledSpan};

pub fn span_to_ansi(span: &StyledSpan) -> String {
    let mut codes = vec![];

    if let Some(ref fg) = span.style.fg {
        codes.push(fg.to_ansi_fg());
    }
    if let Some(ref bg) = span.style.bg {
        codes.push(bg.to_ansi_bg());
    }
    if span.style.bold {
        codes.push("1".to_string());
    }
    if span.style.dim {
        codes.push("2".to_string());
    }
    if span.style.italic {
        codes.push("3".to_string());
    }

    if codes.is_empty() {
        span.text.clone()
    } else {
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), span.text)
    }
}

pub fn spans_to_ansi(spans: &[StyledSpan]) -> String {
    spans.iter().map(span_to_ansi).collect()
}

pub fn render_outputs(outputs: &[ComponentOutput]) -> String {
    outputs
        .iter()
        .filter(|o| o.visible)
        .map(|o| spans_to_ansi(&o.spans))
        .collect::<Vec<_>>()
        .join("  ")
}

pub fn render_prompt(left: &[ComponentOutput], right: &[ComponentOutput], width: usize) -> String {
    let left_str = render_outputs(left);
    let right_str = render_outputs(right);

    let left_visible_len = strip_ansi(&left_str).len();
    let right_visible_len = strip_ansi(&right_str).len();

    let min_gap = 2;
    let padding = width
        .saturating_sub(left_visible_len)
        .saturating_sub(right_visible_len)
        .saturating_sub(min_gap);

    if right_str.is_empty() {
        format!("{}\n> ", left_str)
    } else {
        format!(
            "{}{:padding$}{}\n> ",
            left_str,
            "",
            right_str,
            padding = padding
        )
    }
}

pub fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == 'm' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Style};

    #[test]
    fn test_render_plain_span() {
        let span = StyledSpan::plain("hello");
        let result = span_to_ansi(&span);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_render_colored_span() {
        let span = StyledSpan::new("hello", Style::default().fg(Color::Cyan));
        let result = span_to_ansi(&span);
        assert!(result.contains("\x1b[36m"));
        assert!(result.contains("hello"));
        assert!(result.contains("\x1b[0m"));
    }

    #[test]
    fn test_render_bold_span() {
        let span = StyledSpan::new("hello", Style::default().bold());
        let result = span_to_ansi(&span);
        assert!(result.contains("\x1b[1m"));
    }

    #[test]
    fn test_strip_ansi() {
        let text = "\x1b[36mhello\x1b[0m";
        let result = strip_ansi(text);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_render_outputs_skips_hidden() {
        let outputs = vec![
            ComponentOutput::visible(vec![StyledSpan::plain("visible")]),
            ComponentOutput::hidden(),
            ComponentOutput::visible(vec![StyledSpan::plain("also visible")]),
        ];
        let result = render_outputs(&outputs);
        assert!(result.contains("visible"));
        assert!(result.contains("also visible"));
    }
}
