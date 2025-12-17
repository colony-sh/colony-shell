//! Public API for pest-based parser
//!
//! This module provides the main entry point for parsing Colony shell source
//! directly into AST using pure PEG (no preprocessing required for most constructs).
//!
//! # Architecture
//!
//! Pure PEG approach - pest operates on source text directly:
//! ```text
//! Source Text → pest Grammar → Parse Tree → AST Builder → AST
//! ```
//!
//! Context sensitivity is handled by **grammar structure**:
//! - `SimpleCommand` expects `Word` (barewords allowed)
//! - `Expression` expects `Identifier` (stricter)
//! - `<` is redirect in `SimpleCommand`, comparison in `Expression`
//!
//! # What Doesn't Need Preprocessing
//!
//! - ✅ Context-sensitive barewords (`echo hello` vs `var x = hello`)
//! - ✅ Context-sensitive operators (`<` as redirect vs comparison)
//! - ✅ Variable expansions (`$var`, `${var}`)
//! - ✅ Command substitution (`$(cmd)`, nested)
//! - ✅ Quote interpolation (`"Count: $count"`)
//! - ✅ Nested structures (`"outer $(echo "inner")"`)
//!
//! # What Still Needs Preprocessing
//!
//! - ⏳ Heredocs (content appears after command line)
//! - ⏳ Complex escape sequences in some contexts
//!
//! See `docs/peg-proposal.md` for full architectural discussion.

use pest::Parser;
use crate::pest_parser::{ColonyParser, Rule};
use crate::builder::AstBuilder;
use crate::ast::Program;
use crate::heredoc::preprocess_heredocs;

/// Parse Colony shell source code into AST
///
/// This is the main entry point for parsing. It operates directly on
/// source text using the pest PEG grammar, with minimal preprocessing
/// for heredocs.
///
/// # Heredoc Handling
///
/// Heredocs (`<<EOF`) require preprocessing because their content appears
/// after the command line. This function automatically detects and handles them.
///
/// # Example
/// ```ignore
/// use colony_parser::parse;
///
/// let source = r#"
///     if (x > 10) {
///         echo "big"
///     }
/// "#;
///
/// let program = parse(source)?;
/// assert_eq!(program.statements.len(), 1);
/// ```
///
/// # Errors
///
/// Returns `ParseError` if:
/// - Syntax is invalid (pest parsing fails)
/// - AST building fails (malformed parse tree)
/// - Heredoc preprocessing fails
pub fn parse(source: &str) -> Result<Program, ParseError> {
    // Check if source contains heredocs
    if source.contains("<<") {
        parse_with_heredocs(source)
    } else {
        parse_direct(source)
    }
}

/// Parse source directly with pest (no heredocs)
fn parse_direct(source: &str) -> Result<Program, ParseError> {
    // Parse source directly with pest
    let pairs = ColonyParser::parse(Rule::Program, source)
        .map_err(|e| {
            let (line, column) = match e.line_col {
                pest::error::LineColLocation::Pos((l, c)) => (l, c),
                pest::error::LineColLocation::Span((l, c), _) => (l, c),
            };
            ParseError::Syntax {
                message: e.to_string(),
                line,
                column,
            }
        })?;

    // Build AST from parse tree
    let program = AstBuilder::build_program(pairs)
        .map_err(ParseError::Build)?;

    Ok(program)
}

/// Parse source with heredoc preprocessing
fn parse_with_heredocs(source: &str) -> Result<Program, ParseError> {
    // Preprocess heredocs
    let (normalized, _heredoc_contents) = preprocess_heredocs(source)
        .map_err(|e| ParseError::HeredocPreprocessing(e))?;

    // Parse normalized source
    let pairs = ColonyParser::parse(Rule::Program, &normalized)
        .map_err(|e| {
            let (line, column) = match e.line_col {
                pest::error::LineColLocation::Pos((l, c)) => (l, c),
                pest::error::LineColLocation::Span((l, c), _) => (l, c),
            };
            ParseError::Syntax {
                message: e.to_string(),
                line,
                column,
            }
        })?;

    // Build AST
    let program = AstBuilder::build_program(pairs)
        .map_err(ParseError::Build)?;

    // TODO: Inject heredoc content into AST's Redirect nodes
    // For now, heredoc Redirects will have placeholder content

    Ok(program)
}

