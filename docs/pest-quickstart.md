# pest Parser - Quick Start Guide

This guide walks through implementing the pest-based parser for Colony Shell.

## Prerequisites

- Rust 1.70+
- Existing preprocessor (F001 lexer with mode stack)
- Existing expansion engine (F003)
- Existing AST types (enhanced AST from F002)

## Step 1: Add Dependencies

**crates/colony-parser/Cargo.toml**:
```toml
[package]
name = "colony-parser"
version = "0.1.0"
edition = "2021"

[dependencies]
pest = "2.7"
pest_derive = "2.7"
thiserror = "1.0"

[dev-dependencies]
pretty_assertions = "1.4"
```

## Step 2: Create Parser Module

**crates/colony-parser/src/pest_parser.rs**:
```rust
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "../../../docs/colony.pest"]  // Path relative to src/
pub struct ColonyParser;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let source = "echo hello world";
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_if_statement() {
        let source = "if (x > 10) { echo big }";
        let result = ColonyParser::parse(Rule::IfStatement, source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_pipeline() {
        let source = "cat file.txt | grep pattern | wc -l";
        let result = ColonyParser::parse(Rule::Pipeline, source);
        assert!(result.is_ok());
    }
}
```

## Step 3: Validate Grammar

```bash
# Install pest formatter
cargo install pest_fmt

# Format and validate grammar
pest_fmt docs/colony.pest

# Run basic parser tests
cargo test -p colony-parser
```

Expected output:
```
running 3 tests
test pest_parser::tests::test_parse_simple_command ... ok
test pest_parser::tests::test_parse_if_statement ... ok
test pest_parser::tests::test_parse_pipeline ... ok
```

## Step 4: Build AST from Parse Tree

**crates/colony-parser/src/builder.rs**:
```rust
use pest::iterators::{Pair, Pairs};
use crate::ast::*;
use crate::pest_parser::{ColonyParser, Rule};

pub struct AstBuilder;

impl AstBuilder {
    pub fn build_program(pairs: Pairs<Rule>) -> Result<Program, BuildError> {
        let mut statements = Vec::new();
        let mut span = Span::DUMMY;

        for pair in pairs {
            match pair.as_rule() {
                Rule::Program => {
                    span = span_from_pair(&pair);
                    for inner_pair in pair.into_inner() {
                        if matches!(inner_pair.as_rule(), Rule::EOI) {
                            break;
                        }
                        statements.push(Self::build_statement(inner_pair)?);
                    }
                }
                _ => unreachable!("Expected Program rule"),
            }
        }

        Ok(Program { statements, span })
    }

    fn build_statement(pair: Pair<Rule>) -> Result<Statement, BuildError> {
        match pair.as_rule() {
            Rule::SimpleCommand => {
                Self::build_simple_command(pair)
                    .map(Statement::SimpleCommand)
            }
            Rule::Pipeline => {
                Self::build_pipeline(pair)
                    .map(Statement::Pipeline)
            }
            Rule::IfStatement => {
                Self::build_if_statement(pair)
                    .map(Statement::If)
            }
            Rule::CommandList => {
                Self::build_command_list(pair)
                    .map(Statement::List)
            }
            // ... other statement types
            _ => Err(BuildError::UnexpectedRule {
                expected: "Statement",
                found: format!("{:?}", pair.as_rule()),
            }),
        }
    }

    fn build_simple_command(pair: Pair<Rule>) -> Result<SimpleCommand, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // Parse: Assignment* Word Word* Redirect*
        let mut env_assignments = Vec::new();
        let mut words = Vec::new();
        let mut redirects = Vec::new();

        for part in inner {
            match part.as_rule() {
                Rule::Assignment => {
                    env_assignments.push(Self::build_assignment(part)?);
                }
                Rule::Word => {
                    words.push(Self::build_word(part)?);
                }
                Rule::Redirect => {
                    redirects.push(Self::build_redirect(part)?);
                }
                _ => {}
            }
        }

        let name = words.into_iter().next()
            .ok_or(BuildError::EmptyCommand)?;
        let args = words;

        Ok(SimpleCommand {
            name,
            args,
            redirects,
            env_assignments,
            span,
        })
    }

    // ... implement other build methods
}

fn span_from_pair(pair: &Pair<Rule>) -> Span {
    let span = pair.as_span();
    Span {
        start: Position {
            offset: span.start(),
            line: span.start_pos().line_col().0 as u32,
            column: span.start_pos().line_col().1 as u32,
        },
        end: Position {
            offset: span.end(),
            line: span.end_pos().line_col().0 as u32,
            column: span.end_pos().line_col().1 as u32,
        },
        source_id: None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("unexpected rule: expected {expected}, found {found}")]
    UnexpectedRule {
        expected: &'static str,
        found: String,
    },

    #[error("empty command")]
    EmptyCommand,

    #[error("parse error: {0}")]
    ParseError(String),
}
```

## Step 5: Connect Preprocessing

