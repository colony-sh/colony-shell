# Part 2: Lexical Structure

## 2.1 Character Set

Colony source is UTF-8 encoded. The lexer operates on Unicode scalar values (not bytes).

```rust
pub struct Position {
    pub offset: usize,    // Byte offset in source
    pub line: u32,        // 1-indexed line number
    pub column: u32,      // 1-indexed column (Unicode scalars)
}

pub struct Span {
    pub start: Position,
    pub end: Position,
    pub source_id: Option<usize>,  // For multi-file tracking
}
```

## 2.2 Lexer Modes

The lexer operates in different modes that affect how tokens are interpreted:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LexerMode {
    /// Shell command mode: barewords are strings
    Command,
    /// Expression mode: barewords are identifiers
    Expression,
    /// Inside here-document
    HereDoc(HereDocState),
    /// Inside regex literal
    Regex,
    /// Inside arithmetic expansion
    Arithmetic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HereDocState {
    pub delimiter: &'static str,
    pub strip_tabs: bool,
    pub expand: bool,
}
```

## 2.3 Lexer Structure

```rust
pub struct Lexer<'src> {
    source: &'src str,
    chars: std::iter::Peekable<std::str::CharIndices<'src>>,
    position: Position,
    mode: LexerMode,
    mode_stack: Vec<LexerMode>,
    options: ShellOptions,
    pending_heredocs: Vec<HereDocState>,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str, options: ShellOptions) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            position: Position { offset: 0, line: 1, column: 1 },
            mode: LexerMode::Command,
            mode_stack: Vec::new(),
            options,
            pending_heredocs: Vec::new(),
        }
    }
    
    pub fn push_mode(&mut self, mode: LexerMode) {
        self.mode_stack.push(self.mode);
        self.mode = mode;
    }
    
    pub fn pop_mode(&mut self) {
        if let Some(mode) = self.mode_stack.pop() {
            self.mode = mode;
        }
    }
    
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace_and_comments();
        
        match self.mode {
            LexerMode::Command => self.lex_command_mode(),
            LexerMode::Expression => self.lex_expression_mode(),
            LexerMode::HereDoc(state) => self.lex_heredoc(state),
            LexerMode::Regex => self.lex_regex(),
            LexerMode::Arithmetic => self.lex_arithmetic(),
        }
    }
}
```

## 2.4 Token Types

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub value: TokenValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenValue {
    None,
    String(String),
    Int(i64),
    Float(f64),
    Interpolated(Vec<InterpolatedPart>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum InterpolatedPart {
    Literal(String),
    Variable(String),
    Command(Vec<Token>),
    Expression(Vec<Token>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // === Literals ===
    Word,               // Bareword in command mode
    Identifier,         // Identifier in expression mode
    SingleQuoted,       // '...'
    DoubleQuoted,       // "..." (with interpolation)
    RawString,          // r'...'
    TripleSingleQuoted, // '''...'''
    TripleDoubleQuoted, // """..."""
    Integer,            // 42, 0x2A, 0o52, 0b101010
    Float,              // 3.14, 1e10
    
    // === Keywords (POSIX) ===
    If,
    Then,
    Else,
    Elif,
    Fi,
    Case,
    Esac,
    In,
    For,
    While,
    Until,
    Do,
    Done,
    Function,
    
    // === Keywords (Colony) ===
    Var,
    Setvar,
    Const,
    Proc,
    Func,
    Agent,
    Try,
    Catch,
    Finally,
    Match,
    Return,
    And,        // Keyword 'and'
    Or,         // Keyword 'or'
    Not,        // Keyword 'not'
    True,
    False,
    Null,
    
    // === Pipeline Operators ===
    Pipe,           // |
    ArrowPipe,      // ->
    FatArrowPipe,   // =>
    AlterPipe,      // ~>
    ErrPipe,        // |&  or  2>&1 |
    
    // === List Operators ===
    And2,           // &&
    Or2,            // ||
    Semi,           // ;
    Amp,            // &
    SemiAmp,        // ;&  (case fallthrough)
    SemiSemi,       // ;;  (case terminator)
    SemiSemiAmp,    // ;;&
    
    // === Comparison ===
    Eq,             // ==
    Ne,             // !=
    Lt,             // <
    Le,             // <=
    Gt,             // >
    Ge,             // >=
    Tilde,          // ~ (pattern match)
    TildeEq,        // ~= or =~
    NotTilde,       // !~
    
    // === Arithmetic ===
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    SlashSlash,     // //  (integer division)
    Percent,        // %
    StarStar,       // **  (power)
    
    // === Bitwise ===
    BitAnd,         // & (in expression mode)
    BitOr,          // | (in expression mode)
    BitXor,         // ^
    BitNot,         // ~~ or ! (context-dependent)
    Shl,            // <<
    Shr,            // >>
    
    // === Assignment ===
    Assign,         // =
    PlusAssign,     // +=
    MinusAssign,    // -=
    StarAssign,     // *=
    SlashAssign,    // /=
    ColonAssign,    // :=
    QmarkAssign,    // ?=
    
    // === Delimiters ===
    LParen,         // (
    RParen,         // )
    LBrace,         // {
    RBrace,         // }
    LBracket,       // [
    RBracket,       // ]
    LDoubleBracket, // [[
    RDoubleBracket, // ]]
    
    // === Redirection ===
    RedirectIn,             // <
    RedirectOut,            // >
    RedirectAppend,         // >>
    RedirectClobber,        // >|
    RedirectInOut,          // <>
    RedirectFd,             // N> or N< (with fd number)
    RedirectDup,            // >&N, <&N
    RedirectClose,          // >&-, <&-
    HereDoc,                // <<
    HereDocStrip,           // <<-
    HereString,             // <<<
    ProcessSubIn,           // <(
    ProcessSubOut,          // >(
    
    // === Special ===
    Dollar,         // $
    At,             // @
    Bang,           // !
    Question,       // ?
    Colon,          // :
    Comma,          // ,
    Dot,            // .
    DotDot,         // ..
    DotDotLt,       // ..<
    DotDotEq,       // ..=
    Hash,           // #
    Backslash,      // \
    Backtick,       // `
    
    // === Compound Tokens ===
    DollarParen,    // $(
    DollarBrace,    // ${
    DollarBracket,  // $[
    DollarAt,       // $@
    DollarStar,     // $*
    DollarQmark,    // $?
    DollarBang,     // $!
    DollarDollar,   // $$
    DollarHash,     // $#
    AtParen,        // @(
    AtBrace,        // @{
    AtBracket,      // @[
    
    // === Structural ===
    Newline,
    Comment,
    Eof,
    
    // === Error Recovery ===
    Error,
}
```

## 2.5 Mode Switching Rules

The lexer switches between Command and Expression modes at specific syntactic positions:

### 2.5.1 Switch to Expression Mode

```rust
impl Lexer<'_> {
    fn should_switch_to_expression(&self, prev_token: &TokenKind) -> bool {
        matches!(prev_token,
            // After declaration keywords
            TokenKind::Var |
            TokenKind::Setvar |
            TokenKind::Const |
            
            // After assignment operator
            TokenKind::Assign |
            TokenKind::PlusAssign |
            TokenKind::MinusAssign |
            TokenKind::StarAssign |
            TokenKind::SlashAssign |
            
            // After opening paren in control structures
            // (if we're in: if/while/for/case + lparen context)
            
            // After comparison operators (RHS is expression)
            TokenKind::Eq |
            TokenKind::Ne |
            TokenKind::Lt |
            TokenKind::Le |
            TokenKind::Gt |
            TokenKind::Ge |
            
            // Inside $[ ]
            TokenKind::DollarBracket
        )
    }
}
```

### 2.5.2 Switch to Command Mode

```rust
impl Lexer<'_> {
    fn should_switch_to_command(&self, context: &ParseContext) -> bool {
        matches!(context,
            // Inside $( )
            ParseContext::CommandSubstitution |
            
            // Inside @( )
            ParseContext::SplitSubstitution |
            
            // After { in brace group
            ParseContext::BraceGroup |
            
            // Inside process substitution
            ParseContext::ProcessSubstitution |
            
            // Inside here-doc
            ParseContext::HereDoc(_)
        )
    }
}
```

### 2.5.3 Context-Dependent Mode

Some contexts determine mode based on surrounding syntax:

```colony
# Command mode (default)
echo hello world              # All barewords are strings

