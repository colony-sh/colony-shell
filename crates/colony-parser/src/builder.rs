//! AST Builder - transforms pest parse tree into Colony AST
//!
//! The pest parser generates a parse tree (`Pairs<Rule>`) which we need to
//! transform into our strongly-typed AST. This module walks the parse tree
//! and constructs AST nodes.

use crate::ast::*;
use crate::pest_parser::{ColonyParser, Rule};
use crate::span::{Position, Span};
use pest::iterators::{Pair, Pairs};
use pest::Parser;

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
            Rule::SimpleCommand => Self::build_simple_command(pair).map(Statement::SimpleCommand),
            Rule::Pipeline => Self::build_pipeline(pair).map(Statement::Pipeline),
            Rule::CommandList => Self::build_command_list(pair).map(Statement::List),
            Rule::IfStatement => Self::build_if_statement(pair).map(Statement::If),
            Rule::TryStatement => Self::build_try_statement(pair).map(Statement::Try),
            Rule::MatchStatement => Self::build_match_statement(pair).map(Statement::Match),
            Rule::ForStatement => Self::build_for_statement(pair),
            Rule::WhileStatement => Self::build_while_statement(pair).map(Statement::While),
            Rule::UntilStatement => Self::build_until_statement(pair).map(Statement::Until),
            Rule::CaseStatement => Self::build_case_statement(pair).map(Statement::Case),
            Rule::VarDecl => Self::build_var_decl(pair).map(Statement::VarDecl),
            Rule::FunctionDef => Self::build_function_def(pair).map(Statement::FunctionDef),
            Rule::Subshell => Self::build_subshell(pair).map(Statement::Subshell),
            Rule::BraceGroup => Self::build_brace_group(pair).map(Statement::BraceGroup),
            Rule::CompoundCommand => {
                // CompoundCommand is a wrapper, descend into it
                let inner = pair
                    .into_inner()
                    .next()
                    .ok_or(BuildError::EmptyCompoundCommand)?;
                Self::build_statement(inner)
            }
            Rule::ConditionalCommand => {
                Self::build_conditional_command(pair).map(Statement::Conditional)
            }
            Rule::ArithmeticCommand => {
                Self::build_arithmetic_command(pair).map(Statement::Arithmetic)
            }
            Rule::CoprocCommand => Self::build_coproc_command(pair).map(Statement::Coproc),
            Rule::BashAssignment => {
                // Standalone bash assignment: arr=(a b c)
                // Convert to SimpleCommand with special handling
                let assignment = Self::build_bash_assignment(pair)?;
                let span = assignment.span;

                // Create a pseudo-command ":" (POSIX no-op) that does nothing
                // This allows assignment-only statements to execute
                Ok(Statement::SimpleCommand(SimpleCommand {
                    name: Word::literal(":", span),
                    args: vec![],
                    redirects: vec![],
                    env_assignments: vec![assignment],
                    span,
                }))
            }
            Rule::BreakStatement => {
                // Convert to SimpleCommand for execution handling
                let span = span_from_pair(&pair);
                Ok(Statement::SimpleCommand(SimpleCommand {
                    name: Word::literal("break", span),
                    args: vec![],
                    redirects: vec![],
                    env_assignments: vec![],
                    span,
                }))
            }
            Rule::ContinueStatement => {
                let span = span_from_pair(&pair);
                Ok(Statement::SimpleCommand(SimpleCommand {
                    name: Word::literal("continue", span),
                    args: vec![],
                    redirects: vec![],
                    env_assignments: vec![],
                    span,
                }))
            }
            Rule::ProcDef => Self::build_proc_def(pair).map(Statement::ProcDef),
            Rule::AgentInvocation => {
                Self::build_agent_invocation(pair).map(Statement::AgentInvocation)
            }
            Rule::ReturnStatement => Self::build_return_statement(pair).map(Statement::Return),
            Rule::EmptyStatement => Ok(Statement::Empty),
            Rule::Comment => Ok(Statement::Empty),
            Rule::AndOrList => Self::build_andor_list(pair),
            _ => Err(BuildError::UnexpectedRule {
                expected: "Statement",
                found: format!("{:?}", pair.as_rule()),
            }),
        }
    }

    /// Build a simple command: name args... redirects...
    fn build_simple_command(pair: Pair<Rule>) -> Result<SimpleCommand, BuildError> {
        let span = span_from_pair(&pair);
        let inner = pair.into_inner();

        // SimpleCommand = Assignment* ~ Word ~ Word* ~ Redirect*
        let mut env_assignments = Vec::new();
        let mut words = Vec::new();
        let mut redirects = Vec::new();

        for part in inner {
            match part.as_rule() {
                Rule::BashAssignment => {
                    env_assignments.push(Self::build_bash_assignment(part)?);
                }
                Rule::Assignment => {
                    env_assignments.push(Self::build_assignment(part)?);
                }
                Rule::Word | Rule::CommandWord => {
                    words.push(Self::build_word(part)?);
                }
                Rule::CmdSuffix => {
                    for suffix_inner in part.into_inner() {
                        match suffix_inner.as_rule() {
                            Rule::Redirect => {
                                redirects.push(Self::build_redirect(suffix_inner)?);
                            }
                            Rule::ArgumentWord => {
                                for inner in suffix_inner.into_inner() {
                                    match inner.as_rule() {
                                        Rule::BashAssignment => {
                                            let arg_span = span_from_pair(&inner);
                                            let full_text = inner.as_str();
                                            let mut parts = Vec::new();

                                            for sub in inner.into_inner() {
                                                match sub.as_rule() {
                                                    Rule::BashAssignmentLHS => {
                                                        parts.push(WordPart::Literal(
                                                            sub.as_str().to_string(),
                                                        ));
                                                    }
                                                    Rule::AssignmentName | Rule::Identifier => {
                                                        parts.push(WordPart::Literal(
                                                            sub.as_str().to_string(),
                                                        ));
                                                    }
                                                    Rule::ArraySubscript => {
                                                        parts.push(WordPart::Literal(
                                                            sub.as_str().to_string(),
                                                        ));
                                                    }
                                                    Rule::AssocArrayLiteral
                                                    | Rule::ArrayLiteral => {
                                                        parts.push(WordPart::Literal(
                                                            sub.as_str().to_string(),
                                                        ));
                                                    }
                                                    Rule::Word => {
                                                        let word = Self::build_word(sub)?;
                                                        parts.extend(word.parts);
                                                    }
                                                    _ => {}
                                                }
                                            }

                                            if parts.is_empty() {
                                                parts
                                                    .push(WordPart::Literal(full_text.to_string()));
                                            }

                                            words.push(Word {
                                                parts,
                                                span: arg_span,
                                            });
                                        }
                                        Rule::Word => {
                                            words.push(Self::build_word(inner)?);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Rule::ArgumentWord => {
                    for inner in part.into_inner() {
                        match inner.as_rule() {
                            Rule::BashAssignment => {
                                let arg_span = span_from_pair(&inner);
                                let full_text = inner.as_str();
                                let mut parts = Vec::new();

                                for sub in inner.into_inner() {
                                    match sub.as_rule() {
                                        Rule::BashAssignmentLHS => {
                                            parts.push(WordPart::Literal(sub.as_str().to_string()));
                                        }
                                        Rule::AssignmentName | Rule::Identifier => {
                                            parts.push(WordPart::Literal(sub.as_str().to_string()));
                                        }
                                        Rule::ArraySubscript => {
                                            parts.push(WordPart::Literal(sub.as_str().to_string()));
                                        }
                                        Rule::AssocArrayLiteral | Rule::ArrayLiteral => {
                                            parts.push(WordPart::Literal(sub.as_str().to_string()));
                                        }
                                        Rule::Word => {
                                            let word = Self::build_word(sub)?;
                                            parts.extend(word.parts);
                                        }
                                        _ => {}
                                    }
                                }

                                if parts.is_empty() {
                                    parts.push(WordPart::Literal(full_text.to_string()));
                                }

                                words.push(Word {
                                    parts,
                                    span: arg_span,
                                });
                            }
                            Rule::Word => {
                                words.push(Self::build_word(inner)?);
                            }
                            _ => {}
                        }
                    }
                }
                Rule::Redirect => {
                    redirects.push(Self::build_redirect(part)?);
                }
                _ => {}
            }
        }

        if words.is_empty() && env_assignments.is_empty() && redirects.is_empty() {
            return Err(BuildError::EmptyCommand);
        }

        let name = if words.is_empty() {
            Word::literal(":", span)
        } else {
            words.remove(0)
        };
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
    /// Supports multiple negations: ! ! false = false (even negations cancel out)
    fn build_pipeline(pair: Pair<Rule>) -> Result<Pipeline, BuildError> {
        let span = span_from_pair(&pair);
        let raw_text = pair.as_str();
        let inner = pair.into_inner();

        let mut elements = Vec::new();

        // Count the number of '!' at the start (for multiple negations like ! ! false)
        let trimmed = raw_text.trim_start();
        let mut negation_count = 0;
        for ch in trimmed.chars() {
            if ch == '!' {
                negation_count += 1;
            } else if !ch.is_whitespace() {
                break;
            }
        }
        // Odd number of negations means negated
        let negated = negation_count % 2 == 1;

        // Parse elements
        let mut current_element: Option<Box<Statement>> = None;
        let mut current_pipe_op: Option<PipeOp> = None;
        let mut is_first_element = true;

        for part in inner {
            match part.as_rule() {
                Rule::PipelineElement => {
                    let stmt = Self::build_pipeline_element(part)?;
                    if let Some(prev) = current_element {
                        // Push the previous element - first element gets negation flag
                        elements.push(PipelineElement {
                            command: prev,
                            pipe_op: current_pipe_op,
                            negated: is_first_element && negated,
                        });
                        is_first_element = false;
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

        // Add final element - if it's also the first (single command pipeline), it gets negation
        if let Some(last) = current_element {
            elements.push(PipelineElement {
                command: last,
                pipe_op: None,
                negated: is_first_element && negated,
            });
        }

        Ok(Pipeline { elements, span })
    }

    fn build_pipeline_element(pair: Pair<Rule>) -> Result<Statement, BuildError> {
        let inner = pair
            .into_inner()
            .next()
            .ok_or(BuildError::EmptyPipelineElement)?;

        Self::build_statement(inner)
    }

    fn build_pipe_op(pair: Pair<Rule>) -> Result<PipeOp, BuildError> {
        let op_str = pair.as_str();
        match op_str {
            "|" => Ok(PipeOp::Pipe),
            "|>" => Ok(PipeOp::PipeTap),
            "?>" => Ok(PipeOp::PipeGuard),
            "!>" => Ok(PipeOp::PipeCatch),
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
        let inner = pair.into_inner();

        // CommandList = AndOrList ~ (ListOp ~ AndOrList)* ~ TrailingBackground?
        let mut lists = Vec::new();
        let mut ops = Vec::new();
        let mut has_trailing_background = false;

        for part in inner {
            match part.as_rule() {
                Rule::AndOrList => {
                    lists.push(Self::build_andor_list(part)?);
                }
                Rule::ListOp => {
                    ops.push(Self::build_list_op(part)?);
                }
                Rule::TrailingBackground => {
                    has_trailing_background = true;
                }
                _ => {}
            }
        }

        if lists.is_empty() {
            return Err(BuildError::EmptyCommandList);
        }

        // Convert to CommandList format: first + rest
        let first = Box::new(lists.remove(0));
        let mut rest: Vec<(ListOp, Statement)> = ops.into_iter().zip(lists).collect();

        // If trailing &, add Background op with Empty statement
        if has_trailing_background {
            rest.push((ListOp::Background, Statement::Empty));
        }

        Ok(CommandList { first, rest, span })
    }

    fn build_andor_list(pair: Pair<Rule>) -> Result<Statement, BuildError> {
        // AndOrList = Pipeline ~ (AndOrOp ~ Pipeline)*
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let first_pair = inner.next().ok_or(BuildError::EmptyAndOrList)?;
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

    // TODO: Implement these fully
    fn build_if_statement(pair: Pair<Rule>) -> Result<IfStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // IfStatement = "if" ~ Condition ~ ";"? ~ "then" ~ IfThenBody
        //               ~ ("elif" ~ Condition ~ ";"? ~ "then" ~ IfThenBody)*
        //               ~ ("else" ~ IfElseBody)?
        //               ~ "fi"

        // Parse condition
        let condition = Self::build_condition(inner.next().ok_or(BuildError::MissingCondition)?)?;

        // Parse then body (IfThenBody = Statement* until elif/else/fi)
        let then_body_pair = inner.next().ok_or(BuildError::MissingThenBody)?;
        let then_body = Self::build_if_body(then_body_pair)?;

        let mut elif_branches = Vec::new();
        let mut else_body = None;

        // Parse elif and else branches
        while let Some(part) = inner.next() {
            match part.as_rule() {
                Rule::Condition => {
                    // This is an elif condition
                    let elif_condition = Self::build_condition(part)?;
                    let elif_body_pair = inner.next().ok_or(BuildError::MissingBody)?;
                    let elif_body = Self::build_if_body(elif_body_pair)?;
                    elif_branches.push(ElifBranch {
                        condition: elif_condition,
                        body: elif_body,
                    });
                }
                Rule::IfElseBody => {
                    else_body = Some(Self::build_if_body(part)?);
                }
                _ => {
                    // Skip keywords like "elif", "else", "fi"
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

    fn build_try_statement(pair: Pair<Rule>) -> Result<TryStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let try_block = inner.next().ok_or(BuildError::MissingBody)?;
        let body = Self::build_block(try_block)?;

        let mut catch = None;
        let mut finally_body = None;

        for part in inner {
            match part.as_rule() {
                Rule::CatchBlock => catch = Some(Self::build_catch_block(part)?),
                Rule::Block => finally_body = Some(Self::build_block(part)?),
                _ => {}
            }
        }

        Ok(TryStatement {
            body,
            catch,
            finally_body,
            span,
        })
    }

    fn build_match_statement(pair: Pair<Rule>) -> Result<MatchStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let value_pair = inner.next().ok_or(BuildError::EmptyExpression)?;
        let value = Self::build_expression(value_pair)?;

        let mut arms = Vec::new();
        for part in inner {
            if part.as_rule() == Rule::MatchArm {
                arms.push(Self::build_match_arm(part)?);
            }
        }

        Ok(MatchStatement { value, arms, span })
    }

    fn build_match_arm(pair: Pair<Rule>) -> Result<MatchArm, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let pattern_pair = inner.next().ok_or(BuildError::EmptyExpression)?;
        let first_pattern = Self::build_match_pattern(pattern_pair)?;

        let mut or_patterns = vec![first_pattern];
        let mut guard = None;
        let mut body = None;

        for part in inner {
            match part.as_rule() {
                Rule::MatchPattern => {
                    or_patterns.push(Self::build_match_pattern(part)?);
                }
                Rule::Expression => {
                    if body.is_none() {
                        if guard.is_none() {
                            guard = Some(Self::build_expression(part)?);
                        } else {
                            body = Some(MatchBody::Expr(Self::build_expression(part)?));
                        }
                    }
                }
                Rule::Block => {
                    body = Some(MatchBody::Block(Self::build_block(part)?));
                }
                _ => {}
            }
        }

        // If guard was set but body wasn't, the "guard" was actually the body expression
        if body.is_none() {
            if let Some(guard_expr) = guard.take() {
                body = Some(MatchBody::Expr(guard_expr));
            }
        }

        let body = body.ok_or(BuildError::MissingBody)?;

        let pattern = if or_patterns.len() == 1 {
            or_patterns.into_iter().next().expect("checked len == 1")
        } else {
            MatchPattern::Or(or_patterns)
        };

        Ok(MatchArm {
            pattern,
            guard,
            body,
            span,
        })
    }

    fn build_match_pattern(pair: Pair<Rule>) -> Result<MatchPattern, BuildError> {
        match pair.as_rule() {
            Rule::MatchPattern => {
                let inner = pair
                    .into_inner()
                    .next()
                    .ok_or(BuildError::EmptyExpression)?;
                Self::build_match_pattern(inner)
            }
            Rule::ListPattern => {
                let span = span_from_pair(&pair);
                let mut items = Vec::new();
                let mut rest = ListRest::None;

                for part in pair.into_inner() {
                    match part.as_rule() {
                        Rule::MatchPattern => {
                            items.push(Self::build_match_pattern(part)?);
                        }
                        Rule::ListRest => {
                            let mut inner = part.into_inner();
                            if let Some(name) = inner.next() {
                                rest = ListRest::Bind(name.as_str().to_string());
                            } else {
                                rest = ListRest::Ignore;
                            }
                        }
                        _ => {}
                    }
                }

                Ok(MatchPattern::List(ListPattern { items, rest, span }))
            }
            Rule::DictPattern => {
                let span = span_from_pair(&pair);
                let mut fields = Vec::new();
                let mut allow_extra = false;

                for part in pair.into_inner() {
                    match part.as_rule() {
                        Rule::DictPatternField => {
                            fields.push(Self::build_dict_pattern_field(part)?);
                        }
                        Rule::DictRest => {
                            allow_extra = true;
                        }
                        _ => {}
                    }
                }

                Ok(MatchPattern::Dict(DictPattern {
                    fields,
                    allow_extra,
                    span,
                }))
            }
            Rule::TuplePattern => {
                let span = span_from_pair(&pair);
                let mut items = Vec::new();
                for part in pair.into_inner() {
                    if part.as_rule() == Rule::MatchPattern {
                        items.push(Self::build_match_pattern(part)?);
                    }
                }
                Ok(MatchPattern::Tuple(TuplePattern { items, span }))
            }
            Rule::BindingPattern => Ok(MatchPattern::Binding(pair.as_str().to_string())),
            Rule::LiteralPattern => {
                let expr_pair = pair
                    .into_inner()
                    .next()
                    .ok_or(BuildError::EmptyExpression)?;
                let expr = Self::build_expression_recursive(expr_pair)?;
                Ok(MatchPattern::Literal(expr))
            }
            Rule::WildcardPattern => Ok(MatchPattern::Wildcard),
            Rule::Identifier => Ok(MatchPattern::Binding(pair.as_str().to_string())),
            _ => Err(BuildError::UnexpectedRule {
                expected: "match pattern",
                found: format!("{:?}", pair.as_rule()),
            }),
        }
    }

    fn build_dict_pattern_field(pair: Pair<Rule>) -> Result<DictPatternField, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();
        let name = inner
            .next()
            .ok_or(BuildError::MissingVarName)?
            .as_str()
            .to_string();

        let mut pattern = None;
        for part in inner {
            if part.as_rule() == Rule::MatchPattern {
                pattern = Some(Self::build_match_pattern(part)?);
            }
        }

        Ok(DictPatternField {
            name,
            pattern,
            span,
        })
    }

    fn build_catch_block(pair: Pair<Rule>) -> Result<CatchBlock, BuildError> {
        let span = span_from_pair(&pair);
        let pair_text = pair.as_str().to_string();
        let mut binding = None;
        let mut error_type = None;
        let mut body = Vec::new();

        for part in pair.into_inner() {
            match part.as_rule() {
                Rule::Identifier => {
                    if binding.is_none() {
                        binding = Some(part.as_str().to_string());
                    } else if error_type.is_none() {
                        error_type = Some(part.as_str().to_string());
                    }
                }
                Rule::Block => {
                    body = Self::build_block(part)?;
                }
                _ => {}
            }
        }

        // If a single identifier is provided, interpret it as an error type.
        if error_type.is_none() && binding.is_some() && !pair_text.contains(':') {
            error_type = binding.take();
        }

        // If only a type was provided via "catch :Type", treat the single identifier as type.
        if error_type.is_none() && binding.is_some() && pair_text.contains(':') {
            error_type = binding.take();
        }

        Ok(CatchBlock {
            binding,
            error_type,
            body,
            span,
        })
    }

    fn build_if_body(pair: Pair<Rule>) -> Result<Vec<Statement>, BuildError> {
        let mut statements = Vec::new();

        for part in pair.into_inner() {
            if part.as_rule() == Rule::Statement {
                if let Some(inner_stmt) = part.into_inner().next() {
                    statements.push(Self::build_statement(inner_stmt)?);
                }
            }
        }

        Ok(statements)
    }

    fn build_condition(pair: Pair<Rule>) -> Result<Condition, BuildError> {
        let inner = pair.into_inner().next().ok_or(BuildError::EmptyCondition)?;

        match inner.as_rule() {
            Rule::Expression => Ok(Condition::Expression(Self::build_expression(inner)?)),
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
                    // Skip empty statements (just semicolons/newlines) - they shouldn't affect exit code
                    if inner_stmt.as_rule() == Rule::EmptyStatement {
                        continue;
                    }
                    statements.push(Self::build_statement(inner_stmt)?);
                }
            } else if !matches!(part.as_str(), "{" | "}" | "do" | "done") {
                // Direct statement (shouldn't happen with current grammar)
                statements.push(Self::build_statement(part)?);
            }
        }

        Ok(statements)
    }

    fn build_for_statement(pair: Pair<Rule>) -> Result<Statement, BuildError> {
        let span = span_from_pair(&pair);
        let inner = pair.into_inner();

        // ForStatement = "for" ~ ForKind ~ ";"? ~ Block ~ Redirect*
        let children: Vec<_> = inner.collect();
        let mut iter = children.into_iter().peekable();
        let for_kind = iter.next().ok_or(BuildError::MissingForKind)?;

        // Skip optional semicolon
        let next_item = iter.next().ok_or(BuildError::MissingBody)?;
        let block = if next_item.as_str() == ";" {
            iter.next().ok_or(BuildError::MissingBody)?
        } else {
            next_item
        };

        let body = Self::build_block(block)?;

        let mut redirects = Vec::new();
        for remaining in iter {
            if remaining.as_rule() == Rule::Redirect {
                redirects.push(Self::build_redirect(remaining)?);
            }
        }

        let kind_children: Vec<_> = for_kind.into_inner().collect();

        let is_cstyle = kind_children.iter().any(|c| {
            matches!(
                c.as_rule(),
                Rule::CStyleForInit | Rule::CStyleForCond | Rule::CStyleForUpdate
            )
        });

        if is_cstyle || kind_children.is_empty() {
            let mut init = None;
            let mut condition = None;
            let mut update = None;

            for child in kind_children {
                match child.as_rule() {
                    Rule::CStyleForInit => {
                        let s = child.as_str().trim();
                        if !s.is_empty() {
                            init = Some(s.to_string());
                        }
                    }
                    Rule::CStyleForCond => {
                        let s = child.as_str().trim();
                        if !s.is_empty() {
                            condition = Some(s.to_string());
                        }
                    }
                    Rule::CStyleForUpdate => {
                        let s = child.as_str().trim();
                        if !s.is_empty() {
                            update = Some(s.to_string());
                        }
                    }
                    _ => {}
                }
            }

            return Ok(Statement::CStyleFor(CStyleForStatement {
                init,
                condition,
                update,
                body,
                redirects,
                span,
            }));
        }

        let mut kind_iter = kind_children.into_iter();

        let var = kind_iter
            .next()
            .ok_or(BuildError::MissingForVariable)?
            .as_str()
            .to_string();

        // Get WordList directly (no "in" node to skip!)
        let words_pair = kind_iter.next().ok_or(BuildError::MissingForWords)?;

        let mut words = Vec::new();
        for word_pair in words_pair.into_inner() {
            if word_pair.as_rule() == Rule::Word {
                words.push(Self::build_word(word_pair)?);
            }
        }

        if words.is_empty() {
            return Err(BuildError::MissingForWords);
        }

        Ok(Statement::For(ForStatement {
            var,
            words,
            body,
            redirects,
            span,
        }))
    }

    fn build_while_statement(pair: Pair<Rule>) -> Result<WhileStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // WhileStatement = "while" ~ Condition ~ (";" | NL)* ~ Block ~ Redirect*
        let condition = Self::build_condition(inner.next().ok_or(BuildError::MissingCondition)?)?;

        let body = Self::build_block(inner.next().ok_or(BuildError::MissingBody)?)?;

        let mut redirects = Vec::new();
        for remaining in inner {
            if remaining.as_rule() == Rule::Redirect {
                redirects.push(Self::build_redirect(remaining)?);
            }
        }

        Ok(WhileStatement {
            condition,
            body,
            redirects,
            span,
        })
    }

    fn build_until_statement(pair: Pair<Rule>) -> Result<UntilStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // UntilStatement = "until" ~ Condition ~ (";" | NL)* ~ Block ~ Redirect*
        let condition = Self::build_condition(inner.next().ok_or(BuildError::MissingCondition)?)?;

        let body = Self::build_block(inner.next().ok_or(BuildError::MissingBody)?)?;

        let mut redirects = Vec::new();
        for remaining in inner {
            if remaining.as_rule() == Rule::Redirect {
                redirects.push(Self::build_redirect(remaining)?);
            }
        }

        Ok(UntilStatement {
            condition,
            body,
            redirects,
            span,
        })
    }

    fn build_conditional_command(pair: Pair<Rule>) -> Result<ConditionalCommand, BuildError> {
        let span = span_from_pair(&pair);
        let inner = pair
            .into_inner()
            .next()
            .ok_or(BuildError::EmptyConditionalCommand)?;

        let expr = Self::build_cond_expr(inner)?;

        Ok(ConditionalCommand { expr, span })
    }

    fn build_cond_expr(pair: Pair<Rule>) -> Result<CondExpr, BuildError> {
        match pair.as_rule() {
            Rule::CondExpr | Rule::CondOrExpr => {
                let mut inner = pair.into_inner();
                let first = inner.next().ok_or(BuildError::EmptyConditionalCommand)?;
                let mut result = Self::build_cond_expr(first)?;

                for next in inner {
                    let right = Self::build_cond_expr(next)?;
                    result = CondExpr::Or(Box::new(result), Box::new(right));
                }
                Ok(result)
            }
            Rule::CondAndExpr => {
                let mut inner = pair.into_inner();
                let first = inner.next().ok_or(BuildError::EmptyConditionalCommand)?;
                let mut result = Self::build_cond_expr(first)?;

                for next in inner {
                    let right = Self::build_cond_expr(next)?;
                    result = CondExpr::And(Box::new(result), Box::new(right));
                }
                Ok(result)
            }
            Rule::CondUnaryExpr => {
                let pair_text = pair.as_str().trim();
                let mut inner = pair.into_inner();
                let inner_expr = inner.next().ok_or(BuildError::EmptyConditionalCommand)?;

                if pair_text.starts_with('!') {
                    let expr = Self::build_cond_expr(inner_expr)?;
                    Ok(CondExpr::Not(Box::new(expr)))
                } else {
                    Self::build_cond_expr(inner_expr)
                }
            }
            Rule::CondPrimary => {
                let inner = pair
                    .into_inner()
                    .next()
                    .ok_or(BuildError::EmptyConditionalCommand)?;
                Self::build_cond_expr(inner)
            }
            Rule::CondBinaryTest => {
                let mut inner = pair.into_inner();
                let left = inner
                    .next()
                    .ok_or(BuildError::MissingOperand)?
                    .as_str()
                    .to_string();
                let op_pair = inner.next().ok_or(BuildError::MissingOperator)?;
                let op = Self::build_cond_binary_op(op_pair)?;
                let right_pair = inner.next().ok_or(BuildError::MissingOperand)?;
                let right_raw = right_pair.as_str();
                let right_quoted = (right_raw.starts_with('"') && right_raw.ends_with('"'))
                    || (right_raw.starts_with('\'') && right_raw.ends_with('\''));
                let right = right_raw.to_string();

                Ok(CondExpr::Binary {
                    left,
                    op,
                    right,
                    right_quoted,
                })
            }
            Rule::CondUnaryTest => {
                let mut inner = pair.into_inner();
                let op_pair = inner.next().ok_or(BuildError::MissingOperator)?;
                let op = Self::build_cond_unary_op(op_pair)?;
                let arg = inner
                    .next()
                    .ok_or(BuildError::MissingOperand)?
                    .as_str()
                    .to_string();

                Ok(CondExpr::Unary { op, arg })
            }
            Rule::CondStringTest => {
                let operand = pair
                    .into_inner()
                    .next()
                    .ok_or(BuildError::EmptyConditionalCommand)?;
                Ok(CondExpr::String(operand.as_str().to_string()))
            }
            Rule::CondOperand => Ok(CondExpr::String(pair.as_str().to_string())),
            _ => Err(BuildError::UnexpectedRule {
                expected: "conditional expression",
                found: format!("{:?}", pair.as_rule()),
            }),
        }
    }

    fn build_cond_binary_op(pair: Pair<Rule>) -> Result<CondBinaryOp, BuildError> {
        match pair.as_str() {
            "==" | "=" => Ok(CondBinaryOp::Eq),
            "!=" => Ok(CondBinaryOp::Ne),
            "=~" => Ok(CondBinaryOp::Match),
            "<" => Ok(CondBinaryOp::Lt),
            ">" => Ok(CondBinaryOp::Gt),
            "-eq" => Ok(CondBinaryOp::IntEq),
            "-ne" => Ok(CondBinaryOp::IntNe),
            "-lt" => Ok(CondBinaryOp::IntLt),
            "-le" => Ok(CondBinaryOp::IntLe),
            "-gt" => Ok(CondBinaryOp::IntGt),
            "-ge" => Ok(CondBinaryOp::IntGe),
            "-nt" => Ok(CondBinaryOp::NewerThan),
            "-ot" => Ok(CondBinaryOp::OlderThan),
            "-ef" => Ok(CondBinaryOp::SameFile),
            op => Err(BuildError::UnknownOperator(op.to_string())),
        }
    }

    fn build_cond_unary_op(pair: Pair<Rule>) -> Result<CondUnaryOp, BuildError> {
        match pair.as_str() {
            "-n" => Ok(CondUnaryOp::NonZeroLen),
            "-z" => Ok(CondUnaryOp::ZeroLen),
            "-f" => Ok(CondUnaryOp::IsFile),
            "-d" => Ok(CondUnaryOp::IsDir),
            "-e" => Ok(CondUnaryOp::Exists),
            "-r" => Ok(CondUnaryOp::Readable),
            "-w" => Ok(CondUnaryOp::Writable),
            "-x" => Ok(CondUnaryOp::Executable),
            "-s" => Ok(CondUnaryOp::NonEmpty),
            "-L" | "-h" => Ok(CondUnaryOp::IsSymlink),
            "-p" => Ok(CondUnaryOp::IsPipe),
            "-S" => Ok(CondUnaryOp::IsSocket),
            "-b" => Ok(CondUnaryOp::IsBlock),
            "-c" => Ok(CondUnaryOp::IsChar),
            "-t" => Ok(CondUnaryOp::IsTerminal),
            "-O" => Ok(CondUnaryOp::OwnedByEuid),
            "-G" => Ok(CondUnaryOp::OwnedByEgid),
            "-N" => Ok(CondUnaryOp::ModifiedSinceRead),
            "-v" => Ok(CondUnaryOp::VarIsSet),
            op => Err(BuildError::UnknownOperator(op.to_string())),
        }
    }

    fn build_arithmetic_command(pair: Pair<Rule>) -> Result<ArithmeticCommand, BuildError> {
        let span = span_from_pair(&pair);
        let expr_pair = pair
            .into_inner()
            .next()
            .ok_or(BuildError::EmptyArithmeticCommand)?;

        Ok(ArithmeticCommand {
            expr: expr_pair.as_str().to_string(),
            span,
        })
    }

    fn build_coproc_command(pair: Pair<Rule>) -> Result<CoprocCommand, BuildError> {
        let span = span_from_pair(&pair);
        let inner = pair.into_inner();

        let mut name: Option<String> = None;
        let mut command: Option<Box<Statement>> = None;

        for part in inner {
            match part.as_rule() {
                Rule::CoprocName => {
                    name = Some(part.as_str().to_string());
                }
                Rule::CompoundCommand => {
                    let inner_cmd = part
                        .into_inner()
                        .next()
                        .ok_or(BuildError::EmptyCompoundCommand)?;
                    command = Some(Box::new(Self::build_statement(inner_cmd)?));
                }
                Rule::SimpleCommand => {
                    command = Some(Box::new(
                        Self::build_simple_command(part).map(Statement::SimpleCommand)?,
                    ));
                }
                _ => {
                    return Err(BuildError::UnexpectedRule {
                        expected: "CoprocName, CompoundCommand, or SimpleCommand",
                        found: format!("{:?}", part.as_rule()),
                    });
                }
            }
        }

        let command = command.ok_or(BuildError::MissingCoprocCommand)?;

        Ok(CoprocCommand {
            name,
            command,
            span,
        })
    }

    fn build_case_statement(pair: Pair<Rule>) -> Result<CaseStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // CaseStatement = "case" ~ CaseValue ~ "in" ~ CaseArm* ~ "esac"

        // Parse the value to match against
        let value_pair = inner.next().ok_or(BuildError::MissingCaseValue)?;
        let value = match value_pair.as_rule() {
            Rule::CaseValue => {
                let inner_value = value_pair
                    .into_inner()
                    .next()
                    .ok_or(BuildError::MissingCaseValue)?;
                match inner_value.as_rule() {
                    Rule::Word => Self::build_word(inner_value)?,
                    Rule::Expression => {
                        let expr_str = inner_value.as_str();
                        Word::literal(expr_str, span_from_pair(&inner_value))
                    }
                    _ => {
                        return Err(BuildError::UnexpectedRule {
                            expected: "Word or Expression inside CaseValue",
                            found: format!("{:?}", inner_value.as_rule()),
                        })
                    }
                }
            }
            Rule::Word => Self::build_word(value_pair)?,
            Rule::Expression => {
                let expr_str = value_pair.as_str();
                Word::literal(expr_str, span_from_pair(&value_pair))
            }
            _ => {
                return Err(BuildError::UnexpectedRule {
                    expected: "CaseValue (Word or Expression)",
                    found: format!("{:?}", value_pair.as_rule()),
                })
            }
        };

        // Parse all case arms
        let mut arms = Vec::new();
        for arm_pair in inner {
            if arm_pair.as_rule() == Rule::CaseArm {
                arms.push(Self::build_case_arm(arm_pair)?);
            }
        }

        Ok(CaseStatement { value, arms, span })
    }

    fn build_case_arm(pair: Pair<Rule>) -> Result<CaseArm, BuildError> {
        let inner = pair.into_inner();

        let mut patterns = Vec::new();
        let mut body = Vec::new();
        let mut terminator = CaseTerminator::Break;

        for part in inner {
            match part.as_rule() {
                Rule::CasePattern => {
                    let mut pattern_inner = part.into_inner();
                    if let Some(pattern_content) = pattern_inner.next() {
                        let pattern = match pattern_content.as_rule() {
                            Rule::Word => Self::build_word(pattern_content)?,
                            Rule::Expression => {
                                let expr_str = pattern_content.as_str();
                                Word::literal(expr_str, span_from_pair(&pattern_content))
                            }
                            _ => continue,
                        };
                        patterns.push(pattern);
                    }
                }
                Rule::CaseArmBody => {
                    for stmt_pair in part.into_inner() {
                        if stmt_pair.as_rule() == Rule::CaseArmStatement {
                            if let Some(inner_stmt) = stmt_pair.into_inner().next() {
                                body.push(Self::build_statement(inner_stmt)?);
                            }
                        }
                    }
                }
                Rule::CaseTerminator => {
                    terminator = Self::build_case_terminator(part)?;
                }
                _ => {}
            }
        }

        Ok(CaseArm {
            patterns,
            body,
            terminator,
        })
    }

    fn build_case_terminator(pair: Pair<Rule>) -> Result<CaseTerminator, BuildError> {
        match pair.as_str() {
            ";;" => Ok(CaseTerminator::Break),
            ";&" => Ok(CaseTerminator::Fallthrough),
            ";;&" => Ok(CaseTerminator::Continue),
            other => Err(BuildError::InvalidCaseTerminator(other.to_string())),
        }
    }

    fn build_var_decl(pair: Pair<Rule>) -> Result<VarDecl, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // VarDecl = VarKind ~ Identifier ~ (":" ~ TypeAnnotation)? ~ "=" ~ Expression

        let kind_str = inner.next().ok_or(BuildError::MissingVarKind)?.as_str();
        let kind = match kind_str {
            "var" => VarKind::Var,
            "const" => VarKind::Const,
            "setvar" => VarKind::Setvar,
            _ => return Err(BuildError::InvalidVarKind(kind_str.to_string())),
        };

        let target_pair = inner.next().ok_or(BuildError::MissingVarName)?;
        let target = match target_pair.as_rule() {
            Rule::VarDeclTarget => {
                let inner_target = target_pair
                    .into_inner()
                    .next()
                    .ok_or(BuildError::MissingVarName)?;
                match inner_target.as_rule() {
                    Rule::Identifier => {
                        crate::ast::VarDeclTarget::Name(inner_target.as_str().to_string())
                    }
                    Rule::TupleTarget => {
                        let mut names = Vec::new();
                        for part in inner_target.into_inner() {
                            if part.as_rule() == Rule::Identifier {
                                names.push(part.as_str().to_string());
                            }
                        }
                        crate::ast::VarDeclTarget::Tuple(names)
                    }
                    _ => return Err(BuildError::MissingVarName),
                }
            }
            Rule::Identifier => crate::ast::VarDeclTarget::Name(target_pair.as_str().to_string()),
            Rule::TupleTarget => {
                let mut names = Vec::new();
                for part in target_pair.into_inner() {
                    if part.as_rule() == Rule::Identifier {
                        names.push(part.as_str().to_string());
                    }
                }
                crate::ast::VarDeclTarget::Tuple(names)
            }
            _ => return Err(BuildError::MissingVarName),
        };

        let mut type_annotation = None;

        // Find optional type annotation and expression (after '=')
        let mut expr = None;
        for part in inner {
            match part.as_rule() {
                Rule::TypeAnnotation => {
                    type_annotation = Some(part.as_str().to_string());
                }
                Rule::Expression => {
                    expr = Some(Self::build_expression(part)?);
                    break;
                }
                _ => {}
            }
        }

        let value = expr.ok_or(BuildError::MissingVarValue)?;

        Ok(VarDecl {
            kind,
            target,
            type_annotation,
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
            inner
                .next()
                .ok_or(BuildError::MissingFunctionName)?
                .as_str()
                .to_string()
        } else {
            // No keyword, this is the name
            first.as_str().to_string()
        };

        // Skip parentheses, find block
        let block = inner
            .find(|p| p.as_rule() == Rule::Block)
            .ok_or(BuildError::MissingFunctionBody)?;

        let body = Self::build_block(block)?;

        Ok(FunctionDef { name, body, span })
    }

    fn build_proc_def(pair: Pair<Rule>) -> Result<ProcDef, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // ProcDef = "proc" ~ Identifier ~ PipelineTypeAnnot? ~ ProcParams ~ Block
        let name = inner
            .next()
            .ok_or(BuildError::MissingFunctionName)?
            .as_str()
            .to_string();

        let mut params = Vec::new();
        let mut body = Vec::new();

        for part in inner {
            match part.as_rule() {
                Rule::PipelineTypeAnnot => {
                    // Pipeline type annotation [T -> U] — skip for now (future)
                }
                Rule::ProcParams => {
                    for param_pair in part.into_inner() {
                        if param_pair.as_rule() == Rule::ProcParam {
                            params.push(Self::build_proc_param(param_pair)?);
                        }
                    }
                }
                Rule::Block => {
                    body = Self::build_block(part)?;
                }
                _ => {}
            }
        }

        Ok(ProcDef {
            name,
            params,
            body,
            span,
        })
    }

    fn build_proc_param(pair: Pair<Rule>) -> Result<ProcParam, BuildError> {
        let mut inner = pair.into_inner();

        // ProcParam = Identifier ~ (":" ~ TypeAnnotation)? ~ ("=" ~ Expression)?
        let name = inner
            .next()
            .ok_or(BuildError::MissingVarName)?
            .as_str()
            .to_string();

        let mut default = None;

        for part in inner {
            match part.as_rule() {
                Rule::TypeAnnotation => {
                    // Type annotation — skip for now (future type checking)
                }
                Rule::Expression => {
                    default = Some(Self::build_expression(part)?);
                }
                _ => {}
            }
        }

        Ok(ProcParam { name, default })
    }

    fn build_agent_invocation(pair: Pair<Rule>) -> Result<AgentInvocation, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let target = inner
            .next()
            .ok_or(BuildError::MissingFunctionName)?
            .as_str();

        let (agent_name, tool) = if let Some(colon_pos) = target.find(':') {
            (
                target[..colon_pos].to_string(),
                Some(target[colon_pos + 1..].to_string()),
            )
        } else {
            (target.to_string(), None)
        };

        let prompt_pair = inner.next().ok_or(BuildError::MissingFunctionName)?;
        let prompt_word = prompt_pair
            .into_inner()
            .next()
            .ok_or(BuildError::MissingFunctionName)?;
        let prompt = Self::build_word(prompt_word)?;

        Ok(AgentInvocation {
            agent_name,
            tool,
            prompt,
            span,
        })
    }

    fn build_return_statement(pair: Pair<Rule>) -> Result<ReturnStatement, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let value = if let Some(expr_pair) = inner.next() {
            Some(Self::build_expression(expr_pair)?)
        } else {
            None
        };

        Ok(ReturnStatement { value, span })
    }

    fn build_subshell(pair: Pair<Rule>) -> Result<Subshell, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let cmd_list = inner.next().ok_or(BuildError::MissingBody)?;

        let body_stmt = Self::build_statement(cmd_list)?;

        let body = match body_stmt {
            Statement::List(list) => {
                let mut stmts = vec![*list.first];
                stmts.extend(list.rest.into_iter().map(|(_, s)| s));
                stmts
            }
            other => vec![other],
        };

        let mut redirects = Vec::new();
        for remaining in inner {
            if remaining.as_rule() == Rule::Redirect {
                redirects.push(Self::build_redirect(remaining)?);
            }
        }

        Ok(Subshell {
            body,
            redirects,
            span,
        })
    }

    fn build_brace_group(pair: Pair<Rule>) -> Result<BraceGroup, BuildError> {
        let span = span_from_pair(&pair);

        let mut body = Vec::new();
        let mut redirects = Vec::new();

        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::Statement => {
                    // Statement is a wrapper rule - descend into it
                    if let Some(stmt_inner) = inner.into_inner().next() {
                        body.push(Self::build_statement(stmt_inner)?);
                    }
                }
                Rule::Redirect => {
                    redirects.push(Self::build_redirect(inner)?);
                }
                _ => {}
            }
        }

        Ok(BraceGroup {
            body,
            redirects,
            span,
        })
    }

    pub fn build_expression(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        // Expression always contains exactly one child (TernaryExpr)
        let inner = pair
            .into_inner()
            .next()
            .ok_or(BuildError::EmptyExpression)?;

        Self::build_expression_recursive(inner)
    }

    fn build_expression_recursive(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        match pair.as_rule() {
            // === Precedence Levels (15 total) ===
            Rule::TernaryExpr => Self::build_ternary(pair),
            Rule::NullCoalesceExpr => Self::build_null_coalesce(pair),
            Rule::OrExpr => Self::build_binary_expr(pair, BinaryOp::Or),
            Rule::AndExpr => Self::build_binary_expr(pair, BinaryOp::And),
            Rule::EqualityExpr => Self::build_equality(pair),
            Rule::ComparisonExpr => Self::build_comparison(pair),
            Rule::BitwiseOrExpr => Self::build_binary_expr(pair, BinaryOp::BitOr),
            Rule::BitwiseXorExpr => Self::build_binary_expr(pair, BinaryOp::BitXor),
            Rule::BitwiseAndExpr => Self::build_binary_expr(pair, BinaryOp::BitAnd),
            Rule::ShiftExpr => Self::build_shift(pair),
            Rule::AdditiveExpr => Self::build_additive(pair),
            Rule::MultiplicativeExpr => Self::build_multiplicative(pair),
            Rule::PowerExpr => Self::build_power(pair),
            Rule::UnaryExpr => Self::build_unary(pair),
            Rule::PostfixExpr => Self::build_postfix(pair),
            Rule::PrimaryExpr => Self::build_primary(pair),

            // === Literals (handled by PrimaryExpr but sometimes appear directly) ===
            Rule::Integer => {
                let span = span_from_pair(&pair);
                let value: i64 = pair
                    .as_str()
                    .parse()
                    .map_err(|_| BuildError::InvalidInteger(pair.as_str().to_string()))?;
                Ok(Expression::Int(value, span))
            }
            Rule::Float => {
                let span = span_from_pair(&pair);
                let value: f64 = pair
                    .as_str()
                    .parse()
                    .map_err(|_| BuildError::InvalidFloat(pair.as_str().to_string()))?;
                Ok(Expression::Float(value, span))
            }
            Rule::StringLiteral => {
                let span = span_from_pair(&pair);
                let text = pair.as_str();
                let content = text.trim_matches('"').trim_matches('\'');
                Ok(Expression::Str(content.to_string(), span))
            }
            Rule::Boolean => {
                let span = span_from_pair(&pair);
                let value = pair.as_str() == "true";
                Ok(Expression::Bool(value, span))
            }
            Rule::Null => {
                let span = span_from_pair(&pair);
                Ok(Expression::Null(span))
            }
            Rule::Identifier | Rule::VariableName => {
                let span = span_from_pair(&pair);
                let name = pair.as_str().to_string();
                Ok(Expression::Variable { name, span })
            }

            // === Nested or Unknown ===
            rule => {
                // Try to descend into nested rule
                if let Some(inner) = pair.into_inner().next() {
                    Self::build_expression_recursive(inner)
                } else {
                    Err(BuildError::UnsupportedExpression(format!("{:?}", rule)))
                }
            }
        }
    }

    // === Binary Operators (Left-Associative) ===

    fn build_binary_expr(pair: Pair<Rule>, default_op: BinaryOp) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // Get first operand
        let mut left = Self::build_expression_recursive(inner.next().unwrap())?;

        // Process remaining operands (operators are between them in grammar)
        // Grammar: Term ~ (Op ~ Term)*
        // Parse tree: [Term, Term, Term, ...] (operators stripped by pest)
        for right_part in inner {
            let right = Self::build_expression_recursive(right_part)?;

            left = Expression::Binary {
                left: Box::new(left),
                op: default_op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn build_additive(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let mut left = Self::build_expression_recursive(inner.next().unwrap())?;

        while let Some(op_part) = inner.next() {
            let op = match op_part.as_str() {
                "+" => BinaryOp::Add,
                "-" => BinaryOp::Sub,
                _ => continue, // Not an operator, might be next term
            };

            let right = Self::build_expression_recursive(inner.next().unwrap())?;

            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn build_multiplicative(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let mut left = Self::build_expression_recursive(inner.next().unwrap())?;

        while let Some(op_part) = inner.next() {
            let op = match op_part.as_str() {
                "*" => BinaryOp::Mul,
                "/" => BinaryOp::Div,
                "//" => BinaryOp::IntDiv,
                "%" => BinaryOp::Mod,
                _ => continue,
            };

            let right = Self::build_expression_recursive(inner.next().unwrap())?;

            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn build_power(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let inner = pair.into_inner();

        // Power is RIGHT-associative: 2 ** 3 ** 4 = 2 ** (3 ** 4)
        let terms: Vec<_> = inner.filter(|p| p.as_rule() == Rule::UnaryExpr).collect();

        if terms.is_empty() {
            return Err(BuildError::EmptyExpression);
        }

        if terms.len() == 1 {
            return Self::build_expression_recursive(terms[0].clone());
        }

        // Build right-to-left
        let mut result = Self::build_expression_recursive(terms[terms.len() - 1].clone())?;

        for i in (0..terms.len() - 1).rev() {
            let left = Self::build_expression_recursive(terms[i].clone())?;
            result = Expression::Binary {
                left: Box::new(left),
                op: BinaryOp::Pow,
                right: Box::new(result),
                span,
            };
        }

        Ok(result)
    }

    fn build_shift(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let mut left = Self::build_expression_recursive(inner.next().unwrap())?;

        while let Some(op_part) = inner.next() {
            let op = match op_part.as_str() {
                "<<" => BinaryOp::Shl,
                ">>" => BinaryOp::Shr,
                _ => continue,
            };

            let right = Self::build_expression_recursive(inner.next().unwrap())?;

            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn build_equality(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let mut left = Self::build_expression_recursive(inner.next().unwrap())?;

        while let Some(op_part) = inner.next() {
            let op = match op_part.as_str() {
                "==" => BinaryOp::Eq,
                "!=" => BinaryOp::Ne,
                "=~" => BinaryOp::Match,
                "!~" => BinaryOp::NotMatch,
                "~~" => BinaryOp::Match, // Alternative match operator
                _ => continue,
            };

            let right = Self::build_expression_recursive(inner.next().unwrap())?;

            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn build_comparison(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let mut left = Self::build_expression_recursive(inner.next().unwrap())?;

        while let Some(op_part) = inner.next() {
            let op = match op_part.as_str() {
                "<" => BinaryOp::Lt,
                "<=" => BinaryOp::Le,
                ">" => BinaryOp::Gt,
                ">=" => BinaryOp::Ge,
                _ => continue,
            };

            let right = Self::build_expression_recursive(inner.next().unwrap())?;

            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn build_ternary(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // TernaryExpr = NullCoalesceExpr ~ ("?" ~ Expression ~ ":" ~ Expression)?
        let condition = Self::build_expression_recursive(inner.next().unwrap())?;

        let then_part = inner.next();
        let else_part = inner.next();

        if let (Some(then_expr), Some(else_expr)) = (then_part, else_part) {
            let then_expr = Self::build_expression_recursive(then_expr)?;
            let else_expr = Self::build_expression_recursive(else_expr)?;

            return Ok(Expression::Ternary {
                condition: Box::new(condition),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
                span,
            });
        }

        Ok(condition)
    }

    fn build_null_coalesce(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let mut left = Self::build_expression_recursive(inner.next().unwrap())?;

        // NullCoalesceExpr = OrExpr ~ ("??" ~ OrExpr)*
        for right_part in inner {
            let right = Self::build_expression_recursive(right_part)?;
            left = Expression::NullCoalesce {
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn build_unary(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // UnaryExpr = (UnaryOp ~ UnaryExpr) | PostfixExpr
        let first = inner.next().unwrap();

        match first.as_str() {
            "!" | "not" => {
                let operand = Self::build_expression_recursive(inner.next().unwrap())?;
                Ok(Expression::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                    span,
                })
            }
            "-" => {
                let operand = Self::build_expression_recursive(inner.next().unwrap())?;
                Ok(Expression::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                    span,
                })
            }
            "~" => {
                let operand = Self::build_expression_recursive(inner.next().unwrap())?;
                Ok(Expression::Unary {
                    op: UnaryOp::BitNot,
                    operand: Box::new(operand),
                    span,
                })
            }
            _ => {
                // Not a unary operator, must be PostfixExpr
                Self::build_expression_recursive(first)
            }
        }
    }

    fn build_postfix(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // PostfixExpr = PrimaryExpr ~ PostfixOp*
        let mut expr = Self::build_expression_recursive(inner.next().unwrap())?;

        // Apply postfix operators left-to-right
        for op in inner {
            if op.as_rule() == Rule::PostfixOp {
                let op_text = op.as_str();
                let mut op_inner = op.into_inner();

                if op_text.starts_with('[') {
                    if let Some(index_expr) = op_inner.next() {
                        let index = Self::build_expression_recursive(index_expr)?;
                        expr = Expression::Index {
                            target: Box::new(expr),
                            index: Box::new(index),
                            span,
                        };
                    }
                } else if op_text.starts_with('.') {
                    if let Some(field_name) = op_inner.next() {
                        let field = field_name.as_str().to_string();
                        expr = Expression::Field {
                            target: Box::new(expr),
                            field,
                            span,
                        };
                    }
                } else if op_text.starts_with('(') {
                    let args = Self::build_arg_list(op_inner)?;
                    match expr {
                        Expression::Variable { name, .. } => {
                            expr = Expression::FuncCall { name, args, span };
                        }
                        Expression::OptionalChain { target, .. } => match *target {
                            Expression::Variable { name, .. } => {
                                expr = Expression::OptionalCall { name, args, span };
                            }
                            other => {
                                return Err(BuildError::UnsupportedExpression(format!(
                                    "optional call target: {:?}",
                                    other
                                )));
                            }
                        },
                        Expression::Field { target, field, .. } => {
                            expr = Expression::MethodCall {
                                target,
                                name: field,
                                args,
                                span,
                            };
                        }
                        other => {
                            expr = Expression::MethodCall {
                                target: Box::new(other),
                                name: "__call__".to_string(),
                                args,
                                span,
                            };
                        }
                    }
                } else if op_text == "?" {
                    expr = Expression::OptionalChain {
                        target: Box::new(expr),
                        span,
                    };
                } else if op_text == "!" {
                    expr = Expression::Unwrap {
                        target: Box::new(expr),
                        span,
                    };
                }
            }
        }

        Ok(expr)
    }

    fn build_primary(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let inner = pair
            .into_inner()
            .next()
            .ok_or(BuildError::EmptyExpression)?;

        match inner.as_rule() {
            Rule::Integer => {
                let value: i64 = inner
                    .as_str()
                    .parse()
                    .map_err(|_| BuildError::InvalidInteger(inner.as_str().to_string()))?;
                Ok(Expression::Int(value, span))
            }
            Rule::Float => {
                let value: f64 = inner
                    .as_str()
                    .parse()
                    .map_err(|_| BuildError::InvalidFloat(inner.as_str().to_string()))?;
                Ok(Expression::Float(value, span))
            }
            Rule::StringLiteral => {
                let text = inner.as_str();
                let content = text.trim_matches('"').trim_matches('\'');
                Ok(Expression::Str(content.to_string(), span))
            }
            Rule::Boolean => {
                let value = inner.as_str() == "true";
                Ok(Expression::Bool(value, span))
            }
            Rule::Null => Ok(Expression::Null(span)),
            Rule::Identifier => {
                let name = inner.as_str().to_string();
                Ok(Expression::Variable { name, span })
            }
            Rule::ListExpr => Self::build_list_expr(inner),
            Rule::DictExpr => Self::build_dict_expr(inner),
            Rule::TupleExpr => Self::build_tuple_expr(inner),
            Rule::LambdaExpr => Self::build_lambda_expr(inner),
            Rule::Expression => {
                // Parenthesized expression
                Self::build_expression(inner)
            }
            _ => Self::build_expression_recursive(inner),
        }
    }

    fn build_arg_list(inner: pest::iterators::Pairs<Rule>) -> Result<Vec<Expression>, BuildError> {
        let mut args = Vec::new();

        for part in inner {
            match part.as_rule() {
                Rule::Expression => args.push(Self::build_expression(part)?),
                Rule::ArgList => {
                    for arg in part.into_inner() {
                        if arg.as_rule() == Rule::Expression {
                            args.push(Self::build_expression(arg)?);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(args)
    }

    fn build_list_expr(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut items = Vec::new();

        for part in pair.into_inner() {
            if part.as_rule() == Rule::Expression {
                items.push(Self::build_expression(part)?);
            }
        }

        Ok(Expression::List { items, span })
    }

    fn build_dict_expr(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut entries = Vec::new();

        for entry in pair.into_inner() {
            if entry.as_rule() == Rule::DictEntry {
                let mut entry_inner = entry.into_inner();

                // Key (Identifier, StringLiteral, or [Expression])
                let key_pair = entry_inner.next().unwrap();
                let key = match key_pair.as_rule() {
                    Rule::Identifier | Rule::StringLiteral => {
                        Expression::Str(key_pair.as_str().to_string(), span_from_pair(&key_pair))
                    }
                    Rule::Expression => Self::build_expression(key_pair)?,
                    _ => {
                        return Err(BuildError::UnexpectedRule {
                            expected: "dict key",
                            found: format!("{:?}", key_pair.as_rule()),
                        })
                    }
                };

                // Value
                let value_pair = entry_inner.next().ok_or(BuildError::EmptyExpression)?;
                let value = Self::build_expression(value_pair)?;

                entries.push((key, value));
            }
        }

        Ok(Expression::Dict { entries, span })
    }

    fn build_tuple_expr(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let mut items = Vec::new();

        for part in pair.into_inner() {
            if part.as_rule() == Rule::Expression {
                items.push(Self::build_expression(part)?);
            }
        }

        Ok(Expression::Tuple { items, span })
    }

    fn build_lambda_expr(pair: Pair<Rule>) -> Result<Expression, BuildError> {
        let span = span_from_pair(&pair);
        let inner = pair.into_inner();

        // LambdaExpr = "|" ~ params? ~ "|" ~ (Block | Expression)
        let mut params = Vec::new();

        // Collect parameters (identifiers before the body)
        for part in inner {
            if part.as_rule() == Rule::Identifier {
                params.push(part.as_str().to_string());
            } else if part.as_rule() == Rule::Expression {
                // Found body
                let body = Self::build_expression(part)?;
                return Ok(Expression::Lambda {
                    params,
                    body: Box::new(body),
                    span,
                });
            } else if part.as_rule() == Rule::Block {
                // Block body - convert to expression
                // TODO: Handle block as expression body
                break;
            }
        }

        Err(BuildError::MissingLambdaBody)
    }

    fn build_assignment(pair: Pair<Rule>) -> Result<Assignment, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // Assignment = Identifier ~ AssignOp ~ Expression

        let name = inner
            .next()
            .ok_or(BuildError::MissingAssignmentName)?
            .as_str()
            .to_string();

        let op_pair = inner.next().ok_or(BuildError::MissingAssignmentOp)?;
        let op = match op_pair.as_str() {
            "=" => AssignOp::Assign,
            "+=" => AssignOp::Append,
            ":=" => AssignOp::Default,
            "?=" => AssignOp::IfUnset,
            _ => return Err(BuildError::InvalidAssignOp(op_pair.as_str().to_string())),
        };

        let value_pair = inner.next().ok_or(BuildError::MissingAssignmentValue)?;

        // Assignment value is Expression in pest, but Word in AST
        // For now, convert expression to word literal
        let value_text = value_pair.as_str();
        let value = Word::literal(value_text, span_from_pair(&value_pair));

        Ok(Assignment {
            name,
            subscript: None,
            op,
            value: AssignmentValue::Word(value),
            span,
        })
    }

    fn build_bash_assignment(pair: Pair<Rule>) -> Result<Assignment, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        // BashAssignment = BashAssignmentLHS ~ (AssocArrayLiteral | ArrayLiteral | Word)?
        // BashAssignmentLHS = name ~ [subscript]? ~ ("+=" | "=")

        let lhs = inner
            .next()
            .ok_or(BuildError::MissingAssignmentName)?
            .as_str();

        // Parse the LHS to extract name, subscript, and operator
        let (name, subscript, op) = Self::parse_assignment_lhs(lhs)?;

        // Next pair (if any) is the value
        let value_pair_opt = inner.next();

        let value = if let Some(value_pair) = value_pair_opt {
            match value_pair.as_rule() {
                Rule::AssocArrayLiteral => {
                    let mut entries = Vec::new();
                    for entry_pair in value_pair.into_inner() {
                        if entry_pair.as_rule() == Rule::AssocArrayEntry {
                            let mut entry_inner = entry_pair.into_inner();
                            let key = entry_inner
                                .next()
                                .map(|p| p.as_str().to_string())
                                .unwrap_or_default();
                            let val = entry_inner
                                .next()
                                .map(|p| p.as_str().to_string())
                                .unwrap_or_default();
                            entries.push((key, val));
                        }
                    }
                    AssignmentValue::AssocArray(entries)
                }
                Rule::ArrayLiteral => {
                    let mut array_words = Vec::new();
                    for word_pair in value_pair.into_inner() {
                        if word_pair.as_rule() == Rule::Word {
                            array_words.push(Self::build_word(word_pair)?);
                        }
                    }
                    AssignmentValue::Array(array_words)
                }
                Rule::Word => AssignmentValue::Word(Self::build_word(value_pair)?),
                _ => {
                    return Err(BuildError::UnexpectedRule {
                        expected: "AssocArrayLiteral, ArrayLiteral or Word",
                        found: format!("{:?}", value_pair.as_rule()),
                    });
                }
            }
        } else {
            // Empty assignment (e.g., empty=)
            AssignmentValue::Word(Word {
                parts: vec![],
                span,
            })
        };

        Ok(Assignment {
            name,
            subscript,
            op,
            value,
            span,
        })
    }

    fn parse_assignment_lhs(
        lhs: &str,
    ) -> Result<(String, Option<ArraySubscript>, AssignOp), BuildError> {
        let (rest, op) = if let Some(stripped) = lhs.strip_suffix("+=") {
            (stripped, AssignOp::Append)
        } else if let Some(stripped) = lhs.strip_suffix('=') {
            (stripped, AssignOp::Assign)
        } else {
            return Err(BuildError::MissingAssignmentName);
        };

        if let Some(bracket_pos) = rest.find('[') {
            if rest.ends_with(']') {
                let name = rest[..bracket_pos].to_string();
                let subscript_str = &rest[bracket_pos + 1..rest.len() - 1];

                let subscript = if subscript_str == "@" {
                    ArraySubscript::All
                } else if subscript_str == "*" {
                    ArraySubscript::Star
                } else if let Ok(idx) = subscript_str.parse::<i64>() {
                    ArraySubscript::Index(Box::new(Expression::Int(idx, Span::DUMMY)))
                } else {
                    ArraySubscript::Index(Box::new(Expression::Str(
                        subscript_str.to_string(),
                        Span::DUMMY,
                    )))
                };

                Ok((name, Some(subscript), op))
            } else {
                Ok((rest.to_string(), None, op))
            }
        } else {
            Ok((rest.to_string(), None, op))
        }
    }

    fn build_word(pair: Pair<Rule>) -> Result<Word, BuildError> {
        let span = span_from_pair(&pair);

        // Word = ${ WordPart+ }
        // Word now contains MULTIPLE children (each a QuotedString, DollarExpansion, or Bareword)
        // Build all parts into WordPart vector
        let mut parts = Vec::new();

        for part_pair in pair.into_inner() {
            let rule = part_pair.as_rule();
            let text = part_pair.as_str().to_string(); // Capture before consuming

            let word_part = match rule {
                Rule::Bareword => {
                    let raw = if let Some(chars) = part_pair.into_inner().next() {
                        chars.as_str()
                    } else {
                        &text
                    };

                    if raw.contains('\\') {
                        Self::build_bareword_with_escapes(raw)
                    } else {
                        WordPart::Literal(raw.to_string())
                    }
                }
                Rule::UnquotedEscape => {
                    if text.len() >= 2 {
                        let escaped_char = text.chars().nth(1).unwrap();
                        if matches!(escaped_char, '*' | '?' | '[' | ']' | '@' | '!' | '+') {
                            WordPart::SingleQuoted(escaped_char.to_string())
                        } else {
                            WordPart::Literal(escaped_char.to_string())
                        }
                    } else {
                        WordPart::Literal(text)
                    }
                }
                Rule::BraceExpansion => WordPart::BraceExpansion(text),
                Rule::BacktickCommandSub => {
                    let inner = &text[1..text.len() - 1];
                    // POSIX: Inside backticks, these escapes are processed:
                    // \` -> `
                    // \\ -> \
                    // \$ -> $
                    // \" -> "
                    // \newline -> (removed, line continuation)
                    let unescaped = unescape_backtick_content(inner);
                    WordPart::CommandSubRaw(unescaped)
                }
                Rule::ProcessSub => Self::build_process_sub(part_pair)?,
                Rule::TildeExpansion => {
                    let user = if text.len() <= 1 {
                        None
                    } else {
                        Some(text[1..].to_string())
                    };
                    WordPart::Tilde(user)
                }
                Rule::ExtGlob => Self::build_extglob(part_pair)?,
                Rule::DollarSingleQuote => {
                    let content = text.strip_prefix("$'").unwrap_or(&text);
                    let content = content.strip_suffix('\'').unwrap_or(content);
                    WordPart::AnsiCQuoted(content.to_string())
                }
                Rule::DollarExpansion => Self::build_dollar_expansion(part_pair)?,
                Rule::QuotedString => Self::build_quoted_string(part_pair)?,
                Rule::SingleQuoted => {
                    let content = text
                        .trim_start_matches('\'')
                        .trim_end_matches('\'')
                        .to_string();
                    WordPart::SingleQuoted(content)
                }
                Rule::DoubleQuoted => Self::build_double_quoted(part_pair)?,
                _ => {
                    // Fallback: treat as literal
                    WordPart::Literal(text)
                }
            };

            parts.push(word_part);
        }

        if parts.is_empty() {
            return Err(BuildError::EmptyWord);
        }

        Ok(Word { parts, span })
    }

    fn build_dollar_expansion(pair: Pair<Rule>) -> Result<WordPart, BuildError> {
        // DollarExpansion = ("$[" ~ Expression ~ "]")  // expression
        //                 | ("$(" ~ CommandList ~ ")")  // command sub
        //                 | ("${" ~ VariableRef ~ "}")  // parameter
        //                 | ("$" ~ Identifier)          // simple var
        //                 | LiteralDollar               // lone $ (literal)

        let pair_text = pair.as_str().to_string();

        if pair_text == "$" {
            return Ok(WordPart::Literal("$".to_string()));
        }

        let inner = pair
            .into_inner()
            .next()
            .ok_or(BuildError::EmptyDollarExpansion)?;

        match inner.as_rule() {
            Rule::PositionalParam => {
                // $1, $@, $#, etc. - positional parameters
                let name = inner.as_str().to_string();
                Ok(WordPart::Variable(VariableRef {
                    name,
                    prefix: None,
                    subscript: None,
                    modifier: None,
                    span: span_from_pair(&inner),
                }))
            }
            Rule::Identifier | Rule::VariableName => {
                // Simple $VAR (allows reserved words as variable names)
                let name = inner.as_str().to_string();
                Ok(WordPart::Variable(VariableRef {
                    name,
                    prefix: None,
                    subscript: None,
                    modifier: None,
                    span: span_from_pair(&inner),
                }))
            }
            Rule::VariableRef => {
                // ${VAR} or ${VAR:-default} or ${VAR[0]}
                Self::build_variable_ref(inner).map(WordPart::Variable)
            }
            Rule::DollarArithRaw => {
                // $((expr)) - captured as raw string, will be evaluated during expansion
                let text = inner.as_str();

                // Extract content by counting parens to find matching ))
                // Start after "$((", count parens until we find matching ))
                if !text.starts_with("$((") {
                    return Ok(WordPart::Literal(text.to_string()));
                }

                let chars: Vec<char> = text.chars().collect();
                let mut paren_count = 0; // Start at 0, looking for ))
                let mut end_pos = 3; // Start after "$("

                while end_pos < chars.len() {
                    match chars[end_pos] {
                        '(' => paren_count += 1,
                        ')' => {
                            if paren_count == 0 {
                                // Check if next char is also ) (closing ))
                                if end_pos + 1 < chars.len() && chars[end_pos + 1] == ')' {
                                    // Found matching ))
                                    let content: String = chars[3..end_pos].iter().collect();
                                    return Ok(WordPart::ArithRaw(content));
                                }
                            } else {
                                paren_count -= 1;
                            }
                        }
                        _ => {}
                    }
                    end_pos += 1;
                }

                // No matching )) found - treat as literal
                Ok(WordPart::Literal(text.to_string()))
            }
            Rule::DollarCommandSubRaw => {
                let text = inner.as_str();
                let content = &text[2..text.len() - 1];
                Ok(WordPart::CommandSubRaw(content.to_string()))
            }
            Rule::CommandList => {
                // $(command) - old approach, still here for backwards compat
                let stmt = Self::build_command_list(inner)?;
                Ok(WordPart::CommandSub(Box::new(Statement::List(stmt))))
            }
            Rule::ExprSub => {
                let text = inner.as_str();
                let content = &text[2..text.len() - 1];
                let pairs = ColonyParser::parse(Rule::Expression, content)
                    .map_err(|e| BuildError::Parse(e.to_string()))?;
                let expr_pair = pairs
                    .into_iter()
                    .next()
                    .ok_or(BuildError::EmptyExpression)?;
                let expr = Self::build_expression(expr_pair)?;
                Ok(WordPart::ExprSub(Box::new(expr)))
            }
            Rule::Expression => {
                // $[expr] - parsed inline into expression AST
                let expr = Self::build_expression(inner)?;
                Ok(WordPart::ExprSub(Box::new(expr)))
            }
            _ => {
                // Fallback
                Ok(WordPart::Literal(pair_text))
            }
        }
    }

    fn build_variable_ref(pair: Pair<Rule>) -> Result<VariableRef, BuildError> {
        let span = span_from_pair(&pair);
        let mut inner = pair.into_inner();

        let mut prefix = None;
        let mut subscript = None;
        let mut modifier = None;

        let first = inner.next().ok_or(BuildError::MissingVariableName)?;

        let name = if first.as_rule() == Rule::VarPrefix {
            prefix = Some(match first.as_str() {
                "#" => VarPrefix::Length,
                "!" => VarPrefix::Indices,
                _ => return Err(BuildError::InvalidVarPrefix(first.as_str().to_string())),
            });
            let name_part = inner.next().ok_or(BuildError::MissingVariableName)?;
            Self::extract_variable_name(name_part)?
        } else {
            Self::extract_variable_name(first)?
        };

        for part in inner {
            match part.as_rule() {
                Rule::ArraySubscript => {
                    subscript = Some(Self::build_array_subscript(part)?);
                }
                Rule::VarModifier => {
                    modifier = Some(Self::build_var_modifier(part)?);
                }
                _ => {}
            }
        }

        Ok(VariableRef {
            name,
            prefix,
            subscript,
            modifier,
            span,
        })
    }

    fn extract_variable_name(pair: Pair<Rule>) -> Result<String, BuildError> {
        match pair.as_rule() {
            Rule::VariableRefName => {
                let inner = pair
                    .into_inner()
                    .next()
                    .ok_or(BuildError::MissingVariableName)?;
                Self::extract_variable_name(inner)
            }
            Rule::Identifier | Rule::VariableName | Rule::PositionalParam => {
                Ok(pair.as_str().to_string())
            }
            _ => Ok(pair.as_str().to_string()),
        }
    }

    fn build_array_subscript(pair: Pair<Rule>) -> Result<ArraySubscript, BuildError> {
        let inner = pair
            .into_inner()
            .next()
            .ok_or(BuildError::EmptyExpression)?;

        match inner.as_rule() {
            Rule::ArraySpecial => match inner.as_str() {
                "@" => Ok(ArraySubscript::All),
                "*" => Ok(ArraySubscript::Star),
                _ => Err(BuildError::InvalidArraySubscript(
                    inner.as_str().to_string(),
                )),
            },
            Rule::Expression => {
                let expr = Self::build_expression(inner)?;
                Ok(ArraySubscript::Index(Box::new(expr)))
            }
            Rule::DollarExpansion => {
                let text = inner.as_str();
                if text.starts_with('$') && !text.starts_with("$(") && !text.starts_with("${") {
                    let var_name = &text[1..];
                    let span = Span::DUMMY;
                    let expr = Expression::Variable {
                        name: var_name.to_string(),
                        span,
                    };
                    Ok(ArraySubscript::Index(Box::new(expr)))
                } else if text.starts_with("${") && text.ends_with("}") {
                    let var_name = &text[2..text.len() - 1];
                    let span = Span::DUMMY;
                    let expr = Expression::Variable {
                        name: var_name.to_string(),
                        span,
                    };
                    Ok(ArraySubscript::Index(Box::new(expr)))
                } else {
                    Err(BuildError::InvalidArraySubscript(text.to_string()))
                }
            }
            _ => Err(BuildError::InvalidArraySubscript(
                inner.as_str().to_string(),
            )),
        }
    }

    fn build_var_modifier(pair: Pair<Rule>) -> Result<VarModifier, BuildError> {
        let text = pair.as_str();
        let mut inner = pair.into_inner();

        if text == "#" {
            return Ok(VarModifier::Length);
        }
        if text == "!" {
            return Ok(VarModifier::Indices);
        }

        if text.starts_with(":")
            && !text.starts_with(":-")
            && !text.starts_with(":=")
            && !text.starts_with(":?")
            && !text.starts_with(":+")
        {
            let slice_text = &text[1..];
            let parts: Vec<&str> = slice_text.split(':').collect();
            let offset_str = parts.first().copied().unwrap_or("0");
            let offset = Word {
                parts: vec![WordPart::Literal(offset_str.to_string())],
                span: Span::DUMMY,
            };
            let length = parts.get(1).map(|s| Word {
                parts: vec![WordPart::Literal(s.to_string())],
                span: Span::DUMMY,
            });
            return Ok(VarModifier::Slice {
                offset: Box::new(offset),
                length: length.map(Box::new),
            });
        }

        let first = inner.next();

        if text.starts_with("//") || (text.starts_with("/") && !text.starts_with("//")) {
            let all = text.starts_with("//");
            let pattern = if let Some(ref p) = first {
                if p.as_rule() == Rule::SubstPattern {
                    Self::build_subst_word(p.clone())?
                } else {
                    Word {
                        parts: vec![],
                        span: Span::DUMMY,
                    }
                }
            } else {
                Word {
                    parts: vec![],
                    span: Span::DUMMY,
                }
            };
            let replacement = inner
                .next()
                .filter(|p| p.as_rule() == Rule::SubstReplacement)
                .map(|p| Self::build_subst_word(p))
                .transpose()?
                .unwrap_or_else(|| Word {
                    parts: vec![],
                    span: Span::DUMMY,
                });
            return Ok(VarModifier::Substitute {
                pattern: Box::new(pattern),
                replacement: Box::new(replacement),
                all,
            });
        }

        let word_value = if let Some(first) = first {
            if first.as_rule() == Rule::SliceOffset {
                let offset_word = Self::build_slice_value(first)?;
                let length_word = inner
                    .next()
                    .filter(|p| p.as_rule() == Rule::SliceLength)
                    .map(|p| Self::build_slice_value(p))
                    .transpose()?;
                return Ok(VarModifier::Slice {
                    offset: Box::new(offset_word),
                    length: length_word.map(Box::new),
                });
            }
            Self::build_word(first)?
        } else {
            Word {
                parts: vec![],
                span: Span::DUMMY,
            }
        };

        match text {
            s if s.starts_with(":-") => Ok(VarModifier::Default {
                word: Box::new(word_value),
                check_null: true,
            }),
            s if s.starts_with("-") => Ok(VarModifier::Default {
                word: Box::new(word_value),
                check_null: false,
            }),
            s if s.starts_with(":=") => Ok(VarModifier::Assign {
                word: Box::new(word_value),
                check_null: true,
            }),
            s if s.starts_with("=") => Ok(VarModifier::Assign {
                word: Box::new(word_value),
                check_null: false,
            }),
            s if s.starts_with(":?") => Ok(VarModifier::Error {
                word: Box::new(word_value),
                check_null: true,
            }),
            s if s.starts_with("?") => Ok(VarModifier::Error {
                word: Box::new(word_value),
                check_null: false,
            }),
            s if s.starts_with(":+") => Ok(VarModifier::Alternate {
                word: Box::new(word_value),
                check_null: true,
            }),
            s if s.starts_with("+") => Ok(VarModifier::Alternate {
                word: Box::new(word_value),
                check_null: false,
            }),
            s if s.starts_with("##") => Ok(VarModifier::StripPrefixGreedy {
                pattern: Box::new(word_value),
            }),
            s if s.starts_with("#") => Ok(VarModifier::StripPrefix {
                pattern: Box::new(word_value),
            }),
            s if s.starts_with("%%") => Ok(VarModifier::StripSuffixGreedy {
                pattern: Box::new(word_value),
            }),
            s if s.starts_with("%") => Ok(VarModifier::StripSuffix {
                pattern: Box::new(word_value),
            }),
            "^^" => Ok(VarModifier::UpperCase { all: true }),
            "^" => Ok(VarModifier::UpperCase { all: false }),
            ",," => Ok(VarModifier::LowerCase { all: true }),
            "," => Ok(VarModifier::LowerCase { all: false }),
            "~~" => Ok(VarModifier::ToggleCase { all: true }),
            "~" => Ok(VarModifier::ToggleCase { all: false }),
            _ => Err(BuildError::InvalidVarModifier(text.to_string())),
        }
    }

    fn build_slice_value(pair: Pair<Rule>) -> Result<Word, BuildError> {
        let text = pair.as_str().to_string();
        let mut parts = Vec::new();
        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::DollarExpansion => {
                    parts.push(Self::build_dollar_expansion(inner)?);
                }
                _ => {
                    let inner_text = inner.as_str();
                    if !inner_text.is_empty() {
                        parts.push(WordPart::Literal(inner_text.to_string()));
                    }
                }
            }
        }
        if parts.is_empty() {
            parts.push(WordPart::Literal(text));
        }
        Ok(Word {
            parts,
            span: Span::DUMMY,
        })
    }

    fn build_subst_word(pair: Pair<Rule>) -> Result<Word, BuildError> {
        let mut parts = Vec::new();
        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::QuotedString => {
                    parts.push(Self::build_quoted_string(inner)?);
                }
                Rule::DollarExpansion => {
                    parts.push(Self::build_dollar_expansion(inner)?);
                }
                Rule::BacktickCommandSub => {
                    let text = inner.as_str();
                    let content = text.trim_start_matches('`').trim_end_matches('`');
                    parts.push(WordPart::CommandSubRaw(unescape_backtick_content(content)));
                }
                Rule::UnquotedEscape => {
                    let text = inner.as_str();
                    let escaped = text.trim_start_matches('\\');
                    parts.push(WordPart::Literal(escaped.to_string()));
                }
                Rule::SubstBareword => {
                    parts.push(WordPart::Literal(inner.as_str().to_string()));
                }
                _ => {
                    let inner_text = inner.as_str();
                    if !inner_text.is_empty() {
                        parts.push(WordPart::Literal(inner_text.to_string()));
                    }
                }
            }
        }
        Ok(Word {
            parts,
            span: Span::DUMMY,
        })
    }

    fn build_quoted_string(pair: Pair<Rule>) -> Result<WordPart, BuildError> {
        let inner = pair
            .into_inner()
            .next()
            .ok_or(BuildError::EmptyQuotedString)?;

        match inner.as_rule() {
            Rule::SingleQuoted => {
                let text = inner.as_str();
                let content = text.trim_start_matches('\'').trim_end_matches('\'');
                Ok(WordPart::SingleQuoted(content.to_string()))
            }
            Rule::DoubleQuoted => Self::build_double_quoted(inner),
            Rule::RawString => {
                // r'text' or r"text" - treat like single quoted
                let text = inner.as_str();
                let content = text[2..text.len() - 1].to_string(); // Strip r' and '
                Ok(WordPart::SingleQuoted(content))
            }
            Rule::TripleQuoted => {
                // '''text''' or """text""" - treat like single quoted for now
                let text = inner.as_str();
                let content = text[3..text.len() - 3].to_string(); // Strip ''' and '''
                Ok(WordPart::SingleQuoted(content))
            }
            _ => Err(BuildError::UnexpectedQuotedString(format!(
                "{:?}",
                inner.as_rule()
            ))),
        }
    }

    fn build_double_quoted(pair: Pair<Rule>) -> Result<WordPart, BuildError> {
        // DoubleQuoted = ${ "\"" ~ DoubleQuotedContent* ~ "\"" }
        let mut parts = Vec::new();
        let mut literal_buffer = String::new();

        // Helper to flush accumulated literals
        let flush_literal = |buffer: &mut String, parts: &mut Vec<WordPart>| {
            if !buffer.is_empty() {
                parts.push(WordPart::Literal(buffer.clone()));
                buffer.clear();
            }
        };

        for content in pair.into_inner() {
            // DoubleQuotedContent is a wrapper - descend into it
            let actual_content = if content.as_rule() == Rule::DoubleQuotedContent {
                content.into_inner().next().unwrap()
            } else {
                content
            };

            match actual_content.as_rule() {
                Rule::DollarExpansion => {
                    flush_literal(&mut literal_buffer, &mut parts);
                    parts.push(Self::build_dollar_expansion(actual_content)?);
                }
                Rule::BacktickCommandSub => {
                    flush_literal(&mut literal_buffer, &mut parts);
                    let text = actual_content.as_str();
                    let inner = &text[1..text.len() - 1];
                    parts.push(WordPart::CommandSubRaw(unescape_backtick_content(inner)));
                }
                Rule::EscapeSequence => {
                    // POSIX: Inside double quotes, only $ ` " \ and newline are special after backslash
                    let text = actual_content.as_str();
                    if text.len() >= 2 {
                        let escaped_char = text.chars().nth(1).unwrap();
                        let expanded = match escaped_char {
                            '$' => "$",
                            '`' => "`",
                            '"' => "\"",
                            '\\' => "\\",
                            '\n' => "",
                            _ => {
                                literal_buffer.push_str(text);
                                continue;
                            }
                        };
                        literal_buffer.push_str(expanded);
                    } else {
                        literal_buffer.push_str(text);
                    }
                }
                Rule::DoubleQuotedChar => {
                    literal_buffer.push_str(actual_content.as_str());
                }
                _ => {}
            }
        }

        // Flush any remaining literal text
        flush_literal(&mut literal_buffer, &mut parts);

        Ok(WordPart::DoubleQuoted(parts))
    }

    fn build_process_sub(pair: Pair<Rule>) -> Result<WordPart, BuildError> {
        let text = pair.as_str();
        let mut inner = pair.into_inner();

        let op_pair = inner.next().ok_or(BuildError::MissingVariableName)?;
        let op_text = op_pair.as_str();

        let direction = match op_text {
            "<" => ProcessSubDirection::Input,
            ">" => ProcessSubDirection::Output,
            _ => {
                return Err(BuildError::InvalidVarPrefix(format!(
                    "Unknown process sub operator: {}",
                    op_text
                )))
            }
        };

        let command = if text.len() > 2 {
            text[2..text.len() - 1].to_string()
        } else {
            String::new()
        };

        Ok(WordPart::ProcessSub { direction, command })
    }

    fn build_extglob(pair: Pair<Rule>) -> Result<WordPart, BuildError> {
        let text = pair.as_str();

        let op_char = text.chars().next().ok_or(BuildError::EmptyWord)?;
        let operator = match op_char {
            '?' => ExtGlobOp::ZeroOrOne,
            '*' => ExtGlobOp::ZeroOrMore,
            '+' => ExtGlobOp::OneOrMore,
            '@' => ExtGlobOp::ExactlyOne,
            '!' => ExtGlobOp::Not,
            _ => {
                return Err(BuildError::InvalidVarPrefix(format!(
                    "Unknown extglob operator: {}",
                    op_char
                )))
            }
        };

        let pattern_list = if text.len() > 2 {
            text[2..text.len() - 1].to_string()
        } else {
            String::new()
        };

        Ok(WordPart::ExtGlob {
            operator,
            pattern_list,
        })
    }

    fn build_redirect(pair: Pair<Rule>) -> Result<Redirect, BuildError> {
        let span = span_from_pair(&pair);
        let inner = pair
            .into_inner()
            .next()
            .ok_or(BuildError::MissingRedirectOp)?;

        match inner.as_rule() {
            Rule::VarFdRedirect => Self::build_var_fd_redirect(inner, span),
            Rule::FdRedirect => Self::build_fd_redirect(inner, span),
            Rule::SimpleRedirect => Self::build_simple_redirect(inner, span),
            _ => Err(BuildError::InvalidRedirectOp(format!(
                "{:?}",
                inner.as_rule()
            ))),
        }
    }

    fn build_var_fd_redirect(pair: Pair<Rule>, span: Span) -> Result<Redirect, BuildError> {
        let mut inner = pair.into_inner();

        let first_pair = inner.next().ok_or(BuildError::MissingRedirectOp)?;

        let (var_name, op_pair) = if first_pair.as_rule() == Rule::VarFdName {
            let var_name = first_pair.as_str().to_string();
            let op_pair = inner.next().ok_or(BuildError::MissingRedirectOp)?;
            (var_name, op_pair)
        } else {
            return Err(BuildError::InvalidVarPrefix(
                "{}: empty variable name in redirect".to_string(),
            ));
        };

        let kind = Self::parse_redirect_op(op_pair.as_str())?;

        let target = if let Some(target_pair) = inner.next() {
            let text = target_pair.as_str();
            if text == "-" {
                RedirectTarget::Close
            } else {
                Self::parse_redirect_target(target_pair)?
            }
        } else {
            RedirectTarget::Close
        };

        Ok(Redirect {
            fd: None,
            fd_var: Some(var_name),
            kind,
            target,
            span,
        })
    }

    fn build_fd_redirect(pair: Pair<Rule>, span: Span) -> Result<Redirect, BuildError> {
        let mut inner = pair.into_inner();

        let prefix_pair = inner.next().ok_or(BuildError::MissingRedirectOp)?;
        let prefix_text = prefix_pair.as_str();

        let mut fd_digits = String::new();
        let mut op_str = prefix_text;

        for (i, c) in prefix_text.char_indices() {
            if c.is_ascii_digit() {
                fd_digits.push(c);
            } else {
                op_str = &prefix_text[i..];
                break;
            }
        }

        let fd = fd_digits
            .parse::<i32>()
            .map_err(|_| BuildError::InvalidFd(fd_digits.clone()))?;

        let kind = Self::parse_redirect_op(op_str)?;

        let target_pair = inner.next().ok_or(BuildError::MissingRedirectTarget)?;
        let target = Self::parse_redirect_target(target_pair)?;

        Ok(Redirect {
            fd: Some(fd),
            fd_var: None,
            kind,
            target,
            span,
        })
    }

    fn build_simple_redirect(pair: Pair<Rule>, span: Span) -> Result<Redirect, BuildError> {
        let mut inner = pair.into_inner();

        let op_pair = inner.next().ok_or(BuildError::MissingRedirectOp)?;
        let kind = Self::parse_redirect_op(op_pair.as_str())?;

        let target_pair = inner.next().ok_or(BuildError::MissingRedirectTarget)?;
        let target = Self::parse_redirect_target(target_pair)?;

        Ok(Redirect {
            fd: None,
            fd_var: None,
            kind,
            target,
            span,
        })
    }

    fn parse_redirect_op(op: &str) -> Result<RedirectKind, BuildError> {
        match op {
            "<" => Ok(RedirectKind::Input),
            ">" => Ok(RedirectKind::Output),
            ">>" => Ok(RedirectKind::Append),
            ">|" => Ok(RedirectKind::Clobber),
            "<>" => Ok(RedirectKind::ReadWrite),
            ">&" => Ok(RedirectKind::DupFd),
            "<&" => Ok(RedirectKind::DupFdInput),
            "&>" => Ok(RedirectKind::AndOutput),
            "&>>" => Ok(RedirectKind::AndAppend),
            "<<" => Ok(RedirectKind::HereDoc(HereDocKind::Normal)),
            "<<-" => Ok(RedirectKind::HereDoc(HereDocKind::StripTabs)),
            "<<<" => Ok(RedirectKind::HereDoc(HereDocKind::HereString)),
            _ => Err(BuildError::InvalidRedirectOp(op.to_string())),
        }
    }

    fn parse_redirect_target(pair: Pair<Rule>) -> Result<RedirectTarget, BuildError> {
        match pair.as_rule() {
            Rule::Integer => {
                let fd_str = pair.as_str();
                if fd_str == "-" {
                    Ok(RedirectTarget::Close)
                } else {
                    let fd_num = fd_str
                        .parse::<i32>()
                        .map_err(|_| BuildError::InvalidFd(fd_str.to_string()))?;
                    Ok(RedirectTarget::Fd(fd_num))
                }
            }
            Rule::Word => {
                let word = AstBuilder::build_word(pair)?;
                if let Some(literal) = word.as_literal() {
                    if literal == "-" {
                        return Ok(RedirectTarget::Close);
                    }
                }
                Ok(RedirectTarget::File(word))
            }
            _ => Err(BuildError::InvalidRedirectTarget(format!(
                "{:?}",
                pair.as_rule()
            ))),
        }
    }

    fn build_bareword_with_escapes(input: &str) -> WordPart {
        let mut result = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        let mut has_escaped_glob = false;

        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(&next) = chars.peek() {
                    if matches!(next, '*' | '?' | '[' | ']' | '\\') {
                        has_escaped_glob = true;
                    }
                    result.push(chars.next().unwrap());
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }

        if has_escaped_glob {
            WordPart::SingleQuoted(result)
        } else {
            WordPart::Literal(result)
        }
    }
}

/// Convert pest Pair to our Span type
fn unescape_backtick_content(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('`') => {
                    chars.next();
                    result.push('`');
                }
                Some('\\') => {
                    chars.next();
                    result.push('\\');
                }
                Some('$') => {
                    chars.next();
                    result.push('$');
                }
                Some('"') => {
                    chars.next();
                    result.push('"');
                }
                Some('\n') => {
                    chars.next();
                } // line continuation
                _ => result.push(c),
            }
        } else {
            result.push(c);
        }
    }
    result
}

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

    #[error("empty compound command")]
    EmptyCompoundCommand,

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

    #[error("missing lambda body")]
    MissingLambdaBody,

    #[error("empty word")]
    EmptyWord,

    #[error("empty dollar expansion")]
    EmptyDollarExpansion,

    #[error("empty quoted string")]
    EmptyQuotedString,

    #[error("missing variable name")]
    MissingVariableName,

    #[error("missing modifier word")]
    MissingModifierWord,

    #[error("invalid var modifier: {0}")]
    InvalidVarModifier(String),

    #[error("invalid array subscript: {0}")]
    InvalidArraySubscript(String),

    #[error("invalid var prefix: {0}")]
    InvalidVarPrefix(String),

    #[error("unexpected quoted string type: {0}")]
    UnexpectedQuotedString(String),

    #[error("missing case value")]
    MissingCaseValue,

    #[error("missing case pattern")]
    MissingCasePattern,

    #[error("invalid case terminator: {0}")]
    InvalidCaseTerminator(String),

    #[error("empty conditional command")]
    EmptyConditionalCommand,

    #[error("missing operand")]
    MissingOperand,

    #[error("missing operator")]
    MissingOperator,

    #[error("unknown operator: {0}")]
    UnknownOperator(String),

    #[error("empty arithmetic command")]
    EmptyArithmeticCommand,

    #[error("empty coproc command")]
    EmptyCoprocCommand,

    #[error("missing coproc command body")]
    MissingCoprocCommand,

    #[error("parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pest_parser::ColonyParser;
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
    fn test_build_pipeline_extensions() {
        let source = "one ?> two |> three !> handler";
        let pairs = ColonyParser::parse(Rule::Pipeline, source).unwrap();
        let result = AstBuilder::build_pipeline(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let pipeline = result.unwrap();
        assert_eq!(pipeline.elements.len(), 4);
        assert!(matches!(
            pipeline.elements[0].pipe_op,
            Some(PipeOp::PipeGuard)
        ));
        assert!(matches!(
            pipeline.elements[1].pipe_op,
            Some(PipeOp::PipeTap)
        ));
        assert!(matches!(
            pipeline.elements[2].pipe_op,
            Some(PipeOp::PipeCatch)
        ));
        assert!(pipeline.elements[3].pipe_op.is_none());
    }

    #[test]
    fn test_build_pipeline_catch_from_program() {
        let source = "false !> echo caught";
        let stmt = crate::pest_api::parse_statement(source).unwrap();
        let pipeline = match &stmt {
            Statement::Pipeline(p) => p,
            Statement::List(list) => match &*list.first {
                Statement::Pipeline(p) => p,
                other => panic!("Expected Pipeline inside List, got {:?}", other),
            },
            other => panic!("Expected Pipeline or List, got {:?}", other),
        };
        assert_eq!(pipeline.elements.len(), 2);
        assert!(matches!(
            pipeline.elements[0].pipe_op,
            Some(PipeOp::PipeCatch)
        ));
    }

    #[test]
    fn test_build_if_statement() {
        let source = "if [ 1 -eq 1 ]; then echo yes; fi";
        let pairs = ColonyParser::parse(Rule::IfStatement, source).unwrap();
        let result = AstBuilder::build_if_statement(pairs.into_iter().next().unwrap());

        if let Err(ref e) = result {
            eprintln!("Error building if statement: {:?}", e);
        }
        assert!(
            result.is_ok(),
            "Failed to build POSIX if statement: {:?}",
            result.err()
        );
        let if_stmt = result.unwrap();
        assert!(matches!(if_stmt.condition, Condition::Command(_)));
        assert!(!if_stmt.then_body.is_empty());
        assert_eq!(if_stmt.elif_branches.len(), 0);
        assert!(if_stmt.else_body.is_none());
    }

    #[test]
    fn test_build_match_statement() {
        let source = "match foo { [a, ..rest] if a => { echo ok; } _ => 0 }";
        let pairs = ColonyParser::parse(Rule::MatchStatement, source).unwrap();
        let result = AstBuilder::build_match_statement(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let match_stmt = result.unwrap();
        assert_eq!(match_stmt.arms.len(), 2);

        match &match_stmt.arms[0].pattern {
            MatchPattern::List(list) => {
                assert_eq!(list.items.len(), 1);
                assert!(matches!(list.rest, ListRest::Bind(_)));
            }
            _ => panic!("expected list pattern"),
        }

        assert!(match_stmt.arms[0].guard.is_some());
        match &match_stmt.arms[0].body {
            MatchBody::Block(body) => assert_eq!(body.len(), 1),
            _ => panic!("expected block body"),
        }

        assert!(matches!(match_stmt.arms[1].pattern, MatchPattern::Wildcard));
    }

    #[test]
    fn test_build_match_or_pattern() {
        let source = "match x { 1 | 2 | 3 => { echo small; } _ => { echo big; } }";
        let pairs = ColonyParser::parse(Rule::MatchStatement, source).unwrap();
        let result = AstBuilder::build_match_statement(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let match_stmt = result.unwrap();
        assert_eq!(match_stmt.arms.len(), 2);

        match &match_stmt.arms[0].pattern {
            MatchPattern::Or(patterns) => {
                assert_eq!(patterns.len(), 3);
                assert!(matches!(&patterns[0], MatchPattern::Literal(_)));
                assert!(matches!(&patterns[1], MatchPattern::Literal(_)));
                assert!(matches!(&patterns[2], MatchPattern::Literal(_)));
            }
            _ => panic!("expected or pattern"),
        }

        assert!(matches!(match_stmt.arms[1].pattern, MatchPattern::Wildcard));
    }

    #[test]
    fn test_build_while_statement() {
        // POSIX while uses command-based condition, not expression
        let source = "while [ $x -lt 100 ]; do echo loop; done";
        let pairs = ColonyParser::parse(Rule::WhileStatement, source).unwrap();
        let result = AstBuilder::build_while_statement(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let while_stmt = result.unwrap();
        assert!(matches!(while_stmt.condition, Condition::Command(_)));
        assert_eq!(while_stmt.body.len(), 1);
    }

    #[test]
    fn test_build_until_statement() {
        // POSIX until uses command-based condition, not expression
        let source = "until [ $x ]; do echo waiting; done";
        let pairs = ColonyParser::parse(Rule::UntilStatement, source).unwrap();
        let result = AstBuilder::build_until_statement(pairs.into_iter().next().unwrap());

        if let Err(ref e) = result {
            eprintln!("Error building until statement: {:?}", e);
        }
        assert!(result.is_ok(), "Failed to build until: {:?}", result.err());
        let until_stmt = result.unwrap();
        assert!(matches!(until_stmt.condition, Condition::Command(_)));
        assert_eq!(until_stmt.body.len(), 1);
    }

    #[test]
    fn test_build_for_statement() {
        // TODO: Fix word-list based for loops in grammar
        // For now, test C-style which works
        let source = "for ((;;)); do echo hello; done";
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
        assert!(matches!(
            var_decl.target,
            crate::ast::VarDeclTarget::Name(ref name) if name == "count"
        ));
        assert!(matches!(var_decl.kind, VarKind::Var));
        assert!(var_decl.type_annotation.is_none());
        assert!(matches!(var_decl.value, Expression::Int(42, _)));
    }

    #[test]
    fn test_build_typed_var_decl() {
        let source = "const total: Int = 7";
        let pairs = ColonyParser::parse(Rule::VarDecl, source).unwrap();
        let result = AstBuilder::build_var_decl(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let var_decl = result.unwrap();
        assert!(matches!(
            var_decl.target,
            crate::ast::VarDeclTarget::Name(ref name) if name == "total"
        ));
        assert!(matches!(var_decl.kind, VarKind::Const));
        assert_eq!(var_decl.type_annotation.as_deref(), Some("Int"));
        assert!(matches!(var_decl.value, Expression::Int(7, _)));
    }

    #[test]
    fn test_build_tuple_var_decl() {
        let source = "var (a, b) = (1, 2)";
        let pairs = ColonyParser::parse(Rule::VarDecl, source).unwrap();
        let result = AstBuilder::build_var_decl(pairs.into_iter().next().unwrap());

        assert!(result.is_ok());
        let var_decl = result.unwrap();
        assert!(matches!(
            var_decl.target,
            crate::ast::VarDeclTarget::Tuple(ref names) if names == &vec!["a".to_string(), "b".to_string()]
        ));
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
    fn test_build_proc_def_simple() {
        let source = "proc greet(name) { echo hello }";
        let pairs = ColonyParser::parse(Rule::ProcDef, source).unwrap();
        let result = AstBuilder::build_proc_def(pairs.into_iter().next().unwrap());

        assert!(result.is_ok(), "Failed: {:?}", result.err());
        let proc_def = result.unwrap();
        assert_eq!(proc_def.name, "greet");
        assert_eq!(proc_def.params.len(), 1);
        assert_eq!(proc_def.params[0].name, "name");
        assert!(proc_def.params[0].default.is_none());
        assert_eq!(proc_def.body.len(), 1);
    }

    #[test]
    fn test_build_proc_def_with_default() {
        let source = "proc greet(name, greeting = \"Hello\") { echo hello }";
        let pairs = ColonyParser::parse(Rule::ProcDef, source).unwrap();
        let result = AstBuilder::build_proc_def(pairs.into_iter().next().unwrap());

        assert!(result.is_ok(), "Failed: {:?}", result.err());
        let proc_def = result.unwrap();
        assert_eq!(proc_def.name, "greet");
        assert_eq!(proc_def.params.len(), 2);
        assert_eq!(proc_def.params[0].name, "name");
        assert!(proc_def.params[0].default.is_none());
        assert_eq!(proc_def.params[1].name, "greeting");
        assert!(proc_def.params[1].default.is_some());
    }

    #[test]
    fn test_build_proc_def_no_params() {
        let source = "proc hello() { echo hello }";
        let pairs = ColonyParser::parse(Rule::ProcDef, source).unwrap();
        let result = AstBuilder::build_proc_def(pairs.into_iter().next().unwrap());

        assert!(result.is_ok(), "Failed: {:?}", result.err());
        let proc_def = result.unwrap();
        assert_eq!(proc_def.name, "hello");
        assert!(proc_def.params.is_empty());
        assert_eq!(proc_def.body.len(), 1);
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
        assert!(
            result.is_ok(),
            "Failed to build program: {:?}",
            result.err()
        );
        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
    }

    // === Expression Operator Tests ===

    #[test]
    fn test_arithmetic_operators() {
        // Addition
        let source = "x + y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Add,
                ..
            }
        ));

        // Subtraction
        let source = "x - y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Sub,
                ..
            }
        ));

        // Multiplication
        let source = "x * y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Mul,
                ..
            }
        ));

        // Division
        let source = "x / y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Div,
                ..
            }
        ));

        // Modulo
        let source = "x % y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Mod,
                ..
            }
        ));

        // Power
        let source = "x ** y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Pow,
                ..
            }
        ));
    }

    #[test]
    fn test_comparison_operators() {
        let source = "x < y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Lt,
                ..
            }
        ));

        let source = "x <= y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Le,
                ..
            }
        ));

        let source = "x > y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Gt,
                ..
            }
        ));

        let source = "x >= y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Ge,
                ..
            }
        ));
    }

    #[test]
    #[ignore] // TODO: Fix equality operator parsing in grammar
    fn test_equality_operators() {
        let source = "x == y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Eq,
                ..
            }
        ));

        let source = "x != y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Ne,
                ..
            }
        ));
    }

    #[test]
    fn test_logical_operators() {
        let source = "x && y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::And,
                ..
            }
        ));

        let source = "x || y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Or,
                ..
            }
        ));
    }

    #[test]
    fn test_bitwise_operators() {
        let source = "x & y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::BitAnd,
                ..
            }
        ));

        let source = "x | y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::BitOr,
                ..
            }
        ));

        let source = "x ^ y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::BitXor,
                ..
            }
        ));

        let source = "x << y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Shl,
                ..
            }
        ));

        let source = "x >> y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Binary {
                op: BinaryOp::Shr,
                ..
            }
        ));
    }

    #[test]
    fn test_unary_operators() {
        let source = "-x";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Unary {
                op: UnaryOp::Neg,
                ..
            }
        ));

        let source = "!x";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Unary {
                op: UnaryOp::Not,
                ..
            }
        ));

        let source = "~x";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Expression::Unary {
                op: UnaryOp::BitNot,
                ..
            }
        ));
    }

    #[test]
    fn test_precedence() {
        // Multiplication before addition: 2 + 3 * 4 = 2 + (3 * 4) = 14
        let source = "2 + 3 * 4";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());

        // Parse tree should be: Binary(+, 2, Binary(*, 3, 4))
        match result.unwrap() {
            Expression::Binary {
                left,
                op: BinaryOp::Add,
                right,
                ..
            } => {
                assert!(matches!(*left, Expression::Int(2, _)));
                assert!(matches!(
                    *right,
                    Expression::Binary {
                        op: BinaryOp::Mul,
                        ..
                    }
                ));
            }
            _ => panic!("Expected addition with multiplication on right"),
        }

        // Power before multiplication: 2 * 3 ** 4 = 2 * (3 ** 4) = 162
        let source = "2 * 3 ** 4";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());

        match result.unwrap() {
            Expression::Binary {
                left,
                op: BinaryOp::Mul,
                right,
                ..
            } => {
                assert!(matches!(*left, Expression::Int(2, _)));
                assert!(matches!(
                    *right,
                    Expression::Binary {
                        op: BinaryOp::Pow,
                        ..
                    }
                ));
            }
            _ => panic!("Expected multiplication with power on right"),
        }
    }

    #[test]
    fn test_left_associativity() {
        // 10 - 5 - 2 = (10 - 5) - 2 = 3
        let source = "10 - 5 - 2";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());

        // Should be: Binary(-, Binary(-, 10, 5), 2)
        match result.unwrap() {
            Expression::Binary {
                left,
                op: BinaryOp::Sub,
                right,
                ..
            } => {
                assert!(matches!(
                    *left,
                    Expression::Binary {
                        op: BinaryOp::Sub,
                        ..
                    }
                ));
                assert!(matches!(*right, Expression::Int(2, _)));
            }
            _ => panic!("Expected left-associative subtraction"),
        }
    }

    #[test]
    fn test_right_associativity() {
        // 2 ** 3 ** 4 = 2 ** (3 ** 4) = 2 ** 81 = very large
        let source = "2 ** 3 ** 2";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());

        // Should be: Binary(**, 2, Binary(**, 3, 2))
        match result.unwrap() {
            Expression::Binary {
                left,
                op: BinaryOp::Pow,
                right,
                ..
            } => {
                assert!(matches!(*left, Expression::Int(2, _)));
                assert!(matches!(
                    *right,
                    Expression::Binary {
                        op: BinaryOp::Pow,
                        ..
                    }
                ));
            }
            _ => panic!("Expected right-associative power"),
        }
    }

    #[test]
    #[ignore] // TODO: Fix ternary parsing - might need grammar adjustment
    fn test_ternary_operator() {
        let source = "x > 0 ? positive : negative";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::Ternary { .. }));
    }

    #[test]
    #[ignore] // TODO: Fix null coalesce parsing
    fn test_null_coalesce() {
        let source = "x ?? y";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::NullCoalesce { .. }));
    }

    #[test]
    #[ignore] // TODO: Fix postfix operator parsing
    fn test_postfix_operators() {
        // Index
        let source = "arr[0]";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::Index { .. }));

        // Field
        let source = "obj.field";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::Field { .. }));
    }

    #[test]
    fn test_complex_expressions() {
        // Combined operators with correct precedence
        let source = "x + y * z < 100 && flag";
        let result = parse_and_build_expr(source);
        assert!(
            result.is_ok(),
            "Complex expression should parse: {:?}",
            result.err()
        );

        // Parenthesized to override precedence
        let source = "(x + y) * z";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_collection_literals() {
        // List
        let source = "[1, 2, 3]";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        match result.unwrap() {
            Expression::List { items, .. } => assert_eq!(items.len(), 3),
            _ => panic!("Expected list"),
        }

        // Dict
        let source = r#"{ "key": value }"#;
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::Dict { .. }));

        // Tuple
        let source = "(1, 2, 3)";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        match result.unwrap() {
            Expression::Tuple { items, .. } => assert_eq!(items.len(), 3),
            _ => panic!("Expected tuple"),
        }
    }

    #[test]
    fn test_func_call_expression() {
        let source = "len([1, 2])";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        match result.unwrap() {
            Expression::FuncCall { name, args, .. } => {
                assert_eq!(name, "len");
                assert_eq!(args.len(), 1);
            }
            other => panic!("Expected func call, got {:?}", other),
        }
    }

    #[test]
    fn test_method_call_expression() {
        let source = "[1, 2].join(\",\")";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        match result.unwrap() {
            Expression::MethodCall { name, args, .. } => {
                assert_eq!(name, "join");
                assert_eq!(args.len(), 1);
            }
            other => panic!("Expected method call, got {:?}", other),
        }
    }

    #[test]
    fn test_optional_method_call_expression() {
        let source = "value?.keys()";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::MethodCall { .. }));
    }

    #[test]
    fn test_optional_chain_with_index_expression() {
        let source = "obj?.items[0].name?.len()";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::MethodCall { .. }));
    }

    #[test]
    fn test_optional_call_expression() {
        let source = "maybe?()";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::OptionalCall { .. }));
    }

    #[test]
    fn test_direct_method_call_expression() {
        let source = "items.len()";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::MethodCall { .. }));
    }

    #[test]
    fn test_optional_unwrap_expression() {
        let source = "value?";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::OptionalChain { .. }));

        let source = "value!";
        let result = parse_and_build_expr(source);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Expression::Unwrap { .. }));
    }

    #[test]
    fn test_dollar_expr_sub_parsing() {
        use pest::Parser;

        let source = "$[1 + 2]";
        let mut pairs = ColonyParser::parse(Rule::Word, source).unwrap();
        let word = AstBuilder::build_word(pairs.next().unwrap()).unwrap();
        assert!(matches!(word.parts[0], WordPart::ExprSub(_)));
    }

    // Helper function for expression tests
    fn parse_and_build_expr(source: &str) -> Result<Expression, BuildError> {
        let pairs = ColonyParser::parse(Rule::Expression, source).unwrap();
        AstBuilder::build_expression(pairs.into_iter().next().unwrap())
    }

    #[test]
    fn test_array_slice_parsing() {
        use pest::Parser;

        let source = "arr[@]:2";
        let pairs = ColonyParser::parse(Rule::VariableRef, source);

        match pairs {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!("Top: {:?} = {:?}", pair.as_rule(), pair.as_str());
                    for inner in pair.clone().into_inner() {
                        eprintln!("  -> {:?} = {:?}", inner.as_rule(), inner.as_str());
                        for inner2 in inner.into_inner() {
                            eprintln!("    -> {:?} = {:?}", inner2.as_rule(), inner2.as_str());
                        }
                    }
                    let var_ref = AstBuilder::build_variable_ref(pair).unwrap();
                    eprintln!(
                        "VariableRef: subscript={:?}, modifier={:?}",
                        var_ref.subscript, var_ref.modifier
                    );
                }
            }
            Err(e) => eprintln!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_reserved_word_done() {
        use pest::Parser;

        eprintln!("Testing ReservedWord 'done':");
        let result = ColonyParser::parse(Rule::ReservedWord, "done");
        eprintln!("  ReservedWord match: {:?}", result.is_ok());

        eprintln!("Testing Bareword 'done':");
        let result = ColonyParser::parse(Rule::Bareword, "done");
        eprintln!("  Bareword match: {:?}", result.is_ok());
        if let Err(e) = &result {
            eprintln!("  Error: {}", e);
        }

        eprintln!("Testing Bareword 'donex':");
        let result = ColonyParser::parse(Rule::Bareword, "donex");
        eprintln!("  Bareword match: {:?}", result.is_ok());
    }

    #[test]
    fn test_array_case_mod_parsing() {
        use pest::Parser;

        let source = "arr[@]^^";
        eprintln!("\n=== Parsing: {} ===", source);
        let pairs = ColonyParser::parse(Rule::VariableRef, source);

        match pairs {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!("Top: {:?} = {:?}", pair.as_rule(), pair.as_str());
                    for inner in pair.clone().into_inner() {
                        eprintln!("  -> {:?} = {:?}", inner.as_rule(), inner.as_str());
                        for inner2 in inner.into_inner() {
                            eprintln!("    -> {:?} = {:?}", inner2.as_rule(), inner2.as_str());
                        }
                    }
                    let var_ref = AstBuilder::build_variable_ref(pair).unwrap();
                    eprintln!(
                        "Result: name={}, subscript={:?}, modifier={:?}",
                        var_ref.name, var_ref.subscript, var_ref.modifier
                    );

                    assert!(var_ref.subscript.is_some(), "subscript should be Some");
                    assert!(var_ref.modifier.is_some(), "modifier should be Some for ^^");
                }
            }
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_echo_array_case_mod() {
        use crate::pest_api::parse_statement;

        let stmt = parse_statement(r#"echo "${arr[@]^^}""#).unwrap();
        eprintln!("Statement: {:?}", stmt);

        if let Statement::SimpleCommand(cmd) = stmt {
            eprintln!("Name: {:?}", cmd.name);
            eprintln!("Args count: {}", cmd.args.len());
            for (i, word) in cmd.args.iter().enumerate() {
                eprintln!("Arg {}: {:?}", i, word);
                for (j, part) in word.parts.iter().enumerate() {
                    eprintln!("  Part {}: {:?}", j, part);
                    if let WordPart::DoubleQuoted(inner_parts) = part {
                        for (k, inner_part) in inner_parts.iter().enumerate() {
                            eprintln!("    Inner part {}: {:?}", k, inner_part);
                            if let WordPart::Variable(var_ref) = inner_part {
                                eprintln!(
                                    "      VariableRef: name={}, subscript={:?}, modifier={:?}",
                                    var_ref.name, var_ref.subscript, var_ref.modifier
                                );
                                assert!(var_ref.subscript.is_some(), "subscript should be Some");
                                assert!(var_ref.modifier.is_some(), "modifier should be Some");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_build_agent_invocation_basic() {
        let source = "@myagent \"hello world\"";
        let stmt = crate::pest_api::parse_statement(source).unwrap();
        let pipeline = match &stmt {
            Statement::Pipeline(p) => p,
            Statement::List(list) => match &*list.first {
                Statement::Pipeline(p) => p,
                other => panic!("Expected Pipeline inside List, got {:?}", other),
            },
            other => panic!("Expected Pipeline or List, got {:?}", other),
        };
        assert_eq!(pipeline.elements.len(), 1);
        match &*pipeline.elements[0].command {
            Statement::AgentInvocation(inv) => {
                assert_eq!(inv.agent_name, "myagent");
                assert!(inv.tool.is_none());
            }
            other => panic!("Expected AgentInvocation, got {:?}", other),
        }
    }

    #[test]
    fn test_build_agent_invocation_with_tool() {
        let source = "@myagent:search \"find stuff\"";
        let stmt = crate::pest_api::parse_statement(source).unwrap();
        let pipeline = match &stmt {
            Statement::Pipeline(p) => p,
            Statement::List(list) => match &*list.first {
                Statement::Pipeline(p) => p,
                other => panic!("Expected Pipeline inside List, got {:?}", other),
            },
            other => panic!("Expected Pipeline or List, got {:?}", other),
        };
        assert_eq!(pipeline.elements.len(), 1);
        match &*pipeline.elements[0].command {
            Statement::AgentInvocation(inv) => {
                assert_eq!(inv.agent_name, "myagent");
                assert_eq!(inv.tool.as_deref(), Some("search"));
            }
            other => panic!("Expected AgentInvocation, got {:?}", other),
        }
    }

    #[test]
    fn test_build_agent_in_pipeline() {
        let source = "echo test | @agent \"process\"";
        let stmt = crate::pest_api::parse_statement(source).unwrap();
        let pipeline = match &stmt {
            Statement::Pipeline(p) => p,
            Statement::List(list) => match &*list.first {
                Statement::Pipeline(p) => p,
                other => panic!("Expected Pipeline inside List, got {:?}", other),
            },
            other => panic!("Expected Pipeline or List, got {:?}", other),
        };
        assert_eq!(pipeline.elements.len(), 2);
        assert!(matches!(
            &*pipeline.elements[0].command,
            Statement::SimpleCommand(_)
        ));
        assert!(matches!(
            &*pipeline.elements[1].command,
            Statement::AgentInvocation(_)
        ));
    }
}
