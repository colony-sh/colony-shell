//! Token types for Colony shell lexer

use crate::span::Span;

/// A lexical token
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub value: TokenValue,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span, value: TokenValue) -> Self {
        Self { kind, span, value }
    }

    pub fn simple(kind: TokenKind, span: Span) -> Self {
        Self {
            kind,
            span,
            value: TokenValue::None,
        }
    }
}

/// Token value payload
#[derive(Clone, Debug, PartialEq)]
pub enum TokenValue {
    None,
    String(String),
    Int(i64),
    Float(f64),
    Interpolated(Vec<InterpolatedPart>),
}

/// Parts of an interpolated string
#[derive(Clone, Debug, PartialEq)]
pub enum InterpolatedPart {
    Literal(String),
    Variable(String),
    Command(Vec<Token>),
    Expression(Vec<Token>),
}

/// All token kinds in Colony shell
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // === Literals ===
    /// Bareword in command mode
    Word,
    /// Identifier in expression mode
    Identifier,
    /// '...'
    SingleQuoted,
    /// "..." (with interpolation)
    DoubleQuoted,
    /// r'...'
    RawString,
    /// '''...'''
    TripleSingleQuoted,
    /// """..."""
    TripleDoubleQuoted,
    /// 42, 0x2A, 0o52, 0b101010
    Integer,
    /// 3.14, 1e10
    Float,

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
    /// Keyword 'and'
    And,
    /// Keyword 'or'
    Or,
    /// Keyword 'not'
    Not,
    True,
    False,
    Null,

    // === Pipeline Operators ===
    /// |
    Pipe,
    /// ->
    ArrowPipe,
    /// =>
    FatArrowPipe,
    /// ~>
    AlterPipe,
    /// |& or 2>&1 |
    ErrPipe,

    // === List Operators ===
    /// &&
    And2,
    /// ||
    Or2,
    /// ;
    Semi,
    /// &
    Amp,
    /// ;& (case fallthrough)
    SemiAmp,
    /// ;; (case terminator)
    SemiSemi,
    /// ;;&
    SemiSemiAmp,

    // === Comparison ===
    /// ==
    Eq,
    /// !=
    Ne,
    /// <
    Lt,
    /// <=
    Le,
    /// >
    Gt,
    /// >=
    Ge,
    /// ~ (pattern match)
    Tilde,
    /// ~= or =~
    TildeEq,
    /// !~
    NotTilde,

    // === Arithmetic ===
    /// +
    Plus,
    /// -
    Minus,
    /// *
    Star,
    /// /
    Slash,
    /// // (integer division)
    SlashSlash,
    /// %
    Percent,
    /// ** (power)
    StarStar,

    // === Bitwise ===
    /// & (in expression mode)
    BitAnd,
    /// | (in expression mode)
    BitOr,
    /// ^
    BitXor,
    /// ~~ or ! (context-dependent)
    BitNot,
    /// <<
    Shl,
    /// >>
    Shr,

    // === Assignment ===
    /// =
    Assign,
    /// +=
    PlusAssign,
    /// -=
    MinusAssign,
    /// *=
    StarAssign,
    /// /=
    SlashAssign,
    /// :=
    ColonAssign,
    /// ?=
    QmarkAssign,

    // === Delimiters ===
    /// (
    LParen,
    /// )
    RParen,
    /// {
    LBrace,
    /// }
    RBrace,
    /// [
    LBracket,
    /// ]
    RBracket,
    /// [[
    LDoubleBracket,
    /// ]]
    RDoubleBracket,

    // === Redirection ===
    /// <
    RedirectIn,
    /// >
    RedirectOut,
    /// >>
    RedirectAppend,
    /// >|
    RedirectClobber,
    /// <>
    RedirectInOut,
    /// N> or N<
    RedirectFd,
    /// >&N, <&N
    RedirectDup,
    /// >&-, <&-
    RedirectClose,
    /// <<
    HereDoc,
    /// <<-
    HereDocStrip,
    /// <<<
    HereString,
    /// <(
    ProcessSubIn,
    /// >(
    ProcessSubOut,

    // === Special ===
    /// $
    Dollar,
    /// @
    At,
    /// !
    Bang,
    /// ?
    Question,
    /// :
    Colon,
    /// ,
    Comma,
    /// .
    Dot,
    /// ..
    DotDot,
    /// ..<
    DotDotLt,
    /// ..=
    DotDotEq,
    /// #
    Hash,
    /// \
    Backslash,
    /// `
    Backtick,

    // === Compound Tokens ===
    /// $(
    DollarParen,
    /// ${
    DollarBrace,
    /// $[
    DollarBracket,
    /// $@
    DollarAt,
    /// $*
    DollarStar,
    /// $?
    DollarQmark,
    /// $!
    DollarBang,
    /// $$
    DollarDollar,
    /// $#
    DollarHash,
    /// @(
    AtParen,
    /// @{
    AtBrace,
    /// @[
    AtBracket,

    // === Structural ===
    Newline,
    Comment,
    Eof,

    // === Error Recovery ===
    Error,
}

impl TokenKind {
    /// Check if this token is a keyword (POSIX or Colony)
    pub fn is_keyword(self) -> bool {
        matches!(
            self,
            TokenKind::If
                | TokenKind::Then
                | TokenKind::Else
                | TokenKind::Elif
                | TokenKind::Fi
                | TokenKind::Case
                | TokenKind::Esac
                | TokenKind::In
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Until
                | TokenKind::Do
                | TokenKind::Done
                | TokenKind::Function
                | TokenKind::Var
                | TokenKind::Setvar
                | TokenKind::Const
                | TokenKind::Proc
                | TokenKind::Func
                | TokenKind::Agent
                | TokenKind::Try
                | TokenKind::Catch
                | TokenKind::Finally
                | TokenKind::Match
                | TokenKind::Return
                | TokenKind::And
                | TokenKind::Or
                | TokenKind::Not
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
        )
    }

    /// Check if this token is a pipeline operator
    pub fn is_pipe_op(self) -> bool {
        matches!(
            self,
            TokenKind::Pipe
                | TokenKind::ArrowPipe
                | TokenKind::FatArrowPipe
                | TokenKind::AlterPipe
                | TokenKind::ErrPipe
        )
    }

    /// Check if this token is a list operator
    pub fn is_list_op(self) -> bool {
        matches!(
            self,
            TokenKind::And2 | TokenKind::Or2 | TokenKind::Semi | TokenKind::Amp
        )
    }
}
