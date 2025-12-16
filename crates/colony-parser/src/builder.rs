//! AST Builder - transforms pest parse tree into Colony AST
//!
//! The pest parser generates a parse tree (Pairs<Rule>) which we need to
//! transform into our strongly-typed AST. This module walks the parse tree
//! and constructs AST nodes.

use pest::iterators::{Pair, Pairs};
use crate::pest_parser::{ColonyParser, Rule};
use crate::ast::*;
use crate::span::{Position, Span};

/// Builds AST from pest parse tree
pub struct AstBuilder;

impl AstBuilder {
    /// Build a complete program from parse tree
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
                _ => {
                    return Err(BuildError::UnexpectedRule {
                        expected: "Program",
                        found: format!("{:?}", pair.as_rule()),
                    });
                }
            }
        }

        Ok(Program { statements, span })
    }

    /// Build a statement from parse tree node
    pub fn build_statement(pair: Pair<Rule>) -> Result<Statement, BuildError> {
        match pair.as_rule() {
            Rule::SimpleCommand => {
                Self::build_simple_command(pair)
                    .map(Statement::SimpleCommand)
            }
            Rule::Pipeline => {
                Self::build_pipeline(pair)
                    .map(Statement::Pipeline)
            }
            Rule::CommandList => {
                Self::build_command_list(pair)
                    .map(Statement::List)
            }
            Rule::IfStatement => {
                Self::build_if_statement(pair)
                    .map(Statement::If)
            }
            Rule::ForStatement => {
                Self::build_for_statement(pair)
                    .map(Statement::For)
            }
            Rule::WhileStatement => {
                Self::build_while_statement(pair)
                    .map(Statement::While)
            }
            Rule::UntilStatement => {
                Self::build_until_statement(pair)
                    .map(Statement::Until)
            }
            Rule::VarDecl => {
                Self::build_var_decl(pair)
                    .map(Statement::VarDecl)
            }
            Rule::FunctionDef => {
                Self::build_function_def(pair)
                    .map(Statement::FunctionDef)
            }
            // Assignments are env assignments before commands, not standalone statements
            // Rule::Assignment handled in SimpleCommand
            Rule::EmptyStatement => {
                Ok(Statement::Empty)
            }
            Rule::Comment => {
                // Comments are typically ignored in the AST
                Ok(Statement::Empty)
            }
            _ => {
                Err(BuildError::UnexpectedRule {
                    expected: "Statement",
                    found: format!("{:?}", pair.as_rule()),
                })
            }
        }
    }

    /// Build a simple command: name args... redirects...
    fn build_simple_command(pair: Pair<Rule>) -> Result<SimpleCommand, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // SimpleCommand = Assignment* ~ Word ~ Word* ~ Redirect*
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

        if words.is_empty() {
            return Err(BuildError::EmptyCommand);
        }

        let name = words.remove(0);
        let args = words;

        Ok(SimpleCommand {
            name,
            args,
            redirects,
            env_assignments,
            span,
        })
    }

    /// Build a pipeline: cmd1 | cmd2 | cmd3
    fn build_pipeline(pair: Pair<Rule>) -> Result<Pipeline, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // Pipeline = "!"? ~ PipelineElement ~ (PipeOp ~ PipelineElement)*
        let mut elements = Vec::new();
        let mut negated = false;

        // Check for negation
        if let Some(first) = inner.peek() {
            if first.as_str() == "!" {
                negated = true;
                inner.next();
            }
        }

        // Parse elements
        let mut current_element: Option<Box<Statement>> = None;
        let mut current_pipe_op: Option<PipeOp> = None;

        for part in inner {
            match part.as_rule() {
                Rule::PipelineElement => {
                    let stmt = Self::build_pipeline_element(part)?;
                    if let Some(prev) = current_element {
                        elements.push(PipelineElement {
                            command: prev,
                            pipe_op: current_pipe_op,
                            negated: false,  // Only first element can be negated
                        });
                    }
                    current_element = Some(Box::new(stmt));
                    current_pipe_op = None;
                }
                Rule::PipeOp => {
                    current_pipe_op = Some(Self::build_pipe_op(part)?);
                }
                _ => {}
            }
        }

        // Add final element
        if let Some(last) = current_element {
            elements.push(PipelineElement {
                command: last,
                pipe_op: None,
                negated,
            });
        }

        Ok(Pipeline { elements, span })
    }

    fn build_pipeline_element(pair: Pair<Rule>) -> Result<Statement, BuildError> {
        let inner = pair.into_inner().next()
            .ok_or(BuildError::EmptyPipelineElement)?;

        Self::build_statement(inner)
    }

    fn build_pipe_op(pair: Pair<Rule>) -> Result<PipeOp, BuildError> {
        let op_str = pair.as_str();
        match op_str {
            "|" => Ok(PipeOp::Pipe),
            "->" => Ok(PipeOp::Arrow),
            "=>" => Ok(PipeOp::FatArrow),
            "~>" => Ok(PipeOp::Alter),
            "|&" => Ok(PipeOp::PipeErr),
            _ => Err(BuildError::InvalidPipeOp(op_str.to_string())),
        }
    }

    /// Build a command list: cmd1 && cmd2 || cmd3
    fn build_command_list(pair: Pair<Rule>) -> Result<CommandList, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // CommandList = AndOrList ~ (ListOp ~ AndOrList)*
        let mut lists = Vec::new();
        let mut ops = Vec::new();

        for part in inner {
            match part.as_rule() {
                Rule::AndOrList => {
                    lists.push(Self::build_andor_list(part)?);
                }
                Rule::ListOp => {
                    ops.push(Self::build_list_op(part)?);
                }
                _ => {}
            }
        }

        if lists.is_empty() {
            return Err(BuildError::EmptyCommandList);
        }

        // Convert to CommandList format: first + rest
        let first = Box::new(lists.remove(0));
        let rest: Vec<(ListOp, Statement)> = ops.into_iter()
            .zip(lists.into_iter())
            .collect();

        Ok(CommandList { first, rest, span })
    }

    fn build_andor_list(pair: Pair<Rule>) -> Result<Statement, BuildError> {
        // AndOrList = Pipeline ~ (AndOrOp ~ Pipeline)*
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let first_pair = inner.next()
            .ok_or(BuildError::EmptyAndOrList)?;
        let first = Self::build_statement(first_pair)?;

        // If there are no && or || operators, just return the single pipeline
        let mut ops_and_pipelines = Vec::new();

        let mut current_op: Option<ListOp> = None;
        for part in inner {
            match part.as_rule() {
                Rule::AndOrOp => {
                    let op_str = part.as_str();
                    current_op = Some(match op_str {
                        "&&" => ListOp::And,
                        "||" => ListOp::Or,
                        _ => return Err(BuildError::InvalidListOp(op_str.to_string())),
                    });
                }
                Rule::Pipeline => {
                    if let Some(op) = current_op.take() {
                        let pipeline = Self::build_statement(part)?;
                        ops_and_pipelines.push((op, pipeline));
                    }
                }
                _ => {}
            }
        }

        if ops_and_pipelines.is_empty() {
            Ok(first)
        } else {
            Ok(Statement::List(CommandList {
                first: Box::new(first),
                rest: ops_and_pipelines,
                span,
            }))
        }
    }

    fn build_list_op(pair: Pair<Rule>) -> Result<ListOp, BuildError> {
        let op_str = pair.as_str();
        match op_str {
            "&&" => Ok(ListOp::And),
            "||" => Ok(ListOp::Or),
            ";" => Ok(ListOp::Semi),
            "&" => Ok(ListOp::Background),
            _ => Err(BuildError::InvalidListOp(op_str.to_string())),
        }
    }

    /// Stub implementations for other statement types
    /// TODO: Implement these fully

    fn build_if_statement(_pair: Pair<Rule>) -> Result<IfStatement, BuildError> {
        // TODO: Implement if statement builder
        todo!("Implement if statement builder")
    }

    fn build_for_statement(_pair: Pair<Rule>) -> Result<ForStatement, BuildError> {
        // TODO: Implement for statement builder
        todo!("Implement for statement builder")
    }

    fn build_while_statement(_pair: Pair<Rule>) -> Result<WhileStatement, BuildError> {
        // TODO: Implement while statement builder
        todo!("Implement while statement builder")
    }

    fn build_until_statement(_pair: Pair<Rule>) -> Result<UntilStatement, BuildError> {
        // TODO: Implement until statement builder
        todo!("Implement until statement builder")
    }

    fn build_var_decl(_pair: Pair<Rule>) -> Result<VarDecl, BuildError> {
        // TODO: Implement var decl builder
        todo!("Implement var decl builder")
    }

    fn build_function_def(_pair: Pair<Rule>) -> Result<FunctionDef, BuildError> {
        // TODO: Implement function def builder
        todo!("Implement function def builder")
    }

    fn build_assignment(_pair: Pair<Rule>) -> Result<Assignment, BuildError> {
        // TODO: Implement assignment builder
        todo!("Implement assignment builder")
    }

    fn build_word(pair: Pair<Rule>) -> Result<Word, BuildError> {
        let span = span_from_pair(&pair);
        let text = pair.as_str();

        // For now, treat as simple literal
        // TODO: Handle WordParts properly (expansions, quotes, etc.)
        Ok(Word::literal(text, span))
    }

    fn build_redirect(_pair: Pair<Rule>) -> Result<Redirect, BuildError> {
        // TODO: Implement redirect builder
        todo!("Implement redirect builder")
    }
}