**crates/colony-parser/src/lib.rs**:
```rust
mod pest_parser;
mod builder;
pub mod ast;

use pest::Parser;
use pest_parser::{ColonyParser, Rule};
use builder::AstBuilder;

pub fn parse(source: &str) -> Result<ast::Program, ParseError> {
    // Phase 1: Preprocess (existing F001 lexer)
    let preprocessed = preprocess(source)?;

    // Phase 2: Parse with pest
    let pairs = ColonyParser::parse(Rule::Program, &preprocessed)
        .map_err(|e| ParseError::Syntax(e.to_string()))?;

    // Phase 3: Build AST
    let program = AstBuilder::build_program(pairs)
        .map_err(|e| ParseError::Build(e))?;

    Ok(program)
}

fn preprocess(source: &str) -> Result<String, ParseError> {
    // TODO: Use existing F001 lexer with mode stack
    // For now, pass through unchanged
    Ok(source.to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("syntax error: {0}")]
    Syntax(String),

    #[error("build error: {0}")]
    Build(#[from] builder::BuildError),

    #[error("preprocessing error: {0}")]
    Preprocess(String),
}
```

## Step 6: Write Integration Tests

**crates/colony-parser/tests/integration.rs**:
```rust
use colony_parser::parse;

#[test]
fn test_posix_if_statement() {
    let source = r#"
        if [ $x -gt 10 ]; then
            echo "big"
        fi
    "#;

    let result = parse(source);
    assert!(result.is_ok());

    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_colony_if_expression() {
    let source = r#"
        if (x > 10) {
            echo "big"
        }
    "#;

    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_pipeline() {
    let source = "cat file.txt | grep pattern | wc -l";

    let result = parse(source);
    assert!(result.is_ok());

    let program = result.unwrap();
    match &program.statements[0] {
        Statement::Pipeline(pipeline) => {
            assert_eq!(pipeline.elements.len(), 3);
        }
        _ => panic!("Expected pipeline"),
    }
}

#[test]
fn test_for_loop() {
    let source = r#"
        for file in *.txt; do
            echo $file
        done
    "#;

    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_variable_expansion() {
    let source = r#"
        echo "Count: $count"
        echo "Default: ${missing:-fallback}"
    "#;

    let result = parse(source);
    assert!(result.is_ok());
}
```

## Step 7: Benchmark Against Recursive Descent

**benches/parser_comparison.rs**:
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use colony_parser::parse;

fn bench_simple_command(c: &mut Criterion) {
    let source = "echo hello world";

    c.bench_function("pest: simple command", |b| {
        b.iter(|| parse(black_box(source)))
    });
}

fn bench_complex_script(c: &mut Criterion) {
    let source = r#"
        if [ $# -eq 0 ]; then
            echo "No arguments"
            exit 1
        fi

        for arg in "$@"; do
            if [ -f "$arg" ]; then
                cat "$arg" | grep pattern | wc -l
            else
                echo "Not found: $arg"
            fi
        done
    "#;

    c.bench_function("pest: complex script", |b| {
        b.iter(|| parse(black_box(source)))
    });
}

criterion_group!(benches, bench_simple_command, bench_complex_script);
criterion_main!(benches);
```

Run benchmarks:
```bash
cargo bench -p colony-parser
```

## Step 8: Validate Against F001-F004 Tests

```bash
# Run existing parser tests
cargo test -p colony-parser --test parser_tests

# Expected: All 28 tests pass
# - 12 lexer tests (F001)
# - 16 parser tests (F002)
```

## Step 9: Error Message Comparison

Test error quality:

**Old recursive descent**:
```
Error: unexpected token: expected 'fi', found 'done'
  at line 5
```

**New pest parser**:
```
Error: expected "fi"
  --> script.col:5:8
   |
 5 |     done
   |     ^---
   |
   = expected "fi" to close if statement starting at line 1
```

## Troubleshooting

### Issue: Grammar doesn't compile

```
error: rule `Expression` is left-recursive
```

**Solution**: pest doesn't support left recursion. Use iteration instead:
```pest
# Wrong (left-recursive):
Expression = { Expression "+" Expression }

# Right (iterative):
Expression = { Term ~ ("+" ~ Term)* }
```

### Issue: Preprocessor integration unclear

**Solution**: The preprocessor should:
1. Handle mode stack (Command/Expression/Quote contexts)
2. Emit a **string** that pest can parse
3. Annotate tokens with mode information (comments or special markers)

Example preprocessed output:
```
# MODE: Expression
if ( x > 10 ) {
# MODE: Command
    echo "big"
}
```

### Issue: Parse tree is too verbose

**Solution**: Use atomic (`@`) and silent (`_`) rules:
```pest
# Before: Creates intermediate nodes
Integer = { ASCII_DIGIT+ }

# After: Single token, no tree
Integer = @{ ASCII_DIGIT+ }
```

## Next Steps

1. **Complete builder.rs**: Implement all `build_*` methods
2. **Connect F001 preprocessor**: Replace `preprocess()` stub
3. **Integrate F003 expander**: Hook into AST after building
4. **Performance tuning**: Profile and optimize hot paths
5. **Error recovery**: Add better error messages for common mistakes

## Resources

- [pest book](https://pest.rs/book/)
- [pest playground](https://pest.rs/#editor)
- [Colony grammar](../docs/colony.pest)
- [PEG proposal](../docs/peg-proposal.md)

---

**Next**: See `docs/peg-proposal.md` for full architecture details.
