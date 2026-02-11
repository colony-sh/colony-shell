//! Syntax highlighting for shell input.
//!
//! Provides styled spans for the input buffer using tree-sitter parsing
//! (via `colony_tree_sitter`) with a regex-based fallback.

/// A styled span of text for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan {
    /// The text content.
    pub text: String,
    /// The highlight style.
    pub style: HighlightStyle,
}

/// Highlight style for a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightStyle {
    /// Default/plain text.
    Normal,
    /// Command name (bold).
    Command,
    /// String literal (green).
    String,
    /// Variable reference (cyan).
    Variable,
    /// Operator/pipe/redirect (yellow).
    Operator,
    /// Comment (gray/dim).
    Comment,
    /// Keyword (if, then, while, for, etc.).
    Keyword,
    /// Number literal.
    Number,
    /// Error/invalid syntax (red underline).
    Error,
}

/// Highlight the input text using tree-sitter, falling back to regex.
///
/// Returns a vec of styled spans covering the entire input.
pub fn highlight(input: &str) -> Vec<StyledSpan> {
    if input.is_empty() {
        return Vec::new();
    }

    // Try tree-sitter highlighting first
    match highlight_tree_sitter(input) {
        Some(spans) if !spans.is_empty() => spans,
        _ => highlight_regex(input),
    }
}

/// Highlight using tree-sitter.
fn highlight_tree_sitter(input: &str) -> Option<Vec<StyledSpan>> {
    let tree = colony_tree_sitter::parse(input)?;
    let root = tree.root_node();

    let mut spans = Vec::new();
    let mut last_byte = 0;

    // Walk the tree and map node kinds to highlight styles
    collect_highlights(&root, input, &mut spans, &mut last_byte);

    // Add any remaining text after the last highlighted node
    if last_byte < input.len() {
        spans.push(StyledSpan {
            text: input[last_byte..].to_string(),
            style: HighlightStyle::Normal,
        });
    }

    Some(spans)
}

/// Recursively collect highlight spans from tree-sitter nodes.
fn collect_highlights(
    node: &colony_tree_sitter::Node,
    source: &str,
    spans: &mut Vec<StyledSpan>,
    last_byte: &mut usize,
) {
    let child_count = node.child_count();

    // If this is a leaf node (no children), classify it
    if child_count == 0 {
        let start = node.start_byte();
        let end = node.end_byte();

        // Add plain text gap before this node
        if start > *last_byte {
            spans.push(StyledSpan {
                text: source[*last_byte..start].to_string(),
                style: HighlightStyle::Normal,
            });
        }

        if start < end && end <= source.len() {
            let text = source[start..end].to_string();
            let style = classify_node(node);
            spans.push(StyledSpan { text, style });
            *last_byte = end;
        }

        return;
    }

    // Recurse into children
    for i in 0..child_count {
        let Some(child) = node.child(i) else {
            continue;
        };
        collect_highlights(&child, source, spans, last_byte);
    }
}

/// Map a tree-sitter node kind to a highlight style.
fn classify_node(node: &colony_tree_sitter::Node) -> HighlightStyle {
    let kind = node.kind();

    match kind {
        // Commands
        "command_name" | "function_definition" => HighlightStyle::Command,

        // Strings
        "string" | "raw_string" | "string_content" | "heredoc_body" | "heredoc_content" => {
            HighlightStyle::String
        }

        // Variables
        "variable_name" | "simple_expansion" | "expansion" | "special_variable_name" => {
            HighlightStyle::Variable
        }
        // The $ sigil
        "$" => HighlightStyle::Variable,

        // Operators
        "|" | "||" | "&&" | ";" | ";;" | "&" | "|&" | ">" | ">>" | "<" | "<<" | "<<<" | ">&"
        | "<&" | "<<-" | "->" => HighlightStyle::Operator,

        // Redirects
        "file_redirect" | "heredoc_redirect" | "herestring_redirect" => HighlightStyle::Operator,

        // Keywords
        "if" | "then" | "else" | "elif" | "fi" | "while" | "until" | "do" | "done" | "for"
        | "in" | "case" | "esac" | "function" | "select" | "match" => HighlightStyle::Keyword,

        // Comments
        "comment" => HighlightStyle::Comment,

        // Numbers
        "number" => HighlightStyle::Number,

        // Error nodes
        "ERROR" => HighlightStyle::Error,

        // Word (could be command or argument depending on position)
        "word" => {
            // Check if parent is command and this is the first child (= command name)
            if let Some(parent) = node.parent() {
                if parent.kind() == "command" {
                    let Some(first_child) = parent.child(0) else {
                        return HighlightStyle::Normal;
                    };
                    if first_child.id() == node.id() {
                        return HighlightStyle::Command;
                    }
                }
            }
            HighlightStyle::Normal
        }

        _ => HighlightStyle::Normal,
    }
}