/// Convert pest Pair to our Span type
fn span_from_pair(pair: &Pair<Rule>) -> Span {
    let span = pair.as_span();
    let (start_line, start_col) = span.start_pos().line_col();
    let (end_line, end_col) = span.end_pos().line_col();

    Span {
        start: Position {
            offset: span.start(),
            line: start_line as u32,
            column: start_col as u32,
        },
        end: Position {
            offset: span.end(),
            line: end_line as u32,
            column: end_col as u32,
        },
        source_id: None,
    }
}

/// Errors during AST building
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("unexpected rule: expected {expected}, found {found}")]
    UnexpectedRule {
        expected: &'static str,
        found: String,
    },

    #[error("empty command")]
    EmptyCommand,

    #[error("empty pipeline element")]
    EmptyPipelineElement,

    #[error("empty command list")]
    EmptyCommandList,

    #[error("empty and-or list")]
    EmptyAndOrList,

    #[error("invalid pipe operator: {0}")]
    InvalidPipeOp(String),

    #[error("invalid list operator: {0}")]
    InvalidListOp(String),

    #[error("parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use pest::Parser;

    #[test]
    fn test_build_simple_command() {
        let source = "echo hello world";
        let pairs = ColonyParser::parse(Rule::SimpleCommand, source).unwrap();
        let result = AstBuilder::build_simple_command(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert_eq!(cmd.name.as_literal(), Some("echo"));
        assert_eq!(cmd.args.len(), 2);
    }

    #[test]
    fn test_build_pipeline() {
        let source = "cat file | grep pattern";
        let pairs = ColonyParser::parse(Rule::Pipeline, source).unwrap();
        let result = AstBuilder::build_pipeline(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let pipeline = result.unwrap();
        assert_eq!(pipeline.elements.len(), 2);
    }
}
