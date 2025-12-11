//! Lexer with mode stack for context-sensitive tokenization
//!
//! The lexer handles shell quoting rules by maintaining a stack of lexing modes.
//! This allows proper handling of nested contexts like double quotes containing
//! command substitutions.

use crate::Result;

/// Lexical analysis mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexMode {
    /// Normal unquoted mode
    Normal,
    /// Inside single quotes (no expansions)
    SingleQuote,
    /// Inside double quotes ($ and ` expand)
    DoubleQuote,
    /// After backslash escape
    Escape,
    /// Reading heredoc content
    Heredoc,
}

/// Shell token types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Unquoted word
    Word(String),
    /// Operator token
    Operator(Operator),
    /// Newline (significant in shell grammar)
    Newline,
    /// End of input
    Eof,
}

/// Shell operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Pipe,          // |
    Semi,          // ;
    Amp,           // &
    AndIf,         // &&
    OrIf,          // ||
    Great,         // >
    DGreat,        // >>
    Less,          // <
    DLess,         // <<
    LParen,        // (
    RParen,        // )
    LBrace,        // {
    RBrace,        // }
}

/// Lexer that tokenizes shell input
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    mode_stack: Vec<LexMode>,
}

impl Lexer {
    /// Create a new lexer for the given input
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            mode_stack: vec![LexMode::Normal],
        }
    }

    /// Tokenize the entire input
    pub fn tokenize(&mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if token == Token::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        Ok(tokens)
    }

    /// Get the next token
    fn next_token(&mut self) -> Result<Token> {
        // TODO: Implement proper tokenization
        // For now, return EOF
        Ok(Token::Eof)
    }

    fn current_mode(&self) -> LexMode {
        *self.mode_stack.last().unwrap_or(&LexMode::Normal)
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        Some(ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_basic() {
        let mut lexer = Lexer::new("");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens, vec![Token::Eof]);
    }
}