# Expression mode after keywords
var x = hello                 # 'hello' is identifier (error: undefined)
var x = "hello"               # Correct: string literal

# Expression mode inside $[ ]
echo "Value: $[x + 1]"        # x and 1 are expressions

# Command mode inside $( )
var output = $(ls -la)        # ls -la parsed as command

# Expression mode inside if ( )
if (x > 10) {                 # x > 10 is expression
    echo "big"
}

# Command mode in traditional if
if [ $x -gt 10 ]; then        # [ is command, $x -gt 10 are words
    echo "big"
fi
```

## 2.6 Comments

### 2.6.1 Single-Line Comments

```rust
fn lex_comment(&mut self) -> Token {
    debug_assert!(self.peek() == Some('#'));
    
    let start = self.position.clone();
    self.advance(); // consume '#'
    
    let content_start = self.position.offset;
    while let Some(c) = self.peek() {
        if c == '\n' {
            break;
        }
        self.advance();
    }
    
    let content = &self.source[content_start..self.position.offset];
    
    Token {
        kind: TokenKind::Comment,
        span: Span { start, end: self.position.clone(), source_id: None },
        value: TokenValue::String(content.to_string()),
    }
}
```

### 2.6.2 Multi-Line Comments (Colony extension)

```rust
fn lex_multiline_comment(&mut self) -> Result<Token, LexError> {
    // Must start with /#
    debug_assert!(self.peek() == Some('/'));
    
    let start = self.position.clone();
    self.advance(); // consume '/'
    self.advance(); // consume '#'
    
    let content_start = self.position.offset;
    let mut depth = 1;
    
    loop {
        match (self.peek(), self.peek_next()) {
            (Some('/'), Some('#')) => {
                depth += 1;
                self.advance();
                self.advance();
            }
            (Some('#'), Some('/')) => {
                depth -= 1;
                self.advance();
                self.advance();
                if depth == 0 {
                    break;
                }
            }
            (Some(_), _) => {
                self.advance();
            }
            (None, _) => {
                return Err(LexError::UnterminatedComment(start));
            }
        }
    }
    
    let content = &self.source[content_start..self.position.offset - 2];
    
    Ok(Token {
        kind: TokenKind::Comment,
        span: Span { start, end: self.position.clone(), source_id: None },
        value: TokenValue::String(content.to_string()),
    })
}
```

## 2.7 String Literals

### 2.7.1 Single-Quoted Strings

No interpolation, no escape sequences (except `''` for literal quote):

```rust
fn lex_single_quoted(&mut self) -> Result<Token, LexError> {
    let start = self.position.clone();
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
    
    Ok(Token {
        kind: TokenKind::SingleQuoted,
        span: Span { start, end: self.position.clone(), source_id: None },
        value: TokenValue::String(content),
    })
}
```

### 2.7.2 Double-Quoted Strings

With interpolation and escape sequences:

```rust
fn lex_double_quoted(&mut self) -> Result<Token, LexError> {
    let start = self.position.clone();
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
                    Some('n') => { current_literal.push('\n'); self.advance(); }
                    Some('t') => { current_literal.push('\t'); self.advance(); }
                    Some('r') => { current_literal.push('\r'); self.advance(); }
                    Some('\\') => { current_literal.push('\\'); self.advance(); }
                    Some('"') => { current_literal.push('"'); self.advance(); }
                    Some('$') => { current_literal.push('$'); self.advance(); }
                    Some('`') => { current_literal.push('`'); self.advance(); }
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
                    parts.push(InterpolatedPart::Literal(
                        std::mem::take(&mut current_literal)
                    ));
                }
                
                // Parse interpolation
                let interp = self.lex_interpolation()?;
                parts.push(interp);
            }
            Some('`') => {
                // Legacy command substitution
                if !current_literal.is_empty() {
                    parts.push(InterpolatedPart::Literal(
                        std::mem::take(&mut current_literal)
                    ));
                }
                
                let cmd = self.lex_backtick_command()?;
                parts.push(InterpolatedPart::Command(cmd));
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
    
    Ok(Token {
        kind: TokenKind::DoubleQuoted,
        span: Span { start, end: self.position.clone(), source_id: None },
        value: TokenValue::Interpolated(parts),
    })
}
```

### 2.7.3 Raw Strings (Colony extension)

```rust
fn lex_raw_string(&mut self) -> Result<Token, LexError> {
    let start = self.position.clone();
    self.advance(); // consume 'r'
    
    // Determine delimiter
    let delimiter = match self.peek() {
        Some('\'') => '\'',
        Some('"') => '"',
        _ => return Err(LexError::InvalidRawString(start)),
    };
    self.advance();
    
    let mut content = String::new();
    
    loop {
        match self.peek() {
            Some(c) if c == delimiter => {
                self.advance();
                break;
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
    
    Ok(Token {
        kind: TokenKind::RawString,
        span: Span { start, end: self.position.clone(), source_id: None },
        value: TokenValue::String(content),
    })
}
```

### 2.7.4 Triple-Quoted Strings (Colony extension)

```rust
fn lex_triple_quoted(&mut self, quote: char) -> Result<Token, LexError> {
    let start = self.position.clone();
    
    // Consume opening quotes
    self.advance(); // first quote
    self.advance(); // second quote
    self.advance(); // third quote
    
    // Skip leading newline if present
    if self.peek() == Some('\n') {
        self.advance();
    }
    
    let kind = if quote == '\'' {
        TokenKind::TripleSingleQuoted
    } else {
        TokenKind::TripleDoubleQuoted
    };
    
    let mut content = String::new();
    let mut parts = Vec::new();
    
    loop {
        // Check for closing quotes
        if self.peek() == Some(quote) 
           && self.peek_at(1) == Some(quote) 
           && self.peek_at(2) == Some(quote) 
        {
            self.advance();
            self.advance();
            self.advance();
            break;
        }
        
        match self.peek() {
            Some('$') if quote == '"' => {
                if !content.is_empty() {
                    parts.push(InterpolatedPart::Literal(
                        std::mem::take(&mut content)
                    ));
                }
                let interp = self.lex_interpolation()?;
                parts.push(interp);
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
    
    if quote == '\'' {
        Ok(Token {
            kind,
            span: Span { start, end: self.position.clone(), source_id: None },
            value: TokenValue::String(content),
        })
    } else {
        if !content.is_empty() {
            parts.push(InterpolatedPart::Literal(content));
        }
        Ok(Token {
            kind,
            span: Span { start, end: self.position.clone(), source_id: None },
            value: TokenValue::Interpolated(parts),
        })
    }
}
```

## 2.8 Numeric Literals

```rust
fn lex_number(&mut self) -> Result<Token, LexError> {
    let start = self.position.clone();
    
    // Check for base prefix
    if self.peek() == Some('0') {
        self.advance();
        match self.peek() {
            Some('x') | Some('X') => return self.lex_hex_number(start),
            Some('o') | Some('O') => return self.lex_octal_number(start),
            Some('b') | Some('B') => return self.lex_binary_number(start),
            _ => {
                // Could be 0, 0.5, or legacy octal
            }
        }
    }
    
    // Decimal number
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
    if self.peek() == Some('.') && self.peek_next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
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
    
    // Exponent part
    if let Some('e') | Some('E') = self.peek() {
        is_float = true;
        num_str.push('e');
        self.advance();
        
        if let Some('+') | Some('-') = self.peek() {
            num_str.push(self.peek().unwrap());
            self.advance();
        }
        
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                num_str.push(c);
                self.advance();
            } else {
                break;
            }
        }
    }
    
    if is_float {
        let value: f64 = num_str.parse()
            .map_err(|_| LexError::InvalidNumber(start.clone()))?;
        Ok(Token {
            kind: TokenKind::Float,
            span: Span { start, end: self.position.clone(), source_id: None },
            value: TokenValue::Float(value),
        })
    } else {
        let value: i64 = num_str.parse()
            .map_err(|_| LexError::InvalidNumber(start.clone()))?;
        Ok(Token {
            kind: TokenKind::Integer,
            span: Span { start, end: self.position.clone(), source_id: None },
            value: TokenValue::Int(value),
        })
    }
}
```

## 2.9 Operators and Punctuation

```rust
fn lex_operator(&mut self) -> Result<Token, LexError> {
    let start = self.position.clone();
    let c = self.advance().unwrap();
    
    let kind = match c {
        '|' => match self.peek() {
            Some('|') => { self.advance(); TokenKind::Or2 }
            Some('&') => { self.advance(); TokenKind::ErrPipe }
            _ => TokenKind::Pipe,
        },
        '&' => match self.peek() {
            Some('&') => { self.advance(); TokenKind::And2 }
            _ => TokenKind::Amp,
        },
        ';' => match self.peek() {
            Some(';') => {
                self.advance();
                match self.peek() {
                    Some('&') => { self.advance(); TokenKind::SemiSemiAmp }
                    _ => TokenKind::SemiSemi,
                }
            }
            Some('&') => { self.advance(); TokenKind::SemiAmp }
            _ => TokenKind::Semi,
        },
        '-' => match self.peek() {
            Some('>') => { self.advance(); TokenKind::ArrowPipe }
            Some('=') => { self.advance(); TokenKind::MinusAssign }
            _ => TokenKind::Minus,
        },
        '=' => match self.peek() {
            Some('=') => { self.advance(); TokenKind::Eq }
            Some('>') => { self.advance(); TokenKind::FatArrowPipe }
            Some('~') => { self.advance(); TokenKind::TildeEq }
            _ => TokenKind::Assign,
        },
        '~' => match self.peek() {
            Some('>') => { self.advance(); TokenKind::AlterPipe }
            Some('=') => { self.advance(); TokenKind::TildeEq }
            _ => TokenKind::Tilde,
        },
        '!' => match self.peek() {
            Some('=') => { self.advance(); TokenKind::Ne }
            Some('~') => { self.advance(); TokenKind::NotTilde }
            _ => TokenKind::Bang,
        },
        '<' => self.lex_less_than(start)?,
        '>' => self.lex_greater_than(start)?,
        '+' => match self.peek() {
            Some('=') => { self.advance(); TokenKind::PlusAssign }
            _ => TokenKind::Plus,
        },
        '*' => match self.peek() {
            Some('*') => { self.advance(); TokenKind::StarStar }
            Some('=') => { self.advance(); TokenKind::StarAssign }
            _ => TokenKind::Star,
        },
        '/' => match self.peek() {
            Some('/') => { self.advance(); TokenKind::SlashSlash }
            Some('=') => { self.advance(); TokenKind::SlashAssign }
            Some('#') => return self.lex_multiline_comment(),
            _ => TokenKind::Slash,
        },
        '%' => TokenKind::Percent,
        '^' => TokenKind::BitXor,
        '(' => TokenKind::LParen,
        ')' => TokenKind::RParen,
        '{' => TokenKind::LBrace,
        '}' => TokenKind::RBrace,
        '[' => match self.peek() {
            Some('[') => { self.advance(); TokenKind::LDoubleBracket }
            _ => TokenKind::LBracket,
        },
        ']' => match self.peek() {
            Some(']') => { self.advance(); TokenKind::RDoubleBracket }
            _ => TokenKind::RBracket,
        },
        ':' => match self.peek() {
            Some('=') => { self.advance(); TokenKind::ColonAssign }
            _ => TokenKind::Colon,
        },
        ',' => TokenKind::Comma,
        '.' => match self.peek() {
            Some('.') => {
                self.advance();
                match self.peek() {
                    Some('<') => { self.advance(); TokenKind::DotDotLt }
                    Some('=') => { self.advance(); TokenKind::DotDotEq }
                    _ => TokenKind::DotDot,
                }
            }
            _ => TokenKind::Dot,
        },
        '?' => match self.peek() {
            Some('=') => { self.advance(); TokenKind::QmarkAssign }
            _ => TokenKind::Question,
        },
        '#' => return Ok(self.lex_comment()),
        '$' => self.lex_dollar(start)?,
        '@' => self.lex_at(start)?,
        '\\' => TokenKind::Backslash,
        '`' => TokenKind::Backtick,
        '\n' => TokenKind::Newline,
        _ => return Err(LexError::UnexpectedChar(c, start)),
    };
    
    Ok(Token {
        kind,
        span: Span { start, end: self.position.clone(), source_id: None },
        value: TokenValue::None,
    })
}
```

## 2.10 Dollar Expansions

```rust
fn lex_dollar(&mut self, start: Position) -> Result<TokenKind, LexError> {
    // $ already consumed
    match self.peek() {
        Some('(') => {
            self.advance();
            Ok(TokenKind::DollarParen)  // Command substitution
        }
        Some('{') => {
            self.advance();
            Ok(TokenKind::DollarBrace)  // Parameter expansion
        }
        Some('[') => {
            self.advance();
            Ok(TokenKind::DollarBracket)  // Expression substitution
        }
        Some('@') => {
            self.advance();
            Ok(TokenKind::DollarAt)  // $@
        }
        Some('*') => {
            self.advance();
            Ok(TokenKind::DollarStar)  // $*
        }
        Some('?') => {
            self.advance();
            Ok(TokenKind::DollarQmark)  // $?
        }
        Some('!') => {
            self.advance();
            Ok(TokenKind::DollarBang)  // $!
        }
        Some('$') => {
            self.advance();
            Ok(TokenKind::DollarDollar)  // $$
        }
        Some('#') => {
            self.advance();
            Ok(TokenKind::DollarHash)  // $#
        }
        _ => Ok(TokenKind::Dollar),
    }
}
```

## 2.11 Redirection Operators

```rust
fn lex_less_than(&mut self, start: Position) -> Result<TokenKind, LexError> {
    // < already consumed
    match self.peek() {
        Some('<') => {
            self.advance();
            match self.peek() {
                Some('<') => {
                    self.advance();
                    Ok(TokenKind::HereString)  // <<<
                }
                Some('-') => {
                    self.advance();
                    Ok(TokenKind::HereDocStrip)  // <<-
                }
                _ => Ok(TokenKind::HereDoc),  // <<
            }
        }
        Some('(') => {
            self.advance();
            Ok(TokenKind::ProcessSubIn)  // <(
        }
        Some('>') => {
            self.advance();
            Ok(TokenKind::RedirectInOut)  // <>
        }
        Some('&') => {
            self.advance();
            Ok(TokenKind::RedirectDup)  // <&
        }
        Some('=') => {
            self.advance();
            Ok(TokenKind::Le)  // <=
        }
        _ => Ok(TokenKind::Lt),  // < or RedirectIn
    }
}

fn lex_greater_than(&mut self, start: Position) -> Result<TokenKind, LexError> {
    // > already consumed
    match self.peek() {
        Some('>') => {
            self.advance();
            Ok(TokenKind::RedirectAppend)  // >>
        }
        Some('(') => {
            self.advance();
            Ok(TokenKind::ProcessSubOut)  // >(
        }
        Some('|') => {
            self.advance();
            Ok(TokenKind::RedirectClobber)  // >|
        }
        Some('&') => {
            self.advance();
            Ok(TokenKind::RedirectDup)  // >&
        }
        Some('=') => {
            self.advance();
            Ok(TokenKind::Ge)  // >=
        }
        _ => Ok(TokenKind::Gt),  // > or RedirectOut
    }
}
```

## 2.12 Keywords vs Identifiers

Keywords are context-sensitive. In command position, they're recognized as keywords.
In other positions, they may be identifiers or words.

```rust
fn classify_word(&self, text: &str) -> TokenKind {
    // Only classify as keyword in command position
    if !self.is_command_position() {
        return if self.mode == LexerMode::Expression {
            TokenKind::Identifier
        } else {
            TokenKind::Word
        };
    }
    
    // POSIX keywords
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
        
        // Colony keywords (only if enabled)
        "var" if self.options.parse_proc => TokenKind::Var,
        "setvar" if self.options.parse_proc => TokenKind::Setvar,
        "const" if self.options.parse_proc => TokenKind::Const,
        "proc" if self.options.parse_proc => TokenKind::Proc,
        "func" if self.options.parse_func => TokenKind::Func,
        "agent" if self.options.parse_agent => TokenKind::Agent,
        "try" if self.options.parse_proc => TokenKind::Try,
        "catch" if self.options.parse_proc => TokenKind::Catch,
        "finally" if self.options.parse_proc => TokenKind::Finally,
        "match" if self.options.parse_proc => TokenKind::Match,
        "return" => TokenKind::Return,
        "and" if self.options.parse_proc => TokenKind::And,
        "or" if self.options.parse_proc => TokenKind::Or,
        "not" if self.options.parse_proc => TokenKind::Not,
        "true" if self.options.parse_proc => TokenKind::True,
        "false" if self.options.parse_proc => TokenKind::False,
        "null" if self.options.parse_proc => TokenKind::Null,
        
        _ => if self.mode == LexerMode::Expression {
            TokenKind::Identifier
        } else {
            TokenKind::Word
        },
    }
}
```

---

*Continue to Part 3: Type System*