//! Recursive descent parser for Colony shell grammar
//!
//! Parses token stream into AST following POSIX shell grammar with Colony extensions.

use crate::{
    ast::*,
    span::{Position, Span},
    token::{Token, TokenKind, TokenValue},
    ParseError, Result,
};

/// Parser that builds AST from token stream
///
/// # Deprecated
///
/// This recursive descent parser is deprecated in favor of the pure PEG approach
/// using `pest_api::parse()`. The pest parser operates directly on source text
/// and uses a declarative grammar (colony.pest) instead of hand-written code.
///
/// **Benefits of pest**:
/// - No separate lexer needed
/// - Declarative grammar (easier to maintain)
/// - Better error messages
/// - Automatic precedence from grammar nesting
///
/// Use `colony_parser::pest_api::parse()` instead of `Lexer::new()` + `Parser::new()`.
///
/// This API is kept for reference and backward compatibility, but will be removed
/// in a future version.
#[deprecated(
    since = "0.2.0",
    note = "Use colony_parser::pest_api::parse() instead. See docs/PURE_PEG_FINDINGS.md"
)]
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    /// Create a new parser for the given tokens
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parse the token stream into a Program
    pub fn parse(&mut self) -> Result<Program> {
        let start = self.position();
        let mut statements = Vec::new();

        while !self.is_at_end() {
            // Skip newlines and comments
            self.skip_newlines_and_comments();

            if self.is_at_end() {
                break;
            }

            let stmt = self.parse_statement()?;
            statements.push(stmt);

            // Consume optional trailing newline/semicolon
            self.skip_newlines_and_comments();
        }

        let end = self.position();

        Ok(Program {
            statements,
            span: Span::new(start, end),
        })
    }

    /// Parse a complete statement
    fn parse_statement(&mut self) -> Result<Statement> {
        self.parse_list()
    }

    /// Parse command list (top level with &&, ||, ;, &)
    fn parse_list(&mut self) -> Result<Statement> {
        let mut left = self.parse_pipeline()?;

        // Check for list operators (&&, ||, ;, &)
        if self.check_list_op() {
            let start = left.span().start;
            let mut rest = Vec::new();

            while self.check_list_op() {
                let op = self.parse_list_op()?;

                self.skip_newlines_and_comments();

                let right = self.parse_pipeline()?;
                rest.push((op, right));
            }

            let end = if let Some((_, last)) = rest.last() {
                last.span().end
            } else {
                left.span().end
            };

            left = Statement::List(CommandList {
                first: Box::new(left),
                rest,
                span: Span::new(start, end),
            });
        }

        Ok(left)
    }

    /// Parse a pipeline (without list operators)
    fn parse_pipeline(&mut self) -> Result<Statement> {
        let mut left = self.parse_simple_or_compound()?;

        // Check for pipeline operators
        if self.check_pipe_op() {
            let start = left.span().start;
            let mut elements = vec![PipelineElement {
                command: Box::new(left),
                pipe_op: None,
                negated: false,
            }];

            while self.check_pipe_op() {
                let pipe_op = self.parse_pipe_op()?;

                // Parse next command
                let cmd = self.parse_simple_or_compound()?;

                // Set the pipe op on the previous element
                if let Some(prev) = elements.last_mut() {
                    prev.pipe_op = Some(pipe_op);
                }

                elements.push(PipelineElement {
                    command: Box::new(cmd),
                    pipe_op: None,
                    negated: false,
                });
            }

            let end = elements.last().unwrap().command.span().end;

            left = Statement::Pipeline(Pipeline {
                elements,
                span: Span::new(start, end),
            });
        }

        Ok(left)
    }

    /// Parse simple command or compound command
    fn parse_simple_or_compound(&mut self) -> Result<Statement> {
        match self.peek_kind() {
            Some(TokenKind::LParen) => self.parse_subshell(),
            Some(TokenKind::LBrace) => self.parse_brace_group(),
            Some(TokenKind::If) => self.parse_if(),
            Some(TokenKind::For) => self.parse_for(),
            Some(TokenKind::While) => self.parse_while(),
            Some(TokenKind::Until) => self.parse_until(),
            Some(TokenKind::Case) => self.parse_case(),
            Some(TokenKind::Function) => self.parse_function_keyword(),
            Some(TokenKind::Word) | Some(TokenKind::Identifier) => {
                // Check for function definition: name() { body }
                if self.peek_at(1).map(|t| t.kind) == Some(TokenKind::LParen) {
                    self.parse_function_posix()
                } else {
                    self.parse_simple_command()
                }
            }
            _ => self.parse_simple_command(),
        }
    }

    /// Parse a simple command: name arg1 arg2 ... [redirects]
    fn parse_simple_command(&mut self) -> Result<Statement> {
        let start = self.position();

        // Parse optional environment assignments
        let env_assignments = Vec::new(); // TODO: Parse VAR=value before command

        // Parse command name
        let name = self.parse_word()?;

        // Parse arguments
        let mut args = Vec::new();
        while !self.is_at_end() && !self.check_pipe_or_list_op() && !self.check_compound_keyword() {
            if self.check(TokenKind::Newline) {
                break;
            }

            // TODO: Parse redirections
            // For now, just parse words
            match self.peek_kind() {
                Some(TokenKind::Word)
                | Some(TokenKind::SingleQuoted)
                | Some(TokenKind::DoubleQuoted) => {
                    args.push(self.parse_word()?);
                }
                _ => break,
            }
        }

        let end = if let Some(last_arg) = args.last() {
            last_arg.span.end
        } else {
            name.span.end
        };

        Ok(Statement::SimpleCommand(SimpleCommand {
            name,
            args,
            redirects: vec![],
            env_assignments,
            span: Span::new(start, end),
        }))
    }

    /// Parse a word from tokens
    fn parse_word(&mut self) -> Result<Word> {
        let token = self.advance().ok_or(ParseError::UnexpectedEof)?;

        let span = token.span;

        match &token.kind {
            TokenKind::Word | TokenKind::Identifier => {
                if let TokenValue::String(s) = &token.value {
                    Ok(Word {
                        parts: vec![WordPart::Literal(s.clone())],
                        span,
                    })
                } else {
                    Err(ParseError::UnexpectedToken(token.kind))
                }
            }
            TokenKind::SingleQuoted => {
                if let TokenValue::String(s) = &token.value {
                    Ok(Word {
                        parts: vec![WordPart::SingleQuoted(s.clone())],
                        span,
                    })
                } else {
                    Err(ParseError::UnexpectedToken(token.kind))
                }
            }
            TokenKind::DoubleQuoted => {
                if let TokenValue::Interpolated(parts) = &token.value {
                    // Convert interpolated parts to WordParts
                    let word_parts: Vec<WordPart> = parts
                        .iter()
                        .map(|p| match p {
                            crate::InterpolatedPart::Literal(s) => WordPart::Literal(s.clone()),
                            crate::InterpolatedPart::Variable(v) => {
                                WordPart::Variable(VariableRef {
                                    name: v.clone(),
                                    modifier: None,
                                    span,
                                })
                            }
                            _ => WordPart::Literal("".to_string()), // TODO: handle other interpolation types
                        })
                        .collect();

                    Ok(Word {
                        parts: word_parts,
                        span,
                    })
                } else {
                    Err(ParseError::UnexpectedToken(token.kind))
                }
            }
            _ => Err(ParseError::UnexpectedToken(token.kind)),
        }
    }

    /// Parse pipe operator
    fn parse_pipe_op(&mut self) -> Result<PipeOp> {
        let token = self.advance().ok_or(ParseError::UnexpectedEof)?;

        match token.kind {
            TokenKind::Pipe => Ok(PipeOp::Pipe),
            TokenKind::ArrowPipe => Ok(PipeOp::Arrow),
            TokenKind::FatArrowPipe => Ok(PipeOp::FatArrow),
            TokenKind::AlterPipe => Ok(PipeOp::Alter),
            TokenKind::ErrPipe => Ok(PipeOp::PipeErr),
            _ => Err(ParseError::UnexpectedToken(token.kind)),
        }
    }

    /// Parse list operator
    fn parse_list_op(&mut self) -> Result<ListOp> {
        let token = self.advance().ok_or(ParseError::UnexpectedEof)?;

        match token.kind {
            TokenKind::And2 => Ok(ListOp::And),
            TokenKind::Or2 => Ok(ListOp::Or),
            TokenKind::Semi => Ok(ListOp::Semi),
            TokenKind::Amp => Ok(ListOp::Background),
            _ => Err(ParseError::UnexpectedToken(token.kind)),
        }
    }

    /// Parse subshell: ( commands )
    fn parse_subshell(&mut self) -> Result<Statement> {
        let start = self.position();

        self.expect(TokenKind::LParen)?;
        self.skip_newlines_and_comments();

        let mut body = Vec::new();
        while !self.check(TokenKind::RParen) && !self.is_at_end() {
            body.push(self.parse_pipeline()?);

            // Consume optional semicolon or newline
            while self.check(TokenKind::Semi) || self.check(TokenKind::Newline) {
                self.advance();
            }
        }

        self.expect(TokenKind::RParen)?;
        let end = self.position();

        Ok(Statement::Subshell(Subshell {
            body,
            span: Span::new(start, end),
        }))
    }

    /// Parse brace group: { commands }
    fn parse_brace_group(&mut self) -> Result<Statement> {
        let start = self.position();

        self.expect(TokenKind::LBrace)?;
        self.skip_newlines_and_comments();

        let mut body = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.is_at_end() {
            body.push(self.parse_pipeline()?);

            // Consume optional semicolon or newline
            while self.check(TokenKind::Semi) || self.check(TokenKind::Newline) {
                self.advance();
            }
        }

        self.expect(TokenKind::RBrace)?;
        let end = self.position();

        Ok(Statement::BraceGroup(BraceGroup {
            body,
            span: Span::new(start, end),
        }))
    }

    /// Parse if statement: if cmd; then body [elif...] [else body] fi
    fn parse_if(&mut self) -> Result<Statement> {
        let start = self.position();

        self.expect(TokenKind::If)?;
        self.skip_newlines_and_comments();

        // Parse condition
        let condition = Condition::Command(Box::new(self.parse_pipeline()?));

        // Optional semicolon or newline before 'then'
        if self.check(TokenKind::Semi) {
            self.advance();
        }
        self.skip_newlines_and_comments();

        self.expect(TokenKind::Then)?;
        self.skip_newlines_and_comments();

        // Parse then body (each statement can be a list, but we manually handle separators)
        let mut then_body = Vec::new();
        while !self.check(TokenKind::Elif)
            && !self.check(TokenKind::Else)
            && !self.check(TokenKind::Fi)
            && !self.is_at_end()
        {
            then_body.push(self.parse_pipeline()?);

            // Consume optional separator between statements
            while self.check(TokenKind::Semi) || self.check(TokenKind::Newline) {
                self.advance();
            }
        }

        // Parse elif branches
        let mut elif_branches = Vec::new();
        while self.check(TokenKind::Elif) {
            self.advance(); // consume elif
            self.skip_newlines_and_comments();

            let elif_condition = Condition::Command(Box::new(self.parse_pipeline()?));

            if self.check(TokenKind::Semi) {
                self.advance();
            }
            self.skip_newlines_and_comments();

            self.expect(TokenKind::Then)?;
            self.skip_newlines_and_comments();

            let mut elif_body = Vec::new();
            while !self.check(TokenKind::Elif)
                && !self.check(TokenKind::Else)
                && !self.check(TokenKind::Fi)
                && !self.is_at_end()
            {
                elif_body.push(self.parse_pipeline()?);

                while self.check(TokenKind::Semi) || self.check(TokenKind::Newline) {
                    self.advance();
                }
            }

            elif_branches.push(ElifBranch {
                condition: elif_condition,
                body: elif_body,
            });
        }

        // Parse optional else
        let else_body = if self.check(TokenKind::Else) {
            self.advance(); // consume else
            self.skip_newlines_and_comments();

            let mut body = Vec::new();
            while !self.check(TokenKind::Fi) && !self.is_at_end() {
                body.push(self.parse_pipeline()?);

                while self.check(TokenKind::Semi) || self.check(TokenKind::Newline) {
                    self.advance();
                }
            }

            Some(body)
        } else {
            None
        };

        self.expect(TokenKind::Fi)?;
        let end = self.position();

        Ok(Statement::If(IfStatement {
            condition,
            then_body,
            elif_branches,
            else_body,
            span: Span::new(start, end),
        }))
    }

    /// Parse for statement: for name [in words...]; do body done
    fn parse_for(&mut self) -> Result<Statement> {
        let start = self.position();

        self.expect(TokenKind::For)?;
        self.skip_newlines_and_comments();

        // Parse variable name
        let var_token = self.advance().ok_or(ParseError::UnexpectedEof)?;
        let var = match &var_token.value {
            TokenValue::String(s) => s.clone(),
            _ => return Err(ParseError::UnexpectedToken(var_token.kind)),
        };

        self.skip_newlines_and_comments();

        // Parse optional 'in words...'
        let words = if self.check(TokenKind::In) {
            self.advance(); // consume 'in'
            self.skip_newlines_and_comments();

            let mut words = Vec::new();
            while !self.check(TokenKind::Semi)
                && !self.check(TokenKind::Newline)
                && !self.check(TokenKind::Do)
                && !self.is_at_end()
            {
                words.push(self.parse_word()?);
            }
            words
        } else {
            // Default to $@ (positional parameters)
            vec![]
        };

        // Optional semicolon or newline before 'do'
        if self.check(TokenKind::Semi) {
            self.advance();
        }
        self.skip_newlines_and_comments();

        self.expect(TokenKind::Do)?;
        self.skip_newlines_and_comments();

        // Parse body
        let mut body = Vec::new();
        while !self.check(TokenKind::Done) && !self.is_at_end() {
            body.push(self.parse_pipeline()?);

            while self.check(TokenKind::Semi) || self.check(TokenKind::Newline) {
                self.advance();
            }
        }

        self.expect(TokenKind::Done)?;
        let end = self.position();

        Ok(Statement::For(ForStatement {
            var,
            words,
            body,
            span: Span::new(start, end),
        }))
    }

    /// Parse while statement: while cmd; do body done
    fn parse_while(&mut self) -> Result<Statement> {
        let start = self.position();

        self.expect(TokenKind::While)?;
        self.skip_newlines_and_comments();

        // Parse condition
        let condition = Condition::Command(Box::new(self.parse_pipeline()?));

        // Optional semicolon or newline before 'do'
        if self.check(TokenKind::Semi) {
            self.advance();
        }
        self.skip_newlines_and_comments();

        self.expect(TokenKind::Do)?;
        self.skip_newlines_and_comments();

        // Parse body
        let mut body = Vec::new();
        while !self.check(TokenKind::Done) && !self.is_at_end() {
            body.push(self.parse_pipeline()?);

            while self.check(TokenKind::Semi) || self.check(TokenKind::Newline) {
                self.advance();
            }
        }

        self.expect(TokenKind::Done)?;
        let end = self.position();

        Ok(Statement::While(WhileStatement {
            condition,
            body,
            span: Span::new(start, end),
        }))
    }

    /// Parse until statement: until cmd; do body done
    fn parse_until(&mut self) -> Result<Statement> {
        let start = self.position();

        self.expect(TokenKind::Until)?;
        self.skip_newlines_and_comments();

        // Parse condition
        let condition = Condition::Command(Box::new(self.parse_pipeline()?));

        // Optional semicolon or newline before 'do'
        if self.check(TokenKind::Semi) {
            self.advance();
        }
        self.skip_newlines_and_comments();

        self.expect(TokenKind::Do)?;
        self.skip_newlines_and_comments();

        // Parse body
        let mut body = Vec::new();
        while !self.check(TokenKind::Done) && !self.is_at_end() {
            body.push(self.parse_pipeline()?);

            while self.check(TokenKind::Semi) || self.check(TokenKind::Newline) {
                self.advance();
            }
        }

        self.expect(TokenKind::Done)?;
        let end = self.position();

        Ok(Statement::Until(UntilStatement {
            condition,
            body,
            span: Span::new(start, end),
        }))
    }

    /// Parse case statement: case word in patterns) body ;; esac
    fn parse_case(&mut self) -> Result<Statement> {
        let start = self.position();

        self.expect(TokenKind::Case)?;
        self.skip_newlines_and_comments();

        // Parse the value to match against
        let value = self.parse_word()?;

        self.skip_newlines_and_comments();
        self.expect(TokenKind::In)?;
        self.skip_newlines_and_comments();

        // Parse case arms
        let mut arms = Vec::new();
        while !self.check(TokenKind::Esac) && !self.is_at_end() {
            // Parse patterns (pattern1 | pattern2 | pattern3)
            let mut patterns = vec![self.parse_word()?];

            while self.check(TokenKind::Pipe) {
                self.advance(); // consume |
                self.skip_newlines_and_comments();
                patterns.push(self.parse_word()?);
            }

            self.expect(TokenKind::RParen)?; // Pattern terminator
            self.skip_newlines_and_comments();

            // Parse body
            let mut body = Vec::new();
            while !self.check(TokenKind::SemiSemi)
                && !self.check(TokenKind::SemiAmp)
                && !self.check(TokenKind::SemiSemiAmp)
                && !self.check(TokenKind::Esac)
                && !self.is_at_end()
            {
                body.push(self.parse_pipeline()?);

                while self.check(TokenKind::Semi) || self.check(TokenKind::Newline) {
                    // Don't consume ;; or similar
                    if self.check(TokenKind::SemiSemi)
                        || self.check(TokenKind::SemiAmp)
                        || self.check(TokenKind::SemiSemiAmp)
                    {
                        break;
                    }
                    self.advance();
                }
            }

            // Parse terminator
            let terminator = if self.check(TokenKind::SemiSemi) {
                self.advance();
                CaseTerminator::Break
            } else if self.check(TokenKind::SemiAmp) {
                self.advance();
                CaseTerminator::Fallthrough
            } else if self.check(TokenKind::SemiSemiAmp) {
                self.advance();
                CaseTerminator::Continue
            } else {
                // If we hit esac without terminator, that's ok (last arm)
                CaseTerminator::Break
            };

            self.skip_newlines_and_comments();

            arms.push(CaseArm {
                patterns,
                body,
                terminator,
            });
        }

        self.expect(TokenKind::Esac)?;
        let end = self.position();

        Ok(Statement::Case(CaseStatement {
            value,
            arms,
            span: Span::new(start, end),
        }))
    }

    /// Parse POSIX function: name() compound_command
    fn parse_function_posix(&mut self) -> Result<Statement> {
        let start = self.position();

        // Parse function name
        let name_token = self.advance().ok_or(ParseError::UnexpectedEof)?;
        let name = match &name_token.value {
            TokenValue::String(s) => s.clone(),
            _ => return Err(ParseError::UnexpectedToken(name_token.kind)),
        };

        self.expect(TokenKind::LParen)?;
        self.expect(TokenKind::RParen)?;

        self.skip_newlines_and_comments();

        // Parse body (must be a compound command, typically { ... })
        let body_stmt = self.parse_simple_or_compound()?;

        // Extract body statements from the compound command
        let body = match body_stmt {
            Statement::BraceGroup(bg) => bg.body,
            Statement::Subshell(s) => s.body,
            _ => vec![body_stmt], // Allow any statement as body
        };

        let end = self.position();

        Ok(Statement::FunctionDef(FunctionDef {
            name,
            body,
            span: Span::new(start, end),
        }))
    }

    /// Parse function keyword: function name { body }
    fn parse_function_keyword(&mut self) -> Result<Statement> {
        let start = self.position();

        self.expect(TokenKind::Function)?;
        self.skip_newlines_and_comments();

        // Parse function name
        let name_token = self.advance().ok_or(ParseError::UnexpectedEof)?;
        let name = match &name_token.value {
            TokenValue::String(s) => s.clone(),
            _ => return Err(ParseError::UnexpectedToken(name_token.kind)),
        };

        self.skip_newlines_and_comments();

        // Optional () after name
        if self.check(TokenKind::LParen) {
            self.advance();
            self.expect(TokenKind::RParen)?;
            self.skip_newlines_and_comments();
        }

        // Parse body (must be a compound command)
        let body_stmt = self.parse_simple_or_compound()?;

        let body = match body_stmt {
            Statement::BraceGroup(bg) => bg.body,
            Statement::Subshell(s) => s.body,
            _ => vec![body_stmt],
        };

        let end = self.position();

        Ok(Statement::FunctionDef(FunctionDef {
            name,
            body,
            span: Span::new(start, end),
        }))
    }

    // === Helper Methods ===

    /// Check if current token is a pipe operator
    fn check_pipe_op(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(TokenKind::Pipe)
                | Some(TokenKind::ArrowPipe)
                | Some(TokenKind::FatArrowPipe)
                | Some(TokenKind::AlterPipe)
                | Some(TokenKind::ErrPipe)
        )
    }

    /// Check if current token is a list operator
    fn check_list_op(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(TokenKind::And2) | Some(TokenKind::Or2) | Some(TokenKind::Semi) | Some(TokenKind::Amp)
        )
    }

    /// Check if current token is pipe or list operator
    fn check_pipe_or_list_op(&self) -> bool {
        self.check_pipe_op() || self.check_list_op()
    }

    /// Check if current token is a compound command keyword that ends simple commands
    fn check_compound_keyword(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(TokenKind::Then)
                | Some(TokenKind::Do)
                | Some(TokenKind::Done)
                | Some(TokenKind::Fi)
                | Some(TokenKind::Elif)
                | Some(TokenKind::Else)
                | Some(TokenKind::Esac)
                | Some(TokenKind::In)
        )
    }

    /// Skip newlines and comments
    fn skip_newlines_and_comments(&mut self) {
        while matches!(
            self.peek_kind(),
            Some(TokenKind::Newline) | Some(TokenKind::Comment)
        ) {
            self.advance();
        }
    }

    /// Check if current token matches the given kind
    fn check(&self, kind: TokenKind) -> bool {
        self.peek_kind() == Some(kind)
    }

    /// Expect a specific token kind, consume it, or error
    fn expect(&mut self, kind: TokenKind) -> Result<Token> {
        if let Some(token) = self.peek() {
            if token.kind == kind {
                return Ok(self.advance().unwrap().clone());
            } else {
                return Err(ParseError::ExpectedToken {
                    expected: kind,
                    found: token.kind,
                });
            }
        }
        Err(ParseError::UnexpectedEof)
    }

    /// Get current token without consuming
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    /// Get current token kind
    fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind)
    }

    /// Peek ahead n tokens
    fn peek_at(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.pos + n)
    }

    /// Advance to next token
    fn advance(&mut self) -> Option<&Token> {
        if self.pos < self.tokens.len() {
            let token = &self.tokens[self.pos];
            self.pos += 1;
            Some(token)
        } else {
            None
        }
    }

    /// Check if we're at EOF
    fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Eof) | None)
    }

    /// Get current position
    fn position(&self) -> Position {
        if let Some(token) = self.peek() {
            token.span.start
        } else if let Some(last) = self.tokens.last() {
            last.span.end
        } else {
            Position::zero()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lexer, ShellOptions};

    fn parse(input: &str) -> Result<Program> {
        let mut lexer = Lexer::new(input, ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse()
    }

    #[test]
    fn test_parse_empty() {
        let prog = parse("").unwrap();
        assert_eq!(prog.statements.len(), 0);
    }

    #[test]
    fn test_parse_simple_command() {
        let prog = parse("echo hello").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::SimpleCommand(cmd) => {
                assert_eq!(cmd.args.len(), 1);
            }
            _ => panic!("Expected SimpleCommand"),
        }
    }

    #[test]
    fn test_parse_pipeline() {
        let prog = parse("echo hello | grep h").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::Pipeline(pipeline) => {
                assert_eq!(pipeline.elements.len(), 2);
                assert_eq!(pipeline.elements[0].pipe_op, Some(PipeOp::Pipe));
            }
            _ => panic!("Expected Pipeline"),
        }
    }

    #[test]
    fn test_parse_command_list() {
        let prog = parse("echo hello && echo world").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::List(list) => {
                assert_eq!(list.rest.len(), 1);
                assert_eq!(list.rest[0].0, ListOp::And);
            }
            _ => panic!("Expected CommandList"),
        }
    }

    #[test]
    fn test_parse_multiple_statements() {
        let prog = parse("echo one\necho two\necho three").unwrap();
        assert_eq!(prog.statements.len(), 3);

        for stmt in &prog.statements {
            assert!(matches!(stmt, Statement::SimpleCommand(_)));
        }
    }

    #[test]
    fn test_parse_subshell() {
        let prog = parse("(echo hello)").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::Subshell(subshell) => {
                assert_eq!(subshell.body.len(), 1);
            }
            _ => panic!("Expected Subshell"),
        }
    }

    #[test]
    fn test_parse_if_simple() {
        let prog = parse("if true; then echo yes; fi").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::If(if_stmt) => {
                assert_eq!(if_stmt.then_body.len(), 1);
                assert_eq!(if_stmt.elif_branches.len(), 0);
                assert!(if_stmt.else_body.is_none());
            }
            _ => panic!("Expected If statement"),
        }
    }

    #[test]
    fn test_parse_if_elif_else() {
        let input = "if test -f foo; then\n  echo found\nelif test -d foo; then\n  echo dir\nelse\n  echo nothing\nfi";
        let prog = parse(input).unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::If(if_stmt) => {
                assert_eq!(if_stmt.then_body.len(), 1);
                assert_eq!(if_stmt.elif_branches.len(), 1);
                assert!(if_stmt.else_body.is_some());
                assert_eq!(if_stmt.else_body.as_ref().unwrap().len(), 1);
            }
            _ => panic!("Expected If statement"),
        }
    }

    #[test]
    fn test_parse_while() {
        let prog = parse("while true; do echo loop; done").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::While(while_stmt) => {
                assert_eq!(while_stmt.body.len(), 1);
            }
            _ => panic!("Expected While statement"),
        }
    }

    #[test]
    fn test_parse_until() {
        let prog = parse("until false; do echo loop; done").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::Until(until_stmt) => {
                assert_eq!(until_stmt.body.len(), 1);
            }
            _ => panic!("Expected Until statement"),
        }
    }

    #[test]
    fn test_parse_for_with_in() {
        let prog = parse("for x in a b c; do echo $x; done").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::For(for_stmt) => {
                assert_eq!(for_stmt.var, "x");
                assert_eq!(for_stmt.words.len(), 3);
                assert_eq!(for_stmt.body.len(), 1);
            }
            _ => panic!("Expected For statement"),
        }
    }

    #[test]
    fn test_parse_for_without_in() {
        let prog = parse("for x; do echo $x; done").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::For(for_stmt) => {
                assert_eq!(for_stmt.var, "x");
                assert_eq!(for_stmt.words.len(), 0); // No words = default to $@
                assert_eq!(for_stmt.body.len(), 1);
            }
            _ => panic!("Expected For statement"),
        }
    }

    #[test]
    fn test_parse_case() {
        let input = "case $x in\n  a) echo A ;;\n  b|c) echo BC ;;\n  *) echo other ;;\nesac";
        let prog = parse(input).unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::Case(case_stmt) => {
                assert_eq!(case_stmt.arms.len(), 3);
                assert_eq!(case_stmt.arms[0].patterns.len(), 1); // a
                assert_eq!(case_stmt.arms[1].patterns.len(), 2); // b|c
                assert_eq!(case_stmt.arms[2].patterns.len(), 1); // *
            }
            _ => panic!("Expected Case statement"),
        }
    }

    #[test]
    fn test_parse_function_posix() {
        let prog = parse("greet() { echo hello; }").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::FunctionDef(func) => {
                assert_eq!(func.name, "greet");
                assert_eq!(func.body.len(), 1);
            }
            _ => panic!("Expected FunctionDef"),
        }
    }

    #[test]
    fn test_parse_function_keyword() {
        let prog = parse("function greet { echo hello; }").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::FunctionDef(func) => {
                assert_eq!(func.name, "greet");
                assert_eq!(func.body.len(), 1);
            }
            _ => panic!("Expected FunctionDef"),
        }
    }

    #[test]
    fn test_parse_function_keyword_with_parens() {
        let prog = parse("function greet() { echo hello; }").unwrap();
        assert_eq!(prog.statements.len(), 1);

        match &prog.statements[0] {
            Statement::FunctionDef(func) => {
                assert_eq!(func.name, "greet");
                assert_eq!(func.body.len(), 1);
            }
            _ => panic!("Expected FunctionDef"),
        }
    }
}
