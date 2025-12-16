//! Lexer with mode stack for context-sensitive tokenization
//!
//! The lexer handles shell quoting rules by maintaining a stack of lexing modes.
//! This allows proper handling of nested contexts like double quotes containing
//! command substitutions.

use crate::{
    span::{Position, Span},
    token::{InterpolatedPart, Token, TokenKind, TokenValue},
    LexError, ShellOptions,
};

/// Lexical analysis mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexerMode {
    /// Shell command mode: barewords are strings
    Command,
    /// Expression mode: barewords are identifiers
    Expression,
    /// Inside here-document
    HereDoc,
    /// Inside regex literal
    Regex,
    /// Inside arithmetic expansion
    Arithmetic,
}

/// Pending heredoc that needs to be read after newline
#[derive(Debug, Clone)]
struct PendingHeredoc {
    delimiter: String,
    strip_tabs: bool,
}

/// Lexer that tokenizes shell input
pub struct Lexer<'src> {
    source: &'src str,
    chars: std::iter::Peekable<std::str::CharIndices<'src>>,
    position: Position,
    mode: LexerMode,
    mode_stack: Vec<LexerMode>,
    options: ShellOptions,
    pending_heredocs: Vec<PendingHeredoc>,
    /// If Some, next word token is a heredoc delimiter
    next_word_is_heredoc_delimiter: Option<bool>, // true = strip_tabs (<<-), false = normal (<<)
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given input
    pub fn new(source: &'src str, options: ShellOptions) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            position: Position::zero(),
            mode: LexerMode::Command,
            mode_stack: Vec::new(),
            options,
            pending_heredocs: Vec::new(),
            next_word_is_heredoc_delimiter: None,
        }
    }

    /// Tokenize the entire input
    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    /// Push a new mode onto the stack
    pub fn push_mode(&mut self, mode: LexerMode) {
        self.mode_stack.push(self.mode);
        self.mode = mode;
    }

    /// Pop mode from the stack
    pub fn pop_mode(&mut self) {
        if let Some(mode) = self.mode_stack.pop() {
            self.mode = mode;
        }
    }

    /// Get the next token
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        // Check if we need to read heredoc content
        // This happens after a newline when we have pending heredocs
        if !self.pending_heredocs.is_empty() && self.position.column == 1 {
            // We're at the start of a line and have heredocs to read
            let heredoc = self.pending_heredocs.remove(0);
            return self.read_heredoc_content(&heredoc.delimiter, heredoc.strip_tabs);
        }

        self.skip_whitespace_and_comments();

        let start = self.position;

        // Check for EOF
        if self.peek().is_none() {
            return Ok(Token::simple(TokenKind::Eof, Span::point(start)));
        }

        match self.mode {
            LexerMode::Command => self.lex_command_mode(),
            LexerMode::Expression => self.lex_expression_mode(),
            LexerMode::HereDoc => self.lex_heredoc(),
            LexerMode::Regex => self.lex_regex(),
            LexerMode::Arithmetic => self.lex_arithmetic(),
        }
    }

    /// Lex in command mode (barewords are strings)
    fn lex_command_mode(&mut self) -> Result<Token, LexError> {
        let start = self.position;

        match self.peek() {
            Some('\n') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::Newline,
                    Span::new(start, self.position),
                ))
            }
            Some('#') => self.lex_comment(),
            Some('\'') => self.lex_single_quoted(),
            Some('"') => self.lex_double_quoted(),
            Some('|') => self.lex_pipe(),
            Some('&') => self.lex_amp(),
            Some(';') => self.lex_semi(),
            Some('<') => self.lex_less_than(),
            Some('>') => self.lex_greater_than(),
            Some('(') => {
                self.advance();
                Ok(Token::simple(TokenKind::LParen, Span::new(start, self.position)))
            }
            Some(')') => {
                self.advance();
                Ok(Token::simple(TokenKind::RParen, Span::new(start, self.position)))
            }
            Some('{') => {
                self.advance();
                Ok(Token::simple(TokenKind::LBrace, Span::new(start, self.position)))
            }
            Some('}') => {
                self.advance();
                Ok(Token::simple(TokenKind::RBrace, Span::new(start, self.position)))
            }
            Some('[') => self.lex_bracket(),
            Some('$') => self.lex_dollar(),
            Some('@') if self.options.parse_at => self.lex_at(),
            Some('-') if self.peek_at(1) == Some('>') && self.options.parse_arrow => {
                self.advance(); // -
                self.advance(); // >
                Ok(Token::simple(
                    TokenKind::ArrowPipe,
                    Span::new(start, self.position),
                ))
            }
            Some('=') if self.peek_at(1) == Some('>') => {
                self.advance(); // =
                self.advance(); // >
                Ok(Token::simple(
                    TokenKind::FatArrowPipe,
                    Span::new(start, self.position),
                ))
            }
            Some('~') if self.peek_at(1) == Some('>') => {
                self.advance(); // ~
                self.advance(); // >
                Ok(Token::simple(
                    TokenKind::AlterPipe,
                    Span::new(start, self.position),
                ))
            }
            Some(c) if is_word_start(c) => self.lex_word(),
            Some(c) => Err(LexError::UnexpectedChar(c, start)),
            None => Ok(Token::simple(TokenKind::Eof, Span::point(start))),
        }
    }

    /// Lex in expression mode (barewords are identifiers)
    fn lex_expression_mode(&mut self) -> Result<Token, LexError> {
        let _start = self.position;

        match self.peek() {
            Some(c) if c.is_ascii_digit() => self.lex_number(),
            Some(c) if is_ident_start(c) => self.lex_identifier(),
            Some('\'') => self.lex_single_quoted(),
            Some('"') => self.lex_double_quoted(),
            _ => self.lex_command_mode(), // Fall back to command mode for operators
        }
    }

    /// Read heredoc content until delimiter is found
    /// This is called eagerly after the newline following the command with <<
    fn read_heredoc_content(&mut self, delimiter: &str, strip_tabs: bool) -> Result<Token, LexError> {
        let start = self.position;
        let mut content = String::new();

        loop {
            // Read a line
            let mut line = String::new();

            while let Some(c) = self.peek() {
                if c == '\n' {
                    self.advance(); // consume newline
                    break;
                }
                line.push(c);
                self.advance();
            }

            // Check if this line is the delimiter
            let check_line = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                &line
            };

            if check_line == delimiter {
                // Found the delimiter, we're done
                break;
            }

            // Not the delimiter, add to content
            content.push_str(&line);
            content.push('\n');

            // Check for EOF before delimiter
            if self.peek().is_none() {
                return Err(LexError::UnterminatedString(start));
            }
        }

        // Remove trailing newline if present
        if content.ends_with('\n') {
            content.pop();
        }

        Ok(Token::new(
            TokenKind::Word, // Heredoc content is treated as a word token
            Span::new(start, self.position),
            TokenValue::String(content),
        ))
    }

    /// Lex here-document content (old stub - now unused)
    fn lex_heredoc(&mut self) -> Result<Token, LexError> {
        // This mode is no longer used - heredocs are read eagerly
        Err(LexError::UnexpectedEof)
    }

    /// Lex regex literal
    fn lex_regex(&mut self) -> Result<Token, LexError> {
        // TODO: Implement regex lexing
        Err(LexError::UnexpectedEof)
    }

    /// Lex arithmetic expression
    fn lex_arithmetic(&mut self) -> Result<Token, LexError> {
        let start = self.position;

        // Arithmetic mode is essentially expression mode with all operators enabled
        match self.peek() {
            Some(c) if c.is_ascii_digit() => self.lex_number(),
            Some(c) if is_ident_start(c) => self.lex_identifier(),
            Some('+') => {
                self.advance();
                match self.peek() {
                    Some('+') => {
                        self.advance();
                        // ++ not supported in POSIX arithmetic, treat as two + operators
                        // Back up one char
                        self.position = start;
                        self.advance();
                        Ok(Token::simple(TokenKind::Plus, Span::new(start, self.position)))
                    }
                    Some('=') => {
                        self.advance();
                        Ok(Token::simple(
                            TokenKind::PlusAssign,
                            Span::new(start, self.position),
                        ))
                    }
                    _ => Ok(Token::simple(TokenKind::Plus, Span::new(start, self.position))),
                }
            }
            Some('-') => {
                self.advance();
                match self.peek() {
                    Some('=') => {
                        self.advance();
                        Ok(Token::simple(
                            TokenKind::MinusAssign,
                            Span::new(start, self.position),
                        ))
                    }
                    _ => Ok(Token::simple(TokenKind::Minus, Span::new(start, self.position))),
                }
            }
            Some('*') => {
                self.advance();
                match self.peek() {
                    Some('*') => {
                        self.advance();
                        Ok(Token::simple(
                            TokenKind::StarStar,
                            Span::new(start, self.position),
                        ))
                    }
                    Some('=') => {
                        self.advance();
                        Ok(Token::simple(
                            TokenKind::StarAssign,
                            Span::new(start, self.position),
                        ))
                    }
                    _ => Ok(Token::simple(TokenKind::Star, Span::new(start, self.position))),
                }
            }
            Some('/') => {
                self.advance();
                match self.peek() {
                    Some('=') => {
                        self.advance();
                        Ok(Token::simple(
                            TokenKind::SlashAssign,
                            Span::new(start, self.position),
                        ))
                    }
                    _ => Ok(Token::simple(TokenKind::Slash, Span::new(start, self.position))),
                }
            }
            Some('%') => {
                self.advance();
                Ok(Token::simple(TokenKind::Percent, Span::new(start, self.position)))
            }
            Some('(') => {
                self.advance();
                Ok(Token::simple(TokenKind::LParen, Span::new(start, self.position)))
            }
            Some(')') => {
                self.advance();
                Ok(Token::simple(TokenKind::RParen, Span::new(start, self.position)))
            }
            Some('=') => {
                self.advance();
                match self.peek() {
                    Some('=') => {
                        self.advance();
                        Ok(Token::simple(TokenKind::Eq, Span::new(start, self.position)))
                    }
                    _ => Ok(Token::simple(TokenKind::Assign, Span::new(start, self.position))),
                }
            }
            Some('!') => {
                self.advance();
                match self.peek() {
                    Some('=') => {
                        self.advance();
                        Ok(Token::simple(TokenKind::Ne, Span::new(start, self.position)))
                    }
                    _ => Ok(Token::simple(TokenKind::Bang, Span::new(start, self.position))),
                }
            }
            Some('<') => self.lex_less_than(),
            Some('>') => self.lex_greater_than(),
            Some('&') => self.lex_amp(),
            Some('|') => self.lex_pipe(),
            Some('$') => self.lex_dollar(),
            _ => Err(LexError::UnexpectedChar(
                self.peek().unwrap_or('\0'),
                start,
            )),
        }
    }

    /// Skip whitespace and comments
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() && c != '\n' => {
                    self.advance();
                }
                _ => break,
            }
        }
    }

    /// Lex a comment
    fn lex_comment(&mut self) -> Result<Token, LexError> {
        debug_assert!(self.peek() == Some('#'));

        let start = self.position;
        self.advance(); // consume '#'

        let content_start = self.position.offset;
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }

        let content = &self.source[content_start..self.position.offset];

        Ok(Token::new(
            TokenKind::Comment,
            Span::new(start, self.position),
            TokenValue::String(content.to_string()),
        ))
    }

    /// Lex a single-quoted string
    fn lex_single_quoted(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume opening '

        let mut content = String::new();

        loop {
            match self.peek() {
                Some('\'') => {
                    self.advance();
                    // Check for escaped quote ''
                    if self.peek() == Some('\'') {
                        content.push('\'');
                        self.advance();
                    } else {
                        break;
                    }
                }
                Some(c) => {
                    content.push(c);
                    self.advance();
                }
                None => {
                    return Err(LexError::UnterminatedString(start));
                }
            }
        }

        Ok(Token::new(
            TokenKind::SingleQuoted,
            Span::new(start, self.position),
            TokenValue::String(content),
        ))
    }

    /// Lex a double-quoted string (with interpolation)
    fn lex_double_quoted(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume opening "

        let mut parts = Vec::new();
        let mut current_literal = String::new();

        loop {
            match self.peek() {
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('n') => {
                            current_literal.push('\n');
                            self.advance();
                        }
                        Some('t') => {
                            current_literal.push('\t');
                            self.advance();
                        }
                        Some('r') => {
                            current_literal.push('\r');
                            self.advance();
                        }
                        Some('\\') => {
                            current_literal.push('\\');
                            self.advance();
                        }
                        Some('"') => {
                            current_literal.push('"');
                            self.advance();
                        }
                        Some('$') => {
                            current_literal.push('$');
                            self.advance();
                        }
                        Some(c) => {
                            current_literal.push('\\');
                            current_literal.push(c);
                            self.advance();
                        }
                        None => {
                            return Err(LexError::UnterminatedString(start));
                        }
                    }
                }
                Some('$') => {
                    // Save current literal
                    if !current_literal.is_empty() {
                        parts.push(InterpolatedPart::Literal(std::mem::take(
                            &mut current_literal,
                        )));
                    }

                    self.advance(); // consume $

                    match self.peek() {
                        Some('(') => {
                            // Command substitution $(...)
                            self.advance(); // consume (
                            let cmd_tokens = self.lex_command_substitution()?;
                            parts.push(InterpolatedPart::Command(cmd_tokens));
                        }
                        Some('{') => {
                            // Parameter expansion ${...}
                            // For now, just read the variable name
                            self.advance(); // consume {
                            let mut var_name = String::new();
                            while let Some(c) = self.peek() {
                                if c == '}' {
                                    self.advance(); // consume }
                                    break;
                                }
                                var_name.push(c);
                                self.advance();
                            }
                            parts.push(InterpolatedPart::Variable(var_name));
                        }
                        Some(c) if is_ident_start(c) => {
                            // Simple variable $var
                            let mut var_name = String::new();
                            while let Some(c) = self.peek() {
                                if is_ident_continue(c) {
                                    var_name.push(c);
                                    self.advance();
                                } else {
                                    break;
                                }
                            }
                            parts.push(InterpolatedPart::Variable(var_name));
                        }
                        _ => {
                            // Just a literal $
                            current_literal.push('$');
                        }
                    }
                }
                Some(c) => {
                    current_literal.push(c);
                    self.advance();
                }
                None => {
                    return Err(LexError::UnterminatedString(start));
                }
            }
        }

        // Save final literal
        if !current_literal.is_empty() {
            parts.push(InterpolatedPart::Literal(current_literal));
        }

        Ok(Token::new(
            TokenKind::DoubleQuoted,
            Span::new(start, self.position),
            TokenValue::Interpolated(parts),
        ))
    }

    /// Lex a word (bareword in command mode)
    fn lex_word(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        let mut word = String::new();

        while let Some(c) = self.peek() {
            if is_word_continue(c) {
                word.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // Check if this word is a heredoc delimiter
        if let Some(strip_tabs) = self.next_word_is_heredoc_delimiter.take() {
            // Store this delimiter for processing after newline
            self.pending_heredocs.push(PendingHeredoc {
                delimiter: word.clone(),
                strip_tabs,
            });
        }

        let kind = self.classify_word(&word);

        Ok(Token::new(
            kind,
            Span::new(start, self.position),
            TokenValue::String(word),
        ))
    }

    /// Lex an identifier (bareword in expression mode)
    fn lex_identifier(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        let mut ident = String::new();

        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                ident.push(c);
                self.advance();
            } else {
                break;
            }
        }

        let kind = self.classify_word(&ident);

        Ok(Token::new(
            kind,
            Span::new(start, self.position),
            TokenValue::String(ident),
        ))
    }

    /// Classify a word as keyword or Word/Identifier
    fn classify_word(&self, text: &str) -> TokenKind {
        // POSIX keywords (always recognized)
        match text {
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "else" => TokenKind::Else,
            "elif" => TokenKind::Elif,
            "fi" => TokenKind::Fi,
            "case" => TokenKind::Case,
            "esac" => TokenKind::Esac,
            "in" => TokenKind::In,
            "for" => TokenKind::For,
            "while" => TokenKind::While,
            "until" => TokenKind::Until,
            "do" => TokenKind::Do,
            "done" => TokenKind::Done,
            "function" => TokenKind::Function,
            _ => {
                // Colony keywords (if enabled)
                if self.mode == LexerMode::Expression || self.options.parse_proc {
                    match text {
                        "var" => TokenKind::Var,
                        "setvar" => TokenKind::Setvar,
                        "const" => TokenKind::Const,
                        "proc" if self.options.parse_proc => TokenKind::Proc,
                        "func" if self.options.parse_func => TokenKind::Func,
                        "agent" if self.options.parse_agent => TokenKind::Agent,
                        "try" => TokenKind::Try,
                        "catch" => TokenKind::Catch,
                        "finally" => TokenKind::Finally,
                        "match" => TokenKind::Match,
                        "return" => TokenKind::Return,
                        "and" => TokenKind::And,
                        "or" => TokenKind::Or,
                        "not" => TokenKind::Not,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "null" => TokenKind::Null,
                        _ => {
                            if self.mode == LexerMode::Expression {
                                TokenKind::Identifier
                            } else {
                                TokenKind::Word
                            }
                        }
                    }
                } else if self.mode == LexerMode::Expression {
                    TokenKind::Identifier
                } else {
                    TokenKind::Word
                }
            }
        }
    }

    /// Lex a number literal
    fn lex_number(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        let mut num_str = String::new();
        let mut is_float = false;

        // Integer part
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                if c != '_' {
                    num_str.push(c);
                }
                self.advance();
            } else {
                break;
            }
        }

        // Decimal part
        if self.peek() == Some('.')
            && self
                .peek_at(1)
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            is_float = true;
            num_str.push('.');
            self.advance();

            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '_' {
                    if c != '_' {
                        num_str.push(c);
                    }
                    self.advance();
                } else {
                    break;
                }
            }
        }

        if is_float {
            let value: f64 = num_str
                .parse()
                .map_err(|_| LexError::InvalidNumber(start))?;
            Ok(Token::new(
                TokenKind::Float,
                Span::new(start, self.position),
                TokenValue::Float(value),
            ))
        } else {
            let value: i64 = num_str
                .parse()
                .map_err(|_| LexError::InvalidNumber(start))?;
            Ok(Token::new(
                TokenKind::Integer,
                Span::new(start, self.position),
                TokenValue::Int(value),
            ))
        }
    }

    /// Lex pipe operator
    fn lex_pipe(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume |

        match self.peek() {
            Some('|') => {
                self.advance();
                Ok(Token::simple(TokenKind::Or2, Span::new(start, self.position)))
            }
            Some('&') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::ErrPipe,
                    Span::new(start, self.position),
                ))
            }
            _ => Ok(Token::simple(TokenKind::Pipe, Span::new(start, self.position))),
        }
    }

    /// Lex ampersand
    fn lex_amp(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume &

        match self.peek() {
            Some('&') => {
                self.advance();
                Ok(Token::simple(TokenKind::And2, Span::new(start, self.position)))
            }
            _ => Ok(Token::simple(TokenKind::Amp, Span::new(start, self.position))),
        }
    }

    /// Lex semicolon
    fn lex_semi(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume ;

        match self.peek() {
            Some(';') => {
                self.advance();
                match self.peek() {
                    Some('&') => {
                        self.advance();
                        Ok(Token::simple(
                            TokenKind::SemiSemiAmp,
                            Span::new(start, self.position),
                        ))
                    }
                    _ => Ok(Token::simple(
                        TokenKind::SemiSemi,
                        Span::new(start, self.position),
                    )),
                }
            }
            Some('&') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::SemiAmp,
                    Span::new(start, self.position),
                ))
            }
            _ => Ok(Token::simple(TokenKind::Semi, Span::new(start, self.position))),
        }
    }

    /// Lex less-than operator
    fn lex_less_than(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume <

        match self.peek() {
            Some('<') => {
                self.advance();
                match self.peek() {
                    Some('<') => {
                        self.advance();
                        Ok(Token::simple(
                            TokenKind::HereString,
                            Span::new(start, self.position),
                        ))
                    }
                    Some('-') => {
                        self.advance();
                        // Next word is heredoc delimiter with tab stripping
                        self.next_word_is_heredoc_delimiter = Some(true);
                        Ok(Token::simple(
                            TokenKind::HereDocStrip,
                            Span::new(start, self.position),
                        ))
                    }
                    _ => {
                        // Next word is heredoc delimiter
                        self.next_word_is_heredoc_delimiter = Some(false);
                        Ok(Token::simple(
                            TokenKind::HereDoc,
                            Span::new(start, self.position),
                        ))
                    }
                }
            }
            Some('(') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::ProcessSubIn,
                    Span::new(start, self.position),
                ))
            }
            Some('>') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::RedirectInOut,
                    Span::new(start, self.position),
                ))
            }
            Some('=') => {
                self.advance();
                Ok(Token::simple(TokenKind::Le, Span::new(start, self.position)))
            }
            _ => Ok(Token::simple(TokenKind::Lt, Span::new(start, self.position))),
        }
    }

    /// Lex greater-than operator
    fn lex_greater_than(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume >

        match self.peek() {
            Some('>') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::RedirectAppend,
                    Span::new(start, self.position),
                ))
            }
            Some('(') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::ProcessSubOut,
                    Span::new(start, self.position),
                ))
            }
            Some('=') => {
                self.advance();
                Ok(Token::simple(TokenKind::Ge, Span::new(start, self.position)))
            }
            _ => Ok(Token::simple(TokenKind::Gt, Span::new(start, self.position))),
        }
    }

    /// Lex bracket
    fn lex_bracket(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume [

        match self.peek() {
            Some('[') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::LDoubleBracket,
                    Span::new(start, self.position),
                ))
            }
            _ => Ok(Token::simple(
                TokenKind::LBracket,
                Span::new(start, self.position),
            )),
        }
    }

    /// Lex dollar expansions
    fn lex_dollar(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume $

        match self.peek() {
            Some('(') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::DollarParen,
                    Span::new(start, self.position),
                ))
            }
            Some('{') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::DollarBrace,
                    Span::new(start, self.position),
                ))
            }
            Some('[') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::DollarBracket,
                    Span::new(start, self.position),
                ))
            }
            _ => Ok(Token::simple(TokenKind::Dollar, Span::new(start, self.position))),
        }
    }

    /// Lex @ expansions
    fn lex_at(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        self.advance(); // consume @

        match self.peek() {
            Some('(') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::AtParen,
                    Span::new(start, self.position),
                ))
            }
            Some('{') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::AtBrace,
                    Span::new(start, self.position),
                ))
            }
            Some('[') => {
                self.advance();
                Ok(Token::simple(
                    TokenKind::AtBracket,
                    Span::new(start, self.position),
                ))
            }
            _ => Ok(Token::simple(TokenKind::At, Span::new(start, self.position))),
        }
    }

    /// Lex command substitution $(...)
    /// Returns tokens for the command inside the substitution
    fn lex_command_substitution(&mut self) -> Result<Vec<Token>, LexError> {
        // TODO: Implement command substitution lexing
        // Need to:
        // 1. Track nesting depth for nested $()
        // 2. Collect tokens until matching )
        // 3. Handle strings and escapes that might contain )

        let mut tokens = Vec::new();
        let mut nesting = 1; // We've already consumed the opening (

        // Push command mode onto stack
        self.push_mode(LexerMode::Command);

        while nesting > 0 {
            let token = self.next_token()?;

            match token.kind {
                TokenKind::LParen | TokenKind::DollarParen => {
                    nesting += 1;
                    tokens.push(token);
                }
                TokenKind::RParen => {
                    nesting -= 1;
                    if nesting > 0 {
                        tokens.push(token);
                    }
                    // Don't include the final closing paren
                }
                TokenKind::Eof => {
                    return Err(LexError::UnterminatedString(token.span.start));
                }
                _ => {
                    tokens.push(token);
                }
            }
        }

        // Pop command mode from stack
        self.pop_mode();

        Ok(tokens)
    }

    /// Peek at current character without consuming
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    /// Peek ahead n characters
    fn peek_at(&mut self, n: usize) -> Option<char> {
        let mut iter = self.chars.clone();
        for _ in 0..n {
            iter.next();
        }
        iter.peek().map(|(_, c)| *c)
    }

    /// Advance to next character
    fn advance(&mut self) -> Option<char> {
        if let Some((offset, c)) = self.chars.next() {
            self.position.offset = offset;

            if c == '\n' {
                self.position.line += 1;
                self.position.column = 1;
            } else {
                self.position.column += 1;
            }

            // Move to next character's position
            if let Some((next_offset, _)) = self.chars.peek() {
                self.position.offset = *next_offset;
            } else {
                self.position.offset = self.source.len();
            }

            Some(c)
        } else {
            None
        }
    }
}

