//! POSIX shell parser
//!
//! This crate provides a hand-written lexer and recursive descent parser
//! for POSIX shell syntax. The lexer uses a mode stack to handle context-sensitive
//! tokenization (quoting, heredocs, etc.), and the parser builds an AST.
//!
//! # Example
//! ```
//! use colony_parser::{Lexer, Parser};
//!
//! let input = "echo hello | grep h";
//! // let tokens = Lexer::new(input).tokenize()?;
//! // let ast = Parser::new(tokens).parse()?;
//! ```

pub mod lexer;
pub mod parser;
pub mod ast;

pub use lexer::Lexer;
pub use parser::Parser;
pub use ast::*;

/// Parser error types
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ParseError {
    #[error("unexpected token: {0}")]
    UnexpectedToken(String),

    #[error("unterminated quote")]
    UnterminatedQuote,

    #[error("unexpected end of input")]
    UnexpectedEof,
}

pub type Result<T> = std::result::Result<T, ParseError>;
