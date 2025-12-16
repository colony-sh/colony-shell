//! Colony shell parser
//!
//! This crate provides a hand-written lexer and recursive descent parser
//! for Colony shell syntax. The lexer uses a mode stack to handle context-sensitive
//! tokenization (quoting, heredocs, etc.), and the parser builds an AST.
//!
//! # Example
//! ```ignore
//! use colony_parser::{Lexer, Parser, ShellOptions};
//!
//! let input = "echo hello | grep h";
//! let options = ShellOptions::colony_upgrade();
//! let mut lexer = Lexer::new(input, options);
//! let tokens = lexer.tokenize()?;
//! let mut parser = Parser::new(tokens);
//! let ast = parser.parse()?;
//! ```

pub mod span;
pub mod token;
pub mod lexer;
pub mod parser;
pub mod ast;
pub mod pest_parser;
pub mod builder;
pub mod pest_api;

pub use span::{Position, Span};
pub use token::{Token, TokenKind, TokenValue, InterpolatedPart};
pub use lexer::{Lexer, LexerMode};
pub use parser::Parser;
pub use ast::*;

// Primary API - Pure PEG parsing
pub use pest_api::{parse, parse_statement, parse_expression, ParseError as PestParseError};

/// Lexer error types
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum LexError {
    #[error("unterminated string at {0:?}")]
    UnterminatedString(Position),

    #[error("unterminated comment at {0:?}")]
    UnterminatedComment(Position),

    #[error("invalid raw string at {0:?}")]
    InvalidRawString(Position),

    #[error("invalid number at {0:?}")]
    InvalidNumber(Position),

    #[error("unexpected character '{0}' at {1:?}")]
    UnexpectedChar(char, Position),

    #[error("unexpected end of input")]
    UnexpectedEof,
}

/// Parser error types
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ParseError {
    #[error("unexpected token: {0:?}")]
    UnexpectedToken(TokenKind),

    #[error("expected token {expected:?}, found {found:?}")]
    ExpectedToken {
        expected: TokenKind,
        found: TokenKind,
    },

    #[error("unexpected end of input")]
    UnexpectedEof,

    #[error("lexer error: {0}")]
    LexError(#[from] LexError),
}

/// Shell option configuration
#[derive(Clone, Debug, Default)]
pub struct ShellOptions {
    // === Parsing Options ===
    pub parse_paren: bool,
    pub parse_brace: bool,
    pub parse_at: bool,
    pub parse_arrow: bool,
    pub parse_triple_quote: bool,
    pub parse_raw_string: bool,
    pub parse_func: bool,
    pub parse_proc: bool,
    pub parse_agent: bool,
}

impl ShellOptions {
    /// POSIX-compatible defaults (all features disabled)
    pub fn posix() -> Self {
        Self::default()
    }

    /// Minimal modern upgrades (safe interop)
    pub fn colony_upgrade() -> Self {
        Self {
            parse_paren: true,
            parse_brace: true,
            parse_at: true,
            parse_arrow: true,
            ..Self::default()
        }
    }

    /// Full modern mode
    pub fn colony_all() -> Self {
        Self {
            parse_paren: true,
            parse_brace: true,
            parse_at: true,
            parse_arrow: true,
            parse_triple_quote: true,
            parse_raw_string: true,
            parse_func: true,
            parse_proc: true,
            parse_agent: true,
        }
    }
}

pub type Result<T> = std::result::Result<T, ParseError>;
