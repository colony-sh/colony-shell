//! Natural language detection for routing to agent vs shell execution

use colony_parser::{Lexer, Parser};

/// Detects whether input should be routed to agent or shell parser
pub struct NaturalLanguageDetector;

impl NaturalLanguageDetector {
    /// Check if input looks like natural language
    pub fn is_natural_language(input: &str) -> bool {
        let trimmed = input.trim();

        // Explicit NL prefixes
        if trimmed.starts_with('#') || trimmed.starts_with('?') {
            return true;
        }

        // Try parsing as shell command
        if Self::parse_fails(trimmed) {
            return true;
        }

        // Heuristics for natural language
        Self::has_nl_patterns(trimmed)
    }

    /// Try to parse as shell command
    fn parse_fails(input: &str) -> bool {
        let mut lexer = Lexer::new(input);
        let tokens = match lexer.tokenize() {
            Ok(tokens) => tokens,
            Err(_) => return true, // Parse error = probably NL
        };

        let mut parser = Parser::new(tokens);
        parser.parse().is_err()
    }

    /// Check for natural language patterns
    fn has_nl_patterns(input: &str) -> bool {
        let lowercase = input.to_lowercase();

        // Question words
        let question_words = [
            "what", "why", "how", "when", "where", "who",
            "can you", "could you", "would you", "please",
            "explain", "tell me", "show me",
        ];

        question_words.iter().any(|word| lowercase.starts_with(word))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explicit_nl_prefix() {
        assert!(NaturalLanguageDetector::is_natural_language("# what is rust"));
        assert!(NaturalLanguageDetector::is_natural_language("? how to use cargo"));
    }

    #[test]
    fn test_shell_commands() {
        // These should NOT be detected as NL
        assert!(!NaturalLanguageDetector::is_natural_language("ls -la"));
        assert!(!NaturalLanguageDetector::is_natural_language("echo hello"));
        assert!(!NaturalLanguageDetector::is_natural_language("cd /tmp"));
    }

    #[test]
    fn test_question_patterns() {
        assert!(NaturalLanguageDetector::is_natural_language("what is the weather"));
        assert!(NaturalLanguageDetector::is_natural_language("how do I compile this"));
        assert!(NaturalLanguageDetector::is_natural_language("can you help me"));
    }
}
