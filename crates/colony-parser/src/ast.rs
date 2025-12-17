//! Abstract syntax tree for Colony shell commands

use crate::span::Span;

/// Root of the AST
#[derive(Clone, Debug)]
pub struct Program {
    pub statements: Vec<Statement>,
    pub span: Span,
}

/// A single statement
#[derive(Clone, Debug)]
pub enum Statement {
    // === Commands ===
    /// Simple command: name arg1 arg2 ...
    SimpleCommand(SimpleCommand),
    /// Pipeline: cmd1 | cmd2 | cmd3
    Pipeline(Pipeline),
    /// Command list: cmd1 && cmd2 || cmd3
    List(CommandList),

    // === Compound Commands ===
    /// { commands }
    BraceGroup(BraceGroup),
    /// ( commands )
    Subshell(Subshell),
    /// if/elif/else/fi
    If(IfStatement),
    /// case/esac
    Case(CaseStatement),
    /// for/do/done
    For(ForStatement),
    /// while/do/done
    While(WhileStatement),
    /// until/do/done
    Until(UntilStatement),

    // === Declarations (Colony) ===
    /// var x = value
    VarDecl(VarDecl),
    /// function name() { }
    FunctionDef(FunctionDef),
    /// proc name(...) { }
    ProcDef(ProcDef),

    /// return [value]
    Return(ReturnStatement),

    /// Empty statement
    Empty,
}

impl Statement {
    pub fn span(&self) -> &Span {
        match self {
            Statement::SimpleCommand(c) => &c.span,
            Statement::Pipeline(p) => &p.span,
            Statement::List(l) => &l.span,
            Statement::BraceGroup(b) => &b.span,
            Statement::Subshell(s) => &s.span,
            Statement::If(i) => &i.span,
            Statement::Case(c) => &c.span,
            Statement::For(f) => &f.span,
            Statement::While(w) => &w.span,
            Statement::Until(u) => &u.span,
            Statement::VarDecl(v) => &v.span,
            Statement::FunctionDef(f) => &f.span,
            Statement::ProcDef(p) => &p.span,
            Statement::Return(r) => &r.span,
            Statement::Empty => &Span::DUMMY,
        }
    }
}

/// A simple command: name [args] [redirects]
#[derive(Clone, Debug)]
pub struct SimpleCommand {
    /// Command name/path
    pub name: Word,
    /// Command arguments
    pub args: Vec<Word>,
    /// I/O redirections
    pub redirects: Vec<Redirect>,
    /// Environment assignments before command
    pub env_assignments: Vec<Assignment>,
    pub span: Span,
}

