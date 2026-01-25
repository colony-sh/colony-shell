//! Syntax highlighting using tree-sitter

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use tree_sitter::Parser;

/// Syntax highlighter for shell input
pub struct Highlighter {
    parser: Parser,
}

impl Highlighter {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(colony_tree_sitter::language())
            .expect("Failed to set language");
        Self { parser }
    }

    /// Highlight a line of shell input
    pub fn highlight<'a>(&mut self, input: &'a str) -> Line<'a> {
        // Parse with tree-sitter
        let tree = self.parser.parse(input, None);

        if tree.is_none() {
            // Fallback to plain text if parsing fails
            return Line::from(input);
        }

        let tree = tree.unwrap();
        let root = tree.root_node();

        // Collect highlights
        let mut spans: Vec<Span> = Vec::new();
        let mut last_end = 0;

        self.collect_highlights(root, input, &mut spans, &mut last_end);

        // Add any remaining unhighlighted text
        if last_end < input.len() {
            spans.push(Span::raw(&input[last_end..]));
        }

        Line::from(spans)
    }

    fn collect_highlights<'a>(
        &self,
        node: tree_sitter::Node<'_>,
        source: &'a str,
        spans: &mut Vec<Span<'a>>,
        last_end: &mut usize,
    ) {
        // Process children first for proper ordering
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_highlights(child, source, spans, last_end);
        }

        // Only process leaf nodes for actual highlighting
        if node.child_count() > 0 {
            return;
        }

        let start = node.start_byte();
        let end = node.end_byte();

        // Skip if we've already processed this region
        if start < *last_end {
            return;
        }

        // Add any gap before this node
        if start > *last_end {
            spans.push(Span::raw(&source[*last_end..start]));
        }

        let text = &source[start..end];
        let kind = node.kind();

        // Map node kinds to styles
        let style = match kind {
            // Commands and builtins
            "command_name" | "word" if is_builtin(text) => Style::default().fg(Color::Yellow),
            "command_name" => Style::default().fg(Color::Green),

            // Strings
            "string" | "raw_string" | "string_content" | "heredoc_content" => {
                Style::default().fg(Color::Green)
            }

            // Variables
            "variable_name" | "simple_expansion" | "expansion" => Style::default().fg(Color::Cyan),
            "special_variable_name" => Style::default().fg(Color::Cyan),

            // Operators
            "file_redirect" | "heredoc_redirect" | "herestring_redirect" => {
                Style::default().fg(Color::Magenta)
            }
            "|" | "&&" | "||" | ";" | "&" => Style::default().fg(Color::Magenta),
            ">" | ">>" | "<" | "<<" | "<<<" => Style::default().fg(Color::Magenta),

            // Keywords
            "if" | "then" | "else" | "elif" | "fi" | "case" | "esac" | "for" | "while"
            | "until" | "do" | "done" | "in" | "function" => Style::default().fg(Color::Magenta),

            // Comments
            "comment" => Style::default().fg(Color::DarkGray),

            // Numbers
            "number" => Style::default().fg(Color::Yellow),

            // Default
            _ => Style::default(),
        };

        spans.push(Span::styled(text.to_string(), style));
        *last_end = end;
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "cd" | "pwd"
            | "echo"
            | "export"
            | "unset"
            | "source"
            | "."
            | "exit"
            | "read"
            | "printf"
            | "test"
            | "["
            | "true"
            | "false"
            | "set"
            | "shift"
            | "return"
            | "break"
            | "continue"
            | "eval"
            | "exec"
            | "command"
            | "builtin"
            | "local"
            | "declare"
            | "readonly"
            | "alias"
            | "unalias"
            | "type"
            | "hash"
            | "jobs"
            | "fg"
            | "bg"
            | "wait"
            | "kill"
            | "trap"
            | "history"
            | "fc"
            | "ulimit"
            | "umask"
            | "times"
            | "getopts"
    )
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_simple_command() {
        let mut highlighter = Highlighter::new();
        let line = highlighter.highlight("echo hello");
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_highlight_variable() {
        let mut highlighter = Highlighter::new();
        let line = highlighter.highlight("echo $HOME");
        assert!(!line.spans.is_empty());
    }
}
