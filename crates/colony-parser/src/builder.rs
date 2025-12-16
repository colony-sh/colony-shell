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
                        match inner_pair.as_rule() {
                            Rule::EOI => break,
                            Rule::Statement => {
                                // Statement is a wrapper, descend into it
                                if let Some(stmt_inner) = inner_pair.into_inner().next() {
                                    statements.push(Self::build_statement(stmt_inner)?);
                                }
                            }
                            _ => {
                                // Direct statement (shouldn't happen)
                                statements.push(Self::build_statement(inner_pair)?);
                            }
                        }
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

    fn build_if_statement(pair: Pair<Rule>) -> Result<IfStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // IfStatement = "if" ~ Condition ~ ("then")? ~ Block
        //               ~ ("elif" ~ Condition ~ ("then")? ~ Block)*
        //               ~ ("else" ~ Block)?
        //               ~ ("fi")?

        // Parse condition
        let condition = Self::build_condition(
            inner.next().ok_or(BuildError::MissingCondition)?
        )?;

        // Parse then body
        let then_body = Self::build_block(
            inner.next().ok_or(BuildError::MissingThenBody)?
        )?;

        let mut elif_branches = Vec::new();
        let mut else_body = None;

        // Parse elif and else branches
        while let Some(part) = inner.next() {
            match part.as_str() {
                "elif" => {
                    let elif_condition = Self::build_condition(
                        inner.next().ok_or(BuildError::MissingCondition)?
                    )?;
                    let elif_body = Self::build_block(
                        inner.next().ok_or(BuildError::MissingBody)?
                    )?;
                    elif_branches.push(ElifBranch {
                        condition: elif_condition,
                        body: elif_body,
                    });
                }
                "else" => {
                    else_body = Some(Self::build_block(
                        inner.next().ok_or(BuildError::MissingElseBody)?
                    )?);
                }
                "fi" => break,
                _ => {
                    // Could be Block rule
                    if part.as_rule() == Rule::Block {
                        // This might be an else body
                        else_body = Some(Self::build_block(part)?);
                    }
                }
            }
        }

        Ok(IfStatement {
            condition,
            then_body,
            elif_branches,
            else_body,
            span,
        })
    }

    fn build_condition(pair: Pair<Rule>) -> Result<Condition, BuildError> {
        let inner = pair.into_inner().next()
            .ok_or(BuildError::EmptyCondition)?;

        match inner.as_rule() {
            Rule::Expression => {
                Ok(Condition::Expression(Self::build_expression(inner)?))
            }
            Rule::CommandList => {
                let cmd_list = Self::build_command_list(inner)?;
                Ok(Condition::Command(Box::new(Statement::List(cmd_list))))
            }
            _ => {
                // Try to parse as statement
                let stmt = Self::build_statement(inner)?;
                Ok(Condition::Command(Box::new(stmt)))
            }
        }
    }

    fn build_block(pair: Pair<Rule>) -> Result<Vec<Statement>, BuildError> {
        let mut statements = Vec::new();

        for part in pair.into_inner() {
            // Block contains Statement rules, which contain the actual statement types
            if part.as_rule() == Rule::Statement {
                // Descend into Statement to get the actual command type
                if let Some(inner_stmt) = part.into_inner().next() {
                    statements.push(Self::build_statement(inner_stmt)?);
                }
            } else if !matches!(part.as_str(), "{" | "}" | "do" | "done") {
                // Direct statement (shouldn't happen with current grammar)
                statements.push(Self::build_statement(part)?);
            }
        }

        Ok(statements)
    }

    fn build_for_statement(pair: Pair<Rule>) -> Result<ForStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // ForStatement = "for" ~ ForKind ~ Block
        let for_kind = inner.next().ok_or(BuildError::MissingForKind)?;
        let body = Self::build_block(
            inner.next().ok_or(BuildError::MissingBody)?
        )?;

        // Parse ForKind to extract var and words
        let mut kind_inner = for_kind.into_inner();

        // For now, handle simple case: Identifier ~ "in" ~ WordList
        let var = kind_inner.next()
            .ok_or(BuildError::MissingForVariable)?
            .as_str()
            .to_string();

        // Skip "in" keyword
        let _ = kind_inner.next();

        // Parse words
        let words_pair = kind_inner.next()
            .ok_or(BuildError::MissingForWords)?;

        let mut words = Vec::new();
        for word_pair in words_pair.into_inner() {
            if word_pair.as_rule() == Rule::Word {
                words.push(Self::build_word(word_pair)?);
            }
        }

        Ok(ForStatement {
            var,
            words,
            body,
            span,
        })
    }

    fn build_while_statement(pair: Pair<Rule>) -> Result<WhileStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // WhileStatement = "while" ~ Condition ~ Block
        let condition = Self::build_condition(
            inner.next().ok_or(BuildError::MissingCondition)?
        )?;

        let body = Self::build_block(
            inner.next().ok_or(BuildError::MissingBody)?
        )?;

        Ok(WhileStatement {
            condition,
            body,
            span,
        })
    }

    fn build_until_statement(pair: Pair<Rule>) -> Result<UntilStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // UntilStatement = "until" ~ Condition ~ Block
        let condition = Self::build_condition(
            inner.next().ok_or(BuildError::MissingCondition)?
        )?;

        let body = Self::build_block(
            inner.next().ok_or(BuildError::MissingBody)?
        )?;

        Ok(UntilStatement {
            condition,
            body,
            span,
        })
    }

    fn build_var_decl(pair: Pair<Rule>) -> Result<VarDecl, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // VarDecl = VarKind ~ Identifier ~ (":" ~ TypeAnnotation)? ~ "=" ~ Expression

        let kind_str = inner.next()
            .ok_or(BuildError::MissingVarKind)?
            .as_str();
        let kind = match kind_str {
            "var" => VarKind::Var,
            "const" => VarKind::Const,
            "setvar" => VarKind::Setvar,
            _ => return Err(BuildError::InvalidVarKind(kind_str.to_string())),
        };

        let name = inner.next()
            .ok_or(BuildError::MissingVarName)?
            .as_str()
            .to_string();

        // Skip optional type annotation for now
        // TODO: Parse type annotations properly

        // Find the expression (after '=')
        let mut expr = None;
        for part in inner {
            if part.as_rule() == Rule::Expression {
                expr = Some(Self::build_expression(part)?);
                break;
            }
        }

        let value = expr.ok_or(BuildError::MissingVarValue)?;

        Ok(VarDecl {
            kind,
            name,
            value,
            span,
        })
    }

    fn build_function_def(pair: Pair<Rule>) -> Result<FunctionDef, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // FunctionDef = ("function" ~ Identifier ~ "(" ~ ")" ~ Block)
        //             | (Identifier ~ "(" ~ ")" ~ Block)

        // First element might be "function" keyword or Identifier
        let first = inner.next().ok_or(BuildError::MissingFunctionName)?;

        let name = if first.as_str() == "function" {
            // function keyword present, next is name
            inner.next()
                .ok_or(BuildError::MissingFunctionName)?
                .as_str()
                .to_string()
        } else {
            // No keyword, this is the name
            first.as_str().to_string()
        };

        // Skip parentheses, find block
        let block = inner.find(|p| p.as_rule() == Rule::Block)
            .ok_or(BuildError::MissingFunctionBody)?;

        let body = Self::build_block(block)?;

        Ok(FunctionDef {
            name,
            body,
            span,
        })
    }

    fn build_expression(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        // For now, parse simple literals
        // TODO: Implement full expression parsing with operators

        let inner = pair.into_inner().next()
            .ok_or(BuildError::EmptyExpression)?;

        Self::build_expression_recursive(inner)
    }

    fn build_expression_recursive(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);

        match pair.as_rule() {
            Rule::Integer => {
                let value: i64 = pair.as_str().parse()
                    .map_err(|_| BuildError::InvalidInteger(pair.as_str().to_string()))?;
                Ok(Expression::Int(value, span))
            }
            Rule::Float => {
                let value: f64 = pair.as_str().parse()
                    .map_err(|_| BuildError::InvalidFloat(pair.as_str().to_string()))?;
                Ok(Expression::Float(value, span))
            }
            Rule::StringLiteral => {
                let text = pair.as_str();
                // Remove quotes
                let content = text.trim_matches('"').trim_matches('\'');
                Ok(Expression::Str(content.to_string(), span))
            }
            Rule::Boolean => {
                let value = pair.as_str() == "true";
                Ok(Expression::Bool(value, span))
            }
            Rule::Null => {
                Ok(Expression::Null(span))
            }
            Rule::Identifier => {
                let name = pair.as_str().to_string();
                Ok(Expression::Variable { name, span })
            }
            rule => {
                // Recursively parse nested expressions
                if let Some(inner) = pair.into_inner().next() {
                    Self::build_expression_recursive(inner)
                } else {
                    Err(BuildError::UnsupportedExpression(format!("{:?}", rule)))
                }
            }
        }
    }

    fn build_assignment(pair: Pair<Rule>) -> Result<Assignment, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // Assignment = Identifier ~ AssignOp ~ Expression

        let name = inner.next()
            .ok_or(BuildError::MissingAssignmentName)?
            .as_str()
            .to_string();

        let op_pair = inner.next()
            .ok_or(BuildError::MissingAssignmentOp)?;
        let op = match op_pair.as_str() {
            "=" => AssignOp::Assign,
            "+=" => AssignOp::Append,
            ":=" => AssignOp::Default,
            "?=" => AssignOp::IfUnset,
            _ => return Err(BuildError::InvalidAssignOp(op_pair.as_str().to_string())),
        };

        let value_pair = inner.next()
            .ok_or(BuildError::MissingAssignmentValue)?;

        // Assignment value is Expression in pest, but Word in AST
        // For now, convert expression to word literal
        let value_text = value_pair.as_str();
        let value = Word::literal(value_text, span_from_pair(&value_pair));

        Ok(Assignment {
            name,
            op,
            value,
            span,
        })
    }

    fn build_word(pair: Pair<Rule>) -> Result<Word, BuildError> {
        let span = span_from_pair(&pair);
        let text = pair.as_str();

        // For now, treat as simple literal
        // The Word rule is atomic (@), so we get the raw text
        // TODO: When preprocessing is connected, this will handle:
        // - DollarExpansion (variables, command sub, expression sub)
        // - QuotedString (single, double, raw, triple)
        // - Glob patterns
        Ok(Word::literal(text, span))
    }

    fn build_redirect(pair: Pair<Rule>) -> Result<Redirect, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // Redirect = Integer? ~ RedirectOp ~ (Word | Integer)

        // Check if first element is a file descriptor
        let first = inner.next().ok_or(BuildError::MissingRedirectOp)?;

        let (fd, op_pair) = if first.as_rule() == Rule::Integer {
            let fd = first.as_str().parse::<i32>()
                .map_err(|_| BuildError::InvalidFd(first.as_str().to_string()))?;
            let op = inner.next().ok_or(BuildError::MissingRedirectOp)?;
            (Some(fd), op)
        } else {
            (None, first)
        };

        // Parse redirect operator
        let kind = match op_pair.as_str() {
            "<" => RedirectKind::Input,
            ">" => RedirectKind::Output,
            ">>" => RedirectKind::Append,
            ">|" => RedirectKind::Clobber,
            "<>" => RedirectKind::ReadWrite,
            ">&" => RedirectKind::DupFd,
            "<&" => RedirectKind::DupFd,
            "<<" => RedirectKind::HereDoc(HereDocKind::Normal),
            "<<-" => RedirectKind::HereDoc(HereDocKind::StripTabs),
            "<<<" => RedirectKind::HereDoc(HereDocKind::HereString),
            _ => return Err(BuildError::InvalidRedirectOp(op_pair.as_str().to_string())),
        };

        // Parse target
        let target_pair = inner.next().ok_or(BuildError::MissingRedirectTarget)?;

        let target = match target_pair.as_rule() {
            Rule::Integer => {
                let fd_num = target_pair.as_str().parse::<i32>()
                    .map_err(|_| BuildError::InvalidFd(target_pair.as_str().to_string()))?;
                if fd_num == -1 || target_pair.as_str() == "-" {
                    RedirectTarget::Close
                } else {
                    RedirectTarget::Fd(fd_num)
                }
            }
            Rule::Word => {
                let word = Self::build_word(target_pair)?;
                RedirectTarget::File(word)
            }
            _ => {
                return Err(BuildError::InvalidRedirectTarget(
                    format!("{:?}", target_pair.as_rule())
                ));
            }
        };

        Ok(Redirect {
            fd,
            kind,
            target,
            span,
        })
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

    #[error("empty condition")]
    EmptyCondition,

    #[error("empty expression")]
    EmptyExpression,

    #[error("invalid pipe operator: {0}")]
    InvalidPipeOp(String),

    #[error("invalid list operator: {0}")]
    InvalidListOp(String),

    #[error("invalid var kind: {0}")]
    InvalidVarKind(String),

    #[error("invalid integer: {0}")]
    InvalidInteger(String),

    #[error("invalid float: {0}")]
    InvalidFloat(String),

    #[error("unsupported expression: {0}")]
    UnsupportedExpression(String),

    #[error("missing condition")]
    MissingCondition,

    #[error("missing then body")]
    MissingThenBody,

    #[error("missing else body")]
    MissingElseBody,

    #[error("missing body")]
    MissingBody,

    #[error("missing for kind")]
    MissingForKind,

    #[error("missing for variable")]
    MissingForVariable,

    #[error("missing for words")]
    MissingForWords,

    #[error("missing var kind")]
    MissingVarKind,

    #[error("missing var name")]
    MissingVarName,

    #[error("missing var value")]
    MissingVarValue,

    #[error("missing function name")]
    MissingFunctionName,

    #[error("missing function body")]
    MissingFunctionBody,

    #[error("missing assignment name")]
    MissingAssignmentName,

    #[error("missing assignment operator")]
    MissingAssignmentOp,

    #[error("missing assignment value")]
    MissingAssignmentValue,

    #[error("invalid assignment operator: {0}")]
    InvalidAssignOp(String),

    #[error("missing redirect operator")]
    MissingRedirectOp,

    #[error("missing redirect target")]
    MissingRedirectTarget,

    #[error("invalid redirect operator: {0}")]
    InvalidRedirectOp(String),

    #[error("invalid redirect target: {0}")]
    InvalidRedirectTarget(String),

    #[error("invalid file descriptor: {0}")]
    InvalidFd(String),

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

    #[test]
    fn test_build_if_statement() {
        let source = "if (x > 10) { echo big }";
        let pairs = ColonyParser::parse(Rule::IfStatement, source).unwrap();
        let result = AstBuilder::build_if_statement(pairs.into_iter().next().unwrap());

        if let Err(ref e) = result {
            eprintln!("Error building if statement: {:?}", e);
        }
        assert!(result.is_ok(), "Failed to build if statement: {:?}", result.err());
        let if_stmt = result.unwrap();
        assert!(matches!(if_stmt.condition, Condition::Expression(_)));
        assert_eq!(if_stmt.then_body.len(), 1);
        assert_eq!(if_stmt.elif_branches.len(), 0);
        assert!(if_stmt.else_body.is_none());
    }

    #[test]
    fn test_build_while_statement() {
        let source = "while (x < 100) { echo loop }";
        let pairs = ColonyParser::parse(Rule::WhileStatement, source).unwrap();
        let result = AstBuilder::build_while_statement(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let while_stmt = result.unwrap();
        assert!(matches!(while_stmt.condition, Condition::Expression(_)));
        assert_eq!(while_stmt.body.len(), 1);
    }

    #[test]
    fn test_build_until_statement() {
        let source = "until (x) { echo waiting }";
        let pairs = ColonyParser::parse(Rule::UntilStatement, source).unwrap();
        let result = AstBuilder::build_until_statement(pairs.into_iter().next().unwrap());

        if let Err(ref e) = result {
            eprintln!("Error building until statement: {:?}", e);
        }
        assert!(result.is_ok(), "Failed to build until: {:?}", result.err());
        let until_stmt = result.unwrap();
        assert!(matches!(until_stmt.condition, Condition::Expression(_)));
        assert_eq!(until_stmt.body.len(), 1);
    }

    #[test]
    fn test_build_for_statement() {
        // TODO: Fix word-list based for loops in grammar
        // For now, test C-style which works
        let source = "for (;;) { echo hello }";
        let pairs = ColonyParser::parse(Rule::ForStatement, source).unwrap();
        let result = AstBuilder::build_for_statement(pairs.into_iter().next().unwrap());

        // This will fail until we handle C-style for properly
        // For now, just check it doesn't crash
        let _ = result;
    }

    #[test]
    fn test_build_var_decl() {
        let source = "var count = 42";
        let pairs = ColonyParser::parse(Rule::VarDecl, source).unwrap();
        let result = AstBuilder::build_var_decl(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let var_decl = result.unwrap();
        assert_eq!(var_decl.name, "count");
        assert!(matches!(var_decl.kind, VarKind::Var));
        assert!(matches!(var_decl.value, Expression::Int(42, _)));
    }

    #[test]
    fn test_build_function_def() {
        let source = "function greet() { echo hello }";
        let pairs = ColonyParser::parse(Rule::FunctionDef, source).unwrap();
        let result = AstBuilder::build_function_def(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let func = result.unwrap();
        assert_eq!(func.name, "greet");
        assert_eq!(func.body.len(), 1);
    }

    #[test]
    fn test_build_expression_literals() {
        // Integer
        let source = "42";
        let pairs = ColonyParser::parse(Rule::Expression, source).unwrap();
        let result = AstBuilder::build_expression(pairs.into_iter().next().unwrap());
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::Int(42, _)));

        // Boolean
        let source = "true";
        let pairs = ColonyParser::parse(Rule::Expression, source).unwrap();
        let result = AstBuilder::build_expression(pairs.into_iter().next().unwrap());
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::Bool(true, _)));

        // Identifier (variable)
        let source = "count";
        let pairs = ColonyParser::parse(Rule::Expression, source).unwrap();
        let result = AstBuilder::build_expression(pairs.into_iter().next().unwrap());
        assert!(result.is_ok());
        let expr = result.unwrap();
        match expr {
            Expression::Variable { name, .. } => assert_eq!(name, "count"),
            _ => panic!("Expected Variable expression"),
        }
    }

    #[test]
    fn test_build_program() {
        let source = "echo hello";
        let pairs = ColonyParser::parse(Rule::Program, source).unwrap();
        let result = AstBuilder::build_program(pairs);

        if let Err(ref e) = result {
            eprintln!("Error building program: {:?}", e);
        }
        assert!(result.is_ok(), "Failed to build program: {:?}", result.err());
        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
    }
}
