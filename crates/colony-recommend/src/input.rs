//! Parsed input context for recommendation generation.

use std::path::PathBuf;

/// Parsed context from the current command line input.
#[derive(Debug, Clone)]
pub struct RecommendInput {
    /// Full input line.
    pub line: String,
    /// Current word being typed (under cursor).
    pub word: String,
    /// Cursor position in the line.
    pub cursor: usize,
    /// The command (first word), if any.
    pub command: Option<String>,
    /// Argument position (0 = command itself, 1 = first arg, etc.)
    pub arg_position: usize,
    /// Current working directory.
    pub cwd: PathBuf,
}

impl RecommendInput {
    /// Parse a raw input line at cursor position.
    pub fn parse(line: &str, cursor: usize, cwd: PathBuf) -> Self {
        let before_cursor = &line[..cursor.min(line.len())];
        let words: Vec<&str> = before_cursor.split_whitespace().collect();

        let command = words.first().map(|s| (*s).to_string());
        let ends_with_space = before_cursor.ends_with(' ');

        // Current word: last whitespace-delimited token, or empty if ends with space
        let word = if ends_with_space || before_cursor.is_empty() {
            String::new()
        } else {
            words.last().unwrap_or(&"").to_string()
        };

        // When trailing space, cursor is at the next argument position
        let arg_position = if words.is_empty() {
            0
        } else if ends_with_space {
            words.len()
        } else {
            words.len() - 1
        };

        Self {
            line: line.to_string(),
            word,
            cursor,
            command,
            arg_position,
            cwd,
        }
    }

    /// Whether we're completing the command name (position 0).
    pub fn is_command_position(&self) -> bool {
        self.arg_position == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_position() {
        let input = RecommendInput::parse("gi", 2, PathBuf::from("/tmp"));
        assert!(input.is_command_position());
        assert_eq!(input.word, "gi");
        assert_eq!(input.command, Some("gi".to_string()));
    }

    #[test]
    fn parse_arg_position() {
        let input = RecommendInput::parse("git com", 7, PathBuf::from("/tmp"));
        assert!(!input.is_command_position());
        assert_eq!(input.word, "com");
        assert_eq!(input.command, Some("git".to_string()));
        assert_eq!(input.arg_position, 1);
    }

    #[test]
    fn parse_trailing_space() {
        let input = RecommendInput::parse("git ", 4, PathBuf::from("/tmp"));
        assert!(!input.is_command_position());
        assert_eq!(input.word, "");
        assert_eq!(input.arg_position, 1);
    }

    #[test]
    fn parse_empty() {
        let input = RecommendInput::parse("", 0, PathBuf::from("/tmp"));
        assert!(input.is_command_position());
        assert_eq!(input.word, "");
    }

    #[test]
    fn parse_multiple_args() {
        let input = RecommendInput::parse("git commit -m 'hel", 18, PathBuf::from("/tmp"));
        assert!(!input.is_command_position());
        assert_eq!(input.command, Some("git".to_string()));
        assert_eq!(input.arg_position, 3);
        assert_eq!(input.word, "'hel");
    }

    #[test]
    fn parse_cursor_mid_line() {
        let input = RecommendInput::parse("git commit --amend", 10, PathBuf::from("/tmp"));
        // cursor at position 10 means "git commit"
        assert_eq!(input.word, "commit");
        assert_eq!(input.arg_position, 1);
    }
}