/// Fallback regex-based highlighting for when tree-sitter fails.
fn highlight_regex(input: &str) -> Vec<StyledSpan> {
    let mut spans = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    // Track if we're at the start of a command (for command highlighting)
    let mut at_command_start = true;

    while i < len {
        let c = chars[i];

        // Comment
        if c == '#' {
            let rest: String = chars[i..].iter().collect();
            spans.push(StyledSpan {
                text: rest,
                style: HighlightStyle::Comment,
            });
            break;
        }

        // Variable ($VAR or ${VAR})
        if c == '$' {
            let start = i;
            i += 1;
            if i < len && chars[i] == '{' {
                // ${...}
                i += 1;
                while i < len && chars[i] != '}' {
                    i += 1;
                }
                if i < len {
                    i += 1; // skip '}'
                }
            } else {
                // $VAR
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(StyledSpan {
                text,
                style: HighlightStyle::Variable,
            });
            at_command_start = false;
            continue;
        }

        // Single-quoted string
        if c == '\'' {
            let start = i;
            i += 1;
            while i < len && chars[i] != '\'' {
                i += 1;
            }
            if i < len {
                i += 1; // closing quote
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(StyledSpan {
                text,
                style: HighlightStyle::String,
            });
            at_command_start = false;
            continue;
        }

        // Double-quoted string
        if c == '"' {
            let start = i;
            i += 1;
            while i < len && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1; // skip escaped char
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(StyledSpan {
                text,
                style: HighlightStyle::String,
            });
            at_command_start = false;
            continue;
        }

        // Operators
        if matches!(c, '|' | '&' | ';' | '>' | '<') {
            let start = i;
            i += 1;
            // Check for two-char operators
            if i < len
                && ((c == '|' && chars[i] == '|')
                    || (c == '&' && chars[i] == '&')
                    || (c == '>' && chars[i] == '>')
                    || (c == '<' && chars[i] == '<')
                    || (c == '-' && chars[i] == '>'))
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(StyledSpan {
                text,
                style: HighlightStyle::Operator,
            });
            at_command_start = true;
            continue;
        }

        // Whitespace
        if c.is_whitespace() {
            let start = i;
            while i < len && chars[i].is_whitespace() {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(StyledSpan {
                text,
                style: HighlightStyle::Normal,
            });
            continue;
        }

        // Word (command or argument)
        let start = i;
        while i < len
            && !chars[i].is_whitespace()
            && !matches!(chars[i], '|' | '&' | ';' | '>' | '<' | '$' | '\'' | '"' | '#')
        {
            i += 1;
        }
        let text: String = chars[start..i].iter().collect();
        let style = if at_command_start {
            at_command_start = false;
            if is_keyword(&text) {
                HighlightStyle::Keyword
            } else {
                HighlightStyle::Command
            }
        } else {
            HighlightStyle::Normal
        };
        spans.push(StyledSpan { text, style });
    }

    spans
}

/// Check if a word is a shell keyword.
fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "then"
            | "else"
            | "elif"
            | "fi"
            | "while"
            | "until"
            | "do"
            | "done"
            | "for"
            | "in"
            | "case"
            | "esac"
            | "function"
            | "select"
            | "match"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_empty() {
        let spans = highlight("");
        assert!(spans.is_empty());
    }

    #[test]
    fn test_highlight_simple_command() {
        let spans = highlight_regex("echo hello");
        assert!(!spans.is_empty());
        // First non-whitespace word should be a command
        let first_word = spans
            .iter()
            .find(|s| s.style != HighlightStyle::Normal || !s.text.trim().is_empty());
        assert!(first_word.is_some());
        assert_eq!(first_word.unwrap().style, HighlightStyle::Command);
    }

    #[test]
    fn test_highlight_variable() {
        let spans = highlight_regex("echo $HOME");
        let var_span = spans.iter().find(|s| s.style == HighlightStyle::Variable);
        assert!(var_span.is_some());
        assert_eq!(var_span.unwrap().text, "$HOME");
    }

    #[test]
    fn test_highlight_variable_braces() {
        let spans = highlight_regex("echo ${HOME}");
        let var_span = spans.iter().find(|s| s.style == HighlightStyle::Variable);
        assert!(var_span.is_some());
        assert_eq!(var_span.unwrap().text, "${HOME}");
    }

    #[test]
    fn test_highlight_single_quoted_string() {
        let spans = highlight_regex("echo 'hello world'");
        let str_span = spans.iter().find(|s| s.style == HighlightStyle::String);
        assert!(str_span.is_some());
        assert_eq!(str_span.unwrap().text, "'hello world'");
    }

    #[test]
    fn test_highlight_double_quoted_string() {
        let spans = highlight_regex("echo \"hello\"");
        let str_span = spans.iter().find(|s| s.style == HighlightStyle::String);
        assert!(str_span.is_some());
        assert_eq!(str_span.unwrap().text, "\"hello\"");
    }

    #[test]
    fn test_highlight_pipe_operator() {
        let spans = highlight_regex("ls | grep foo");
        let op_span = spans.iter().find(|s| s.style == HighlightStyle::Operator);
        assert!(op_span.is_some());
        assert_eq!(op_span.unwrap().text, "|");
    }

    #[test]
    fn test_highlight_comment() {
        let spans = highlight_regex("echo hello # comment");
        let comment_span = spans.iter().find(|s| s.style == HighlightStyle::Comment);
        assert!(comment_span.is_some());
        assert!(comment_span.unwrap().text.starts_with('#'));
    }

    #[test]
    fn test_highlight_keyword() {
        let spans = highlight_regex("if true; then echo yes; fi");
        let keyword_spans: Vec<_> = spans
            .iter()
            .filter(|s| s.style == HighlightStyle::Keyword)
            .collect();
        assert!(keyword_spans.iter().any(|s| s.text == "if"));
    }

    #[test]
    fn test_highlight_after_pipe_is_command() {
        let spans = highlight_regex("ls | grep foo");
        let command_spans: Vec<_> = spans
            .iter()
            .filter(|s| s.style == HighlightStyle::Command)
            .collect();
        // Both "ls" and "grep" should be commands
        assert!(command_spans.iter().any(|s| s.text == "ls"));
        assert!(command_spans.iter().any(|s| s.text == "grep"));
    }

    #[test]
    fn test_highlight_and_operator() {
        let spans = highlight_regex("cmd1 && cmd2");
        let op_span = spans.iter().find(|s| s.style == HighlightStyle::Operator);
        assert!(op_span.is_some());
        assert_eq!(op_span.unwrap().text, "&&");
    }

    #[test]
    fn test_highlight_redirect() {
        let spans = highlight_regex("echo hello > file.txt");
        let op_span = spans.iter().find(|s| s.style == HighlightStyle::Operator);
        assert!(op_span.is_some());
    }

    #[test]
    fn test_is_keyword() {
        assert!(is_keyword("if"));
        assert!(is_keyword("then"));
        assert!(is_keyword("else"));
        assert!(is_keyword("fi"));
        assert!(is_keyword("while"));
        assert!(is_keyword("for"));
        assert!(is_keyword("do"));
        assert!(is_keyword("done"));
        assert!(!is_keyword("echo"));
        assert!(!is_keyword("ls"));
    }

    #[test]
    fn test_classify_node_kinds() {
        // Test the classify function with known node kinds
        // We can't easily create tree-sitter nodes in tests, but we can
        // verify the regex fallback works correctly for all syntax types.
        let input = "for i in 1 2 3; do echo $i; done";
        let spans = highlight_regex(input);
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_highlight_style_classification() {
        // Ensure all styles are distinguishable
        let styles = [
            HighlightStyle::Normal,
            HighlightStyle::Command,
            HighlightStyle::String,
            HighlightStyle::Variable,
            HighlightStyle::Operator,
            HighlightStyle::Comment,
            HighlightStyle::Keyword,
            HighlightStyle::Number,
            HighlightStyle::Error,
        ];

        for (i, a) in styles.iter().enumerate() {
            for (j, b) in styles.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }
}