/// An assignment: name=value or name+=value
#[derive(Clone, Debug)]
pub struct Assignment {
    pub name: String,
    pub op: AssignOp,
    pub value: Word,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AssignOp {
    Assign,  // =
    Append,  // +=
    Default, // :=
    IfUnset, // ?=
}

/// A pipeline: cmd1 | cmd2 -> cmd3
#[derive(Clone, Debug)]
pub struct Pipeline {
    pub elements: Vec<PipelineElement>,
    pub span: Span,
}

/// Element in a pipeline
#[derive(Clone, Debug)]
pub struct PipelineElement {
    /// The command
    pub command: Box<Statement>,
    /// Pipe operator following this command (None for last)
    pub pipe_op: Option<PipeOp>,
    /// Negation prefix (!)
    pub negated: bool,
}

/// Pipeline operator
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PipeOp {
    /// | - POSIX byte pipe
    Pipe,
    /// -> - Typed pipe
    Arrow,
    /// => - Auto-converting pipe
    FatArrow,
    /// ~> - Alter pipe
    Alter,
    /// |& - Pipe stderr too
    PipeErr,
}

/// List of commands with operators
#[derive(Clone, Debug)]
pub struct CommandList {
    pub first: Box<Statement>,
    pub rest: Vec<(ListOp, Statement)>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListOp {
    /// && - Run if previous succeeded
    And,
    /// || - Run if previous failed
    Or,
    /// ; - Sequential
    Semi,
    /// & - Background
    Background,
}

/// I/O redirection
#[derive(Clone, Debug)]
pub struct Redirect {
    /// Source file descriptor (default based on type)
    pub fd: Option<i32>,
    /// Redirection type
    pub kind: RedirectKind,
    /// Target (filename, fd number, or here-doc content)
    pub target: RedirectTarget,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RedirectKind {
    /// < - Input
    Input,
    /// > - Output (truncate)
    Output,
    /// >> - Output (append)
    Append,
    /// >| - Output (clobber)
    Clobber,
    /// <> - Input/output
    ReadWrite,
    /// <& or >& - Duplicate fd
    DupFd,
    /// <<, <<-, <<< - Here-doc/string
    HereDoc(HereDocKind),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HereDocKind {
    /// << - Normal here-doc
    Normal,
    /// <<- - Strip leading tabs
    StripTabs,
    /// <<< - Here-string
    HereString,
}

#[derive(Clone, Debug)]
pub enum RedirectTarget {
    /// Filename
    File(Word),
    /// File descriptor number
    Fd(i32),
    /// Close fd
    Close,
    /// Here-document content
    HereDoc {
        delimiter: String,
        content: String,
        expand: bool,
    },
}

/// A shell word (may contain multiple parts)
#[derive(Clone, Debug)]
pub struct Word {
    pub parts: Vec<WordPart>,
    pub span: Span,
}

impl Word {
    pub fn literal(s: &str, span: Span) -> Self {
        Self {
            parts: vec![WordPart::Literal(s.to_string())],
            span,
        }
    }

    pub fn is_literal(&self) -> bool {
        self.parts.len() == 1 && matches!(&self.parts[0], WordPart::Literal(_))
    }

    pub fn as_literal(&self) -> Option<&str> {
        if self.parts.len() == 1 {
            match &self.parts[0] {
                WordPart::Literal(s) => Some(s),
                _ => None,
            }
        } else {
            None
        }
    }
}

/// Part of a word
#[derive(Clone, Debug)]
pub enum WordPart {
    /// Plain text
    Literal(String),
    /// Single-quoted: 'text'
    SingleQuoted(String),
    /// Double-quoted: "text with $var"
    DoubleQuoted(Vec<WordPart>),
    /// Variable reference: $var, ${var}, ${var:-default}
    Variable(VariableRef),
    /// Command substitution: $(cmd) or `cmd` - pre-parsed AST (legacy)
    CommandSub(Box<Statement>),
    /// Command substitution: $(cmd) - raw string, parsed during expansion (new)
    CommandSubRaw(String),
    /// Tilde expansion: ~ or ~user
    Tilde(Option<String>),
}

/// Variable reference with optional modifier
#[derive(Clone, Debug)]
pub struct VariableRef {
    pub name: String,
    pub modifier: Option<VarModifier>,
    pub span: Span,
}

/// Variable expansion modifier
#[derive(Clone, Debug)]
pub enum VarModifier {
    /// ${var:-word} or ${var-word}
    Default {
        word: Box<Word>,
        check_null: bool, // : prefix
    },
    /// ${var:=word} or ${var=word}
    Assign {
        word: Box<Word>,
        check_null: bool,
    },
    /// ${var:?word} or ${var?word}
    Error {
        word: Box<Word>,
        check_null: bool,
    },
    /// ${var:+word} or ${var+word}
    Alternate {
        word: Box<Word>,
        check_null: bool,
    },
    /// ${#var}
    Length,
}

/// if/elif/else/fi
#[derive(Clone, Debug)]
pub struct IfStatement {
    pub condition: Condition,
    pub then_body: Vec<Statement>,
    pub elif_branches: Vec<ElifBranch>,
    pub else_body: Option<Vec<Statement>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ElifBranch {
    pub condition: Condition,
    pub body: Vec<Statement>,
}

/// Condition for if/while/until
#[derive(Clone, Debug)]
pub enum Condition {
    /// Command exit status
    Command(Box<Statement>),
    /// Expression (Colony): if (expr)
    Expression(Expression),
}

/// case/esac
#[derive(Clone, Debug)]
pub struct CaseStatement {
    pub value: Word,
    pub arms: Vec<CaseArm>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct CaseArm {
    pub patterns: Vec<Word>,
    pub body: Vec<Statement>,
    pub terminator: CaseTerminator,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CaseTerminator {
    /// ;; - End arm
    Break,
    /// ;& - Fallthrough
    Fallthrough,
    /// ;;& - Continue matching
    Continue,
}

/// for loop
#[derive(Clone, Debug)]
pub struct ForStatement {
    pub var: String,
    pub words: Vec<Word>,
    pub body: Vec<Statement>,
    pub span: Span,
}

/// while loop
#[derive(Clone, Debug)]
pub struct WhileStatement {
    pub condition: Condition,
    pub body: Vec<Statement>,
    pub span: Span,
}

/// until loop
#[derive(Clone, Debug)]
pub struct UntilStatement {
    pub condition: Condition,
    pub body: Vec<Statement>,
    pub span: Span,
}

/// var/const declaration
#[derive(Clone, Debug)]
pub struct VarDecl {
    pub kind: VarKind,
    pub name: String,
    pub value: Expression,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VarKind {
    /// var - Mutable local
    Var,
    /// const - Immutable
    Const,
    /// setvar - Modify existing
    Setvar,
}

/// POSIX function: name() { body }
#[derive(Clone, Debug)]
pub struct FunctionDef {
    pub name: String,
    pub body: Vec<Statement>,
    pub span: Span,
}

/// proc definition: proc name(...) { body }
#[derive(Clone, Debug)]
pub struct ProcDef {
    pub name: String,
    pub params: Vec<ProcParam>,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ProcParam {
    pub name: String,
    pub default: Option<Expression>,
}

/// return statement
#[derive(Clone, Debug)]
pub struct ReturnStatement {
    pub value: Option<Expression>,
    pub span: Span,
}

/// Brace group
#[derive(Clone, Debug)]
pub struct BraceGroup {
    pub body: Vec<Statement>,
    pub span: Span,
}

/// Subshell
#[derive(Clone, Debug)]
pub struct Subshell {
    pub body: Vec<Statement>,
    pub span: Span,
}

/// Expression node
#[derive(Clone, Debug)]
pub enum Expression {
    // === Literals ===
    Null(Span),
    Bool(bool, Span),
    Int(i64, Span),
    Float(f64, Span),
    Str(String, Span),

    // === Collections ===
    List {
        items: Vec<Expression>,
        span: Span,
    },
    Dict {
        entries: Vec<(Expression, Expression)>,
        span: Span,
    },

    // === References ===
    Variable {
        name: String,
        span: Span,
    },
    Index {
        target: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },

    // === Operators ===
    Binary {
        left: Box<Expression>,
        op: BinaryOp,
        right: Box<Expression>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expression>,
        span: Span,
    },

    // === Conditionals ===
    Ternary {
        condition: Box<Expression>,
        then_expr: Box<Expression>,
        else_expr: Box<Expression>,
        span: Span,
    },
    NullCoalesce {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },

    // === Field Access ===
    Field {
        target: Box<Expression>,
        field: String,
        span: Span,
    },

    // === Calls ===
    FuncCall {
        name: String,
        args: Vec<Expression>,
        span: Span,
    },

    // === Substitutions ===
    CommandSub {
        command: Box<Statement>,
        span: Span,
    },

    // === Collections (Additional) ===
    Tuple {
        items: Vec<Expression>,
        span: Span,
    },

    // === Lambda ===
    Lambda {
        params: Vec<String>,
        body: Box<Expression>,
        span: Span,
    },

    // === Grouping ===
    Paren {
        inner: Box<Expression>,
        span: Span,
    },
}

impl Expression {
    pub fn span(&self) -> &Span {
        match self {
            Expression::Null(s) => s,
            Expression::Bool(_, s) => s,
            Expression::Int(_, s) => s,
            Expression::Float(_, s) => s,
            Expression::Str(_, s) => s,
            Expression::List { span, .. } => span,
            Expression::Dict { span, .. } => span,
            Expression::Variable { span, .. } => span,
            Expression::Index { span, .. } => span,
            Expression::Binary { span, .. } => span,
            Expression::Unary { span, .. } => span,
            Expression::Ternary { span, .. } => span,
            Expression::NullCoalesce { span, .. } => span,
            Expression::Field { span, .. } => span,
            Expression::FuncCall { span, .. } => span,
            Expression::CommandSub { span, .. } => span,
            Expression::Tuple { span, .. } => span,
            Expression::Lambda { span, .. } => span,
            Expression::Paren { span, .. } => span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Pattern matching
    Match,
    NotMatch,
    // Logical
    And,
    Or,
    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnaryOp {
    /// - (negation)
    Neg,
    /// not, !
    Not,
    /// ~ (bitwise not)
    BitNot,
}
