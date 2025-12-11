//! Recursive descent parser for POSIX shell grammar

use crate::{ast::*, lexer::Token, Result};

/// Parser that builds AST from token stream
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    /// Create a new parser for the given tokens
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parse the token stream into an AST
    pub fn parse(&mut self) -> Result<Command> {
        // TODO: Implement parser
        // For now, return a placeholder
        Ok(Command::Simple(SimpleCommand {
            assignments: vec![],
            words: vec![],
            redirects: vec![],
        }))
    }

    fn peek(&self) -> Token {
        self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let token = self.peek();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        token
    }
}