/// Parse a single statement (for REPL or testing)
pub fn parse_statement(source: &str) -> Result<crate::ast::Statement, ParseError> {
    let pairs = ColonyParser::parse(Rule::Statement, source)
        .map_err(|e| {
            let (line, column) = match e.line_col {
                pest::error::LineColLocation::Pos((l, c)) => (l, c),
                pest::error::LineColLocation::Span((l, c), _) => (l, c),
            };
            ParseError::Syntax {
                message: e.to_string(),
                line,
                column,
            }
        })?;

    let pair = pairs.into_iter().next()
        .ok_or(ParseError::EmptyInput)?;

    // Statement is a wrapper, descend into it
    let inner = pair.into_inner().next()
        .ok_or(ParseError::EmptyInput)?;

    AstBuilder::build_statement(inner)
        .map_err(ParseError::Build)
}

/// Parse an expression (for evaluation)
pub fn parse_expression(source: &str) -> Result<crate::ast::Expression, ParseError> {
    let pairs = ColonyParser::parse(Rule::Expression, source)
        .map_err(|e| {
            let (line, column) = match e.line_col {
                pest::error::LineColLocation::Pos((l, c)) => (l, c),
                pest::error::LineColLocation::Span((l, c), _) => (l, c),
            };
            ParseError::Syntax {
                message: e.to_string(),
                line,
                column,
            }
        })?;

    let pair = pairs.into_iter().next()
        .ok_or(ParseError::EmptyInput)?;

    AstBuilder::build_expression(pair)
        .map_err(ParseError::Build)
}

