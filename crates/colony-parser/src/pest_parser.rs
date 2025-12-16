//! pest-based parser for Colony Shell
//!
//! This module provides a PEG parser generated from colony.pest grammar.
//! It works in conjunction with the existing mode-stack lexer for preprocessing
//! and produces the same AST as the recursive descent parser.
//!
//! Architecture:
//! 1. Preprocess: Mode-stack lexer (existing) handles context-sensitive tokenization
//! 2. Parse: pest parses the syntactic structure
//! 3. Build: AST builder transforms pest's parse tree into our AST
//!
//! See docs/peg-proposal.md for full design.

use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "colony.pest"]
pub struct ColonyParser;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let source = "echo hello world";
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "Failed to parse simple command: {:?}", result.err());
    }

    #[test]
    fn test_parse_pipeline() {
        let source = "cat file.txt | grep pattern | wc -l";
        let result = ColonyParser::parse(Rule::Pipeline, source);
        assert!(result.is_ok(), "Failed to parse pipeline: {:?}", result.err());
    }

    #[test]
    fn test_parse_if_statement() {
        let source = "if (x > 10) { echo big }";
        let result = ColonyParser::parse(Rule::IfStatement, source);
        assert!(result.is_ok(), "Failed to parse if statement: {:?}", result.err());
    }

    #[test]
    fn test_parse_for_loop() {
        // Test C-style for loop first (simpler)
        let source = "for (;;) { echo hello }";
        let result = ColonyParser::parse(Rule::ForStatement, source);
        assert!(result.is_ok(), "Failed to parse for loop: {:?}", result.err());
    }

    #[test]
    fn test_parse_variable_decl() {
        let source = "var x = 42";
        let result = ColonyParser::parse(Rule::VarDecl, source);
        assert!(result.is_ok(), "Failed to parse var decl: {:?}", result.err());
    }

    #[test]
    fn test_parse_expression() {
        let source = "x + y * 2";
        let result = ColonyParser::parse(Rule::Expression, source);
        assert!(result.is_ok(), "Failed to parse expression: {:?}", result.err());
    }

    #[test]
    fn test_parse_program() {
        // Start with simple program - one statement at a time
        let source = "echo hello";
        let result = ColonyParser::parse(Rule::Program, source);
        assert!(result.is_ok(), "Failed to parse program: {:?}", result.err());
    }
}