/// Check if character can start a word
fn is_word_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '/' || c == '.' || c == '-' || c == '~'
}

/// Check if character can continue a word
fn is_word_continue(c: char) -> bool {
    !c.is_whitespace()
        && c != '|'
        && c != '&'
        && c != ';'
        && c != '<'
        && c != '>'
        && c != '('
        && c != ')'
        && c != '{'
        && c != '}'
        && c != '$'
        && c != '"'
        && c != '\''
}

/// Check if character can start an identifier
fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// Check if character can continue an identifier
fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_empty() {
        let mut lexer = Lexer::new("", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn test_lexer_simple_command() {
        let mut lexer = Lexer::new("echo hello", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, TokenKind::Word);
        assert_eq!(tokens[1].kind, TokenKind::Word);
        assert_eq!(tokens[2].kind, TokenKind::Eof);
    }

    #[test]
    fn test_lexer_pipeline() {
        let mut lexer = Lexer::new("echo hello | grep h", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 6);
        assert_eq!(tokens[0].kind, TokenKind::Word);
        assert_eq!(tokens[1].kind, TokenKind::Word);
        assert_eq!(tokens[2].kind, TokenKind::Pipe);
        assert_eq!(tokens[3].kind, TokenKind::Word);
        assert_eq!(tokens[4].kind, TokenKind::Word);
        assert_eq!(tokens[5].kind, TokenKind::Eof);
    }

    #[test]
    fn test_lexer_strings() {
        let mut lexer = Lexer::new("'single' \"double\"", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, TokenKind::SingleQuoted);
        assert_eq!(tokens[1].kind, TokenKind::DoubleQuoted);
        assert_eq!(tokens[2].kind, TokenKind::Eof);
    }

    #[test]
    fn test_lexer_operators() {
        let mut lexer = Lexer::new("&& || ; &", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0].kind, TokenKind::And2);
        assert_eq!(tokens[1].kind, TokenKind::Or2);
        assert_eq!(tokens[2].kind, TokenKind::Semi);
        assert_eq!(tokens[3].kind, TokenKind::Amp);
        assert_eq!(tokens[4].kind, TokenKind::Eof);
    }

    #[test]
    fn test_lexer_comment() {
        let mut lexer = Lexer::new("# this is a comment\necho", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        // Comments are skipped during tokenization (handled in skip_whitespace_and_comments)
        // But our current implementation returns them - let's verify
        assert_eq!(tokens[0].kind, TokenKind::Comment);
        assert_eq!(tokens[1].kind, TokenKind::Newline);
        assert_eq!(tokens[2].kind, TokenKind::Word);
    }

    #[test]
    fn test_lexer_variable_interpolation() {
        let mut lexer = Lexer::new("\"hello $name\"", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::DoubleQuoted);

        // Check that we have interpolation
        match &tokens[0].value {
            TokenValue::Interpolated(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(&parts[0], InterpolatedPart::Literal(s) if s == "hello "));
                assert!(matches!(&parts[1], InterpolatedPart::Variable(s) if s == "name"));
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_lexer_braced_variable() {
        let mut lexer = Lexer::new("\"hello ${name}!\"", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::DoubleQuoted);

        match &tokens[0].value {
            TokenValue::Interpolated(parts) => {
                assert_eq!(parts.len(), 3);
                assert!(matches!(&parts[0], InterpolatedPart::Literal(s) if s == "hello "));
                assert!(matches!(&parts[1], InterpolatedPart::Variable(s) if s == "name"));
                assert!(matches!(&parts[2], InterpolatedPart::Literal(s) if s == "!"));
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_lexer_command_substitution() {
        let mut lexer = Lexer::new("\"output: $(whoami)\"", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::DoubleQuoted);

        match &tokens[0].value {
            TokenValue::Interpolated(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(&parts[0], InterpolatedPart::Literal(s) if s == "output: "));
                match &parts[1] {
                    InterpolatedPart::Command(cmd_tokens) => {
                        assert_eq!(cmd_tokens.len(), 1);
                        assert_eq!(cmd_tokens[0].kind, TokenKind::Word);
                    }
                    _ => panic!("Expected command substitution"),
                }
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_lexer_nested_command_substitution() {
        let mut lexer = Lexer::new("$(echo $(whoami))", ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();
        // Should have: $( echo $( whoami ) )
        assert_eq!(tokens[0].kind, TokenKind::DollarParen);
        assert_eq!(tokens[1].kind, TokenKind::Word); // echo
        assert_eq!(tokens[2].kind, TokenKind::DollarParen);
        assert_eq!(tokens[3].kind, TokenKind::Word); // whoami
        assert_eq!(tokens[4].kind, TokenKind::RParen);
        assert_eq!(tokens[5].kind, TokenKind::RParen);
    }

    #[test]
    fn test_lexer_heredoc() {
        let input = "cat <<EOF\nline 1\nline 2\nEOF\n";
        let mut lexer = Lexer::new(input, ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();

        // Should have: cat, <<, EOF (delimiter), newline, heredoc_content, newline, EOF (token)
        assert_eq!(tokens[0].kind, TokenKind::Word); // cat
        assert_eq!(tokens[1].kind, TokenKind::HereDoc); // <<
        assert_eq!(tokens[2].kind, TokenKind::Word); // EOF delimiter
        assert_eq!(tokens[3].kind, TokenKind::Newline);
        assert_eq!(tokens[4].kind, TokenKind::Word); // heredoc content

        // Check content
        match &tokens[4].value {
            TokenValue::String(s) => {
                assert_eq!(s, "line 1\nline 2");
            }
            _ => panic!("Expected string value for heredoc content"),
        }
    }

    #[test]
    fn test_lexer_heredoc_strip_tabs() {
        let input = "cat <<-EOF\n\t\tline 1\n\t\tline 2\n\t\tEOF\n";
        let mut lexer = Lexer::new(input, ShellOptions::posix());
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::Word); // cat
        assert_eq!(tokens[1].kind, TokenKind::HereDocStrip); // <<-
        assert_eq!(tokens[2].kind, TokenKind::Word); // EOF delimiter
        assert_eq!(tokens[3].kind, TokenKind::Newline);
        assert_eq!(tokens[4].kind, TokenKind::Word); // heredoc content

        // Content lines still have tabs, only delimiter matching strips them
        match &tokens[4].value {
            TokenValue::String(s) => {
                assert_eq!(s, "\t\tline 1\n\t\tline 2");
            }
            _ => panic!("Expected string value for heredoc content"),
        }
    }
}