/// Errors during parsing
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("syntax error at line {line}, column {column}: {message}")]
    Syntax {
        message: String,
        line: usize,
        column: usize,
    },

    #[error("AST build error: {0}")]
    Build(#[from] crate::builder::BuildError),

    #[error("heredoc preprocessing error: {0}")]
    HeredocPreprocessing(String),

    #[error("empty input")]
    EmptyInput,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_program() {
        let source = "echo hello world";
        let result = parse(source);
        assert!(result.is_ok());

        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
    }

    #[test]
    fn test_parse_multi_statement() {
        let source = r#"
            echo "Starting"
            var count = 10
            if (count > 5) {
                echo "big"
            }
        "#;

        let result = parse(source);
        assert!(result.is_ok(), "Failed: {:?}", result.err());

        let program = result.unwrap();
        assert_eq!(program.statements.len(), 3);
    }

    #[test]
    fn test_parse_pipeline() {
        let source = "cat file.txt | grep pattern | wc -l";
        let result = parse(source);
        assert!(result.is_ok());

        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
    }

    #[test]
    fn test_parse_statement_api() {
        let source = "echo hello";
        let result = parse_statement(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_expression_api() {
        let source = "42";
        let result = parse_expression(source);
        assert!(result.is_ok());

        let source = "x + y";
        let result = parse_expression(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_context_from_grammar() {
        // This tests that grammar structure provides context

        // "hello" as bareword in command
        let source = "echo hello";
        let result = parse(source);
        assert!(result.is_ok(), "Bareword in command should work");

        // "hello" as identifier in expression
        let source = "var x = hello";
        let result = parse(source);
        assert!(result.is_ok(), "Identifier in expression should work");

        // Both parse successfully - context from grammar!
    }

    #[test]
    fn test_operator_context() {
        // '<' as redirect
        let source = "cat < input.txt";
        let result = parse(source);
        assert!(result.is_ok(), "< as redirect should work");

        // '<' as comparison
        let source = "if (x < 10) { echo small }";
        let result = parse(source);
        assert!(result.is_ok(), "< as comparison should work");
    }

    #[test]
    fn test_dollar_expansion() {
        let source = "echo $HOME";
        let result = parse(source);
        assert!(result.is_ok(), "Should parse $var");

        let source = r#"echo "Path: $PATH""#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse $var in quotes");

        let source = "echo $(ls)";
        let result = parse(source);
        assert!(result.is_ok(), "Should parse $(cmd)");
    }

    #[test]
    fn test_command_sub_with_args() {
        use pest::Parser;
        use crate::pest_parser::{ColonyParser, Rule};

        // Test -1: Can we parse "world" as a Word?
        eprintln!("\n=== Test -1: Parse 'world' as Word ===");
        match ColonyParser::parse(Rule::Word, "world") {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    eprintln!("✓ Word parsed: {:?}", pair.as_str());
                }
            }
            Err(e) => {
                eprintln!("✗ Cannot parse as Word: {}", e);
            }
        }

        // Test 0: Can we parse "echo world" as CommandList?
        eprintln!("\n=== Test 0: Parse 'echo world' as CommandList ===");
        match ColonyParser::parse(Rule::CommandList, "echo world") {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!("✓ CommandList: {:?}", pair.as_str());
                    for inner in pair.into_inner() {
                        eprintln!("  {:?} = {:?}", inner.as_rule(), inner.as_str());
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ Cannot parse as CommandList: {}", e);
            }
        }

        // Test 0b: Can we parse "echo world" as SimpleCommand?
        eprintln!("\n=== Test 0b: Parse 'echo world' as SimpleCommand ===");
        match ColonyParser::parse(Rule::SimpleCommand, "echo world") {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!("✓ SimpleCommand: {:?}", pair.as_str());
                    for inner in pair.into_inner() {
                        eprintln!("  {:?} = {:?}", inner.as_rule(), inner.as_str());
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ Cannot parse as SimpleCommand: {}", e);
            }
        }

        // First test: Can we parse $(echo world) as a Word by itself?
        eprintln!("\n=== Test 1: Parse $(echo world) as Word ===");
        match ColonyParser::parse(Rule::Word, "$(echo world)") {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    eprintln!("✓ Word parsed: {:?}", pair.as_str());
                    for inner in pair.into_inner() {
                        eprintln!("  Part: {:?} = {:?}", inner.as_rule(), inner.as_str());
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ Cannot parse as Word: {}", e);
            }
        }

        // Second test: Can we parse "hello $(echo world)" as two Words?
        eprintln!("\n=== Test 2: Parse 'hello $(echo world)' as Words ===");
        let test2 = "hello $(echo world)";
        match ColonyParser::parse(Rule::SimpleCommand, test2) {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!("Matched: {:?} = {:?}", pair.as_rule(), pair.as_str());
                    for inner in pair.into_inner() {
                        eprintln!("  {:?} = {:?}", inner.as_rule(), inner.as_str());
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }

        let source = "echo hello $(echo world)";

        // Test raw pest parsing
        eprintln!("\n=== Test 3: Full command with $(echo world) ===");
        match ColonyParser::parse(Rule::SimpleCommand, source) {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!("Rule: {:?}, Text: {:?}, Span: {:?}", pair.as_rule(), pair.as_str(), pair.as_span());
                    for inner in pair.into_inner() {
                        eprintln!("  Inner Rule: {:?}, Text: {:?}", inner.as_rule(), inner.as_str());
                    }
                }
            }
            Err(e) => {
                eprintln!("Pest parse error: {}", e);
            }
        }

        // Now test parse_statement
        let result = parse_statement(source);

        if let Err(ref e) = result {
            eprintln!("Parse error: {}", e);
        }
        assert!(result.is_ok(), "Should parse command sub with args");

        let stmt = result.unwrap();
        eprintln!("\n=== AST Structure ===");
        eprintln!("{:#?}", stmt);

        // Navigate through List/Pipeline to get to SimpleCommand
        let cmd = match stmt {
            crate::ast::Statement::SimpleCommand(ref cmd) => cmd,
            crate::ast::Statement::List(ref list) => {
                match &*list.first {
                    crate::ast::Statement::Pipeline(ref pipeline) => {
                        if let crate::ast::Statement::SimpleCommand(ref cmd) = *pipeline.elements[0].command {
                            cmd
                        } else {
                            panic!("Pipeline doesn't contain SimpleCommand");
                        }
                    }
                    crate::ast::Statement::SimpleCommand(ref cmd) => cmd,
                    _ => panic!("List doesn't contain expected structure"),
                }
            }
            _ => panic!("Unexpected statement type"),
        };

        eprintln!("\n=== SimpleCommand Details ===");
        eprintln!("Command name: {:?}", cmd.name);
        eprintln!("Number of args: {}", cmd.args.len());
        for (i, arg) in cmd.args.iter().enumerate() {
            eprintln!("Arg {}: {:?}", i, arg);
        }

        assert_eq!(cmd.args.len(), 2, "Should have 2 arguments: 'hello' and $(echo world)");
    }

    #[test]
    fn test_heredoc_basic() {
        let source = r#"cat <<EOF
line 1
line 2
EOF"#;

        let result = parse(source);
        assert!(result.is_ok(), "Should parse basic heredoc: {:?}", result.err());
    }

    #[test]
    fn test_heredoc_strip_tabs() {
        let source = "cat <<-EOF\n\tindented content\nEOF";

        let result = parse(source);
        assert!(result.is_ok(), "Should parse <<- heredoc: {:?}", result.err());
    }

    #[test]
    fn test_heredoc_quoted_delimiter() {
        let source = r#"cat <<'EOF'
$var should not expand
EOF"#;

        let result = parse(source);
        assert!(result.is_ok(), "Should parse quoted delimiter: {:?}", result.err());
    }

    #[test]
    fn test_heredoc_in_pipeline() {
        let source = r#"cat <<EOF | grep pattern
search this content
and this too
EOF"#;

        let result = parse(source);
        assert!(result.is_ok(), "Should parse heredoc in pipeline: {:?}", result.err());
    }
}
