//! Abstract syntax tree for Colony shell commands

use crate::span::Span;
use serde::Serialize;

/// Root of the AST
#[derive(Clone, Debug, Serialize)]
pub struct Program {
    pub statements: Vec<Statement>,
    pub span: Span,
}

/// A single statement
#[derive(Clone, Debug, Serialize)]
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
    /// for ((init; cond; update))
    CStyleFor(CStyleForStatement),
    /// while/do/done
    While(WhileStatement),
    /// until/do/done
    Until(UntilStatement),

    // === Bash Extended Commands ===
    /// [[ expression ]] - Extended conditional
    Conditional(ConditionalCommand),
    /// (( expression )) - Arithmetic command
    Arithmetic(ArithmeticCommand),
    /// coproc [NAME] command
    Coproc(CoprocCommand),

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
            Statement::CStyleFor(f) => &f.span,
            Statement::While(w) => &w.span,
            Statement::Until(u) => &u.span,
            Statement::Conditional(c) => &c.span,
            Statement::Arithmetic(a) => &a.span,
            Statement::Coproc(c) => &c.span,
            Statement::VarDecl(v) => &v.span,
            Statement::FunctionDef(f) => &f.span,
            Statement::ProcDef(p) => &p.span,
            Statement::Return(r) => &r.span,
            Statement::Empty => &Span::DUMMY,
        }
    }
}

/// A simple command: name [args] [redirects]
#[derive(Clone, Debug, Serialize)]
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

/// An assignment: name=value or name=(array values) or name[index]=value
#[derive(Clone, Debug, Serialize)]
pub struct Assignment {
    pub name: String,
    pub subscript: Option<ArraySubscript>,
    pub op: AssignOp,
    pub value: AssignmentValue,
    pub span: Span,
}

/// Value in an assignment
#[derive(Clone, Debug, Serialize)]
pub enum AssignmentValue {
    Word(Word),
    Array(Vec<Word>),
    AssocArray(Vec<(String, String)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum AssignOp {
    Assign,  // =
    Append,  // +=
    Default, // :=
    IfUnset, // ?=
}

/// A pipeline: cmd1 | cmd2 -> cmd3
#[derive(Clone, Debug, Serialize)]
pub struct Pipeline {
    pub elements: Vec<PipelineElement>,
    pub span: Span,
}

/// Element in a pipeline
#[derive(Clone, Debug, Serialize)]
pub struct PipelineElement {
    /// The command
    pub command: Box<Statement>,
    /// Pipe operator following this command (None for last)
    pub pipe_op: Option<PipeOp>,
    /// Negation prefix (!)
    pub negated: bool,
}

/// Pipeline operator
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
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
#[derive(Clone, Debug, Serialize)]
pub struct CommandList {
    pub first: Box<Statement>,
    pub rest: Vec<(ListOp, Statement)>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
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
#[derive(Clone, Debug, Serialize)]
pub struct Redirect {
    /// Source file descriptor (default based on type)
    pub fd: Option<i32>,
    /// Source fd from variable (e.g., {COPROC[1]} syntax)
    pub fd_var: Option<String>,
    /// Redirection type
    pub kind: RedirectKind,
    /// Target (filename, fd number, or here-doc content)
    pub target: RedirectTarget,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
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
    /// >& - Duplicate fd (output direction, defaults to fd 1)
    DupFd,
    /// <& - Duplicate fd (input direction, defaults to fd 0)
    DupFdInput,
    /// <<, <<-, <<< - Here-doc/string
    HereDoc(HereDocKind),
    /// &> - Redirect both stdout and stderr (bash extension)
    AndOutput,
    /// &>> - Append both stdout and stderr (bash extension)
    AndAppend,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum HereDocKind {
    /// << - Normal here-doc
    Normal,
    /// <<- - Strip leading tabs
    StripTabs,
    /// <<< - Here-string
    HereString,
}

#[derive(Clone, Debug, Serialize)]
pub enum RedirectTarget {
    /// Filename
    File(Word),
    /// File descriptor number
    Fd(i32),
    /// Close fd
    Close,
    HereDoc {
        delimiter: String,
        content: String,
        expand: bool,
        strip_tabs: bool,
    },
}

/// A shell word (may contain multiple parts)
#[derive(Clone, Debug, Serialize)]
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
#[derive(Clone, Debug, Serialize)]
pub enum WordPart {
    /// Plain text
    Literal(String),
    /// Single-quoted: 'text'
    SingleQuoted(String),
    /// Double-quoted: "text with $var"
    DoubleQuoted(Vec<WordPart>),
    /// ANSI-C quoted: $'text with \n escapes' - raw string, processed during expansion
    AnsiCQuoted(String),
    /// Variable reference: $var, ${var}, ${var:-default}
    Variable(VariableRef),
    /// Command substitution: $(cmd) or `cmd` - pre-parsed AST (legacy)
    CommandSub(Box<Statement>),
    /// Command substitution: $(cmd) - raw string, parsed during expansion (new)
    CommandSubRaw(String),
    /// Arithmetic expansion: $((expr)) - raw string, evaluated during expansion
    ArithRaw(String),
    /// Tilde expansion: ~ or ~user
    Tilde(Option<String>),
    /// Brace expansion: {a,b,c} or {1..5} - raw string, expanded at expansion time
    BraceExpansion(String),
    /// Process substitution: <(cmd) or >(cmd) - raw command string, direction
    ProcessSub {
        /// The direction: Input (<) means the path can be read from, Output (>) means written to
        direction: ProcessSubDirection,
        /// The raw command string to execute
        command: String,
    },
    /// Extended glob pattern: ?(pat), *(pat), +(pat), @(pat), !(pat)
    /// Stores the operator (?, *, +, @, !) and the pattern list as raw string
    ExtGlob {
        /// The extglob operator: '?' (zero or one), '*' (zero or more), '+' (one or more),
        /// '@' (exactly one), '!' (not)
        operator: ExtGlobOp,
        /// The pattern list, e.g., "foo|bar|baz" from @(foo|bar|baz)
        pattern_list: String,
    },
}

/// Extended glob operator
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ExtGlobOp {
    /// ?(pattern-list) - Matches zero or one occurrence
    ZeroOrOne,
    /// *(pattern-list) - Matches zero or more occurrences
    ZeroOrMore,
    /// +(pattern-list) - Matches one or more occurrences
    OneOrMore,
    /// @(pattern-list) - Matches exactly one of the patterns
    ExactlyOne,
    /// !(pattern-list) - Matches anything except the patterns
    Not,
}

/// Direction of process substitution
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ProcessSubDirection {
    /// <(cmd) - input process substitution, command's stdout becomes readable file
    Input,
    /// >(cmd) - output process substitution, file writes go to command's stdin
    Output,
}

/// Variable reference with optional prefix, subscript and modifier
#[derive(Clone, Debug, Serialize)]
pub struct VariableRef {
    pub name: String,
    pub prefix: Option<VarPrefix>,
    pub subscript: Option<ArraySubscript>,
    pub modifier: Option<VarModifier>,
    pub span: Span,
}

/// Prefix modifier (comes before variable name)
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum VarPrefix {
    /// # - Length
    Length,
    /// ! - Indices
    Indices,
}

/// Array subscript
#[derive(Clone, Debug, Serialize)]
pub enum ArraySubscript {
    /// [index] - numeric or expression
    Index(Box<Expression>),
    /// [@] - all elements as separate words
    All,
    /// [*] - all elements as single word
    Star,
}

/// Variable expansion modifier
#[derive(Clone, Debug, Serialize)]
pub enum VarModifier {
    /// ${var:-word} or ${var-word}
    Default { word: Box<Word>, check_null: bool },
    /// ${var:=word} or ${var=word}
    Assign { word: Box<Word>, check_null: bool },
    /// ${var:?word} or ${var?word}
    Error { word: Box<Word>, check_null: bool },
    /// ${var:+word} or ${var+word}
    Alternate { word: Box<Word>, check_null: bool },
    /// ${#var} or ${#arr[@]}
    Length,
    /// ${!arr[@]} - array indices
    Indices,
    /// ${arr[@]:offset} or ${arr[@]:offset:length}
    Slice {
        offset: Box<Word>,
        length: Option<Box<Word>>,
    },
    /// ${var#pattern} - remove shortest prefix match
    StripPrefix { pattern: Box<Word> },
    /// ${var##pattern} - remove longest prefix match
    StripPrefixGreedy { pattern: Box<Word> },
    /// ${var%pattern} - remove shortest suffix match
    StripSuffix { pattern: Box<Word> },
    /// ${var%%pattern} - remove longest suffix match
    StripSuffixGreedy { pattern: Box<Word> },
    /// ${var^} - uppercase first character, ${var^^} - uppercase all
    UpperCase { all: bool },
    /// ${var,} - lowercase first character, ${var,,} - lowercase all
    LowerCase { all: bool },
    /// ${var~} - toggle case of first character, ${var~~} - toggle case of all
    ToggleCase { all: bool },
    /// ${var/pattern/replacement} - replace first match
    /// ${var//pattern/replacement} - replace all matches
    Substitute {
        pattern: Box<Word>,
        replacement: Box<Word>,
        all: bool, // true for //, false for /
    },
}

/// if/elif/else/fi
#[derive(Clone, Debug, Serialize)]
pub struct IfStatement {
    pub condition: Condition,
    pub then_body: Vec<Statement>,
    pub elif_branches: Vec<ElifBranch>,
    pub else_body: Option<Vec<Statement>>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize)]
pub struct ElifBranch {
    pub condition: Condition,
    pub body: Vec<Statement>,
}

/// Condition for if/while/until
#[derive(Clone, Debug, Serialize)]
pub enum Condition {
    /// Command exit status
    Command(Box<Statement>),
    /// Expression (Colony): if (expr)
    Expression(Expression),
}

/// case/esac
#[derive(Clone, Debug, Serialize)]
pub struct CaseStatement {
    pub value: Word,
    pub arms: Vec<CaseArm>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize)]
pub struct CaseArm {
    pub patterns: Vec<Word>,
    pub body: Vec<Statement>,
    pub terminator: CaseTerminator,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum CaseTerminator {
    /// ;; - End arm
    Break,
    /// ;& - Fallthrough
    Fallthrough,
    /// ;;& - Continue matching
    Continue,
}

/// for loop (traditional: for var in words)
#[derive(Clone, Debug, Serialize)]
pub struct ForStatement {
    pub var: String,
    pub words: Vec<Word>,
    pub body: Vec<Statement>,
    pub redirects: Vec<Redirect>,
    pub span: Span,
}

/// C-style for loop: for ((init; cond; update)); do body; done
#[derive(Clone, Debug, Serialize)]
pub struct CStyleForStatement {
    /// Initialization expression (e.g., "i=0" or "i=0, j=10")
    pub init: Option<String>,
    /// Condition expression (e.g., "i<3" or "i<=5")
    pub condition: Option<String>,
    /// Update expression (e.g., "i++" or "i++, j--")
    pub update: Option<String>,
    /// Loop body
    pub body: Vec<Statement>,
    /// Redirections applied to the loop
    pub redirects: Vec<Redirect>,
    pub span: Span,
}

/// while loop
#[derive(Clone, Debug, Serialize)]
pub struct WhileStatement {
    pub condition: Condition,
    pub body: Vec<Statement>,
    pub redirects: Vec<Redirect>,
    pub span: Span,
}

/// until loop
#[derive(Clone, Debug, Serialize)]
pub struct UntilStatement {
    pub condition: Condition,
    pub body: Vec<Statement>,
    pub redirects: Vec<Redirect>,
    pub span: Span,
}

/// [[ expression ]] - Extended conditional command
#[derive(Clone, Debug, Serialize)]
pub struct ConditionalCommand {
    pub expr: CondExpr,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize)]
pub struct CoprocCommand {
    pub name: Option<String>,
    pub command: Box<Statement>,
    pub span: Span,
}

/// Conditional expression inside [[ ]]
#[derive(Clone, Debug, Serialize)]
pub enum CondExpr {
    And(Box<CondExpr>, Box<CondExpr>),
    Or(Box<CondExpr>, Box<CondExpr>),
    Not(Box<CondExpr>),
    Binary {
        left: String,
        op: CondBinaryOp,
        right: String,
        /// Whether the right operand was quoted (affects glob matching in == and !=)
        right_quoted: bool,
    },
    Unary {
        op: CondUnaryOp,
        arg: String,
    },
    String(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum CondBinaryOp {
    /// == or = (string equality, supports glob patterns in [[ ]])
    Eq,
    /// !=
    Ne,
    /// =~ (regex match)
    Match,
    /// < (string less than)
    Lt,
    /// > (string greater than)
    Gt,
    /// -eq (integer equal)
    IntEq,
    /// -ne (integer not equal)
    IntNe,
    /// -lt (integer less than)
    IntLt,
    /// -le (integer less or equal)
    IntLe,
    /// -gt (integer greater than)
    IntGt,
    /// -ge (integer greater or equal)
    IntGe,
    /// -nt (file newer than)
    NewerThan,
    /// -ot (file older than)
    OlderThan,
    /// -ef (same file)
    SameFile,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum CondUnaryOp {
    /// -n (non-zero length)
    NonZeroLen,
    /// -z (zero length)
    ZeroLen,
    /// -f (regular file)
    IsFile,
    /// -d (directory)
    IsDir,
    /// -e (exists)
    Exists,
    /// -r (readable)
    Readable,
    /// -w (writable)
    Writable,
    /// -x (executable)
    Executable,
    /// -s (non-empty file)
    NonEmpty,
    /// -L or -h (symbolic link)
    IsSymlink,
    /// -p (named pipe)
    IsPipe,
    /// -S (socket)
    IsSocket,
    /// -b (block device)
    IsBlock,
    /// -c (character device)
    IsChar,
    /// -t (terminal)
    IsTerminal,
    /// -O (owned by euid)
    OwnedByEuid,
    /// -G (owned by egid)
    OwnedByEgid,
    /// -N (modified since read)
    ModifiedSinceRead,
    /// -v (variable is set)
    VarIsSet,
}

/// (( expression )) - Arithmetic command
#[derive(Clone, Debug, Serialize)]
pub struct ArithmeticCommand {
    pub expr: String,
    pub span: Span,
}

/// var/const declaration
#[derive(Clone, Debug, Serialize)]
pub struct VarDecl {
    pub kind: VarKind,
    pub name: String,
    pub value: Expression,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum VarKind {
    /// var - Mutable local
    Var,
    /// const - Immutable
    Const,
    /// setvar - Modify existing
    Setvar,
}

/// POSIX function: name() { body }
#[derive(Clone, Debug, Serialize)]
pub struct FunctionDef {
    pub name: String,
    pub body: Vec<Statement>,
    pub span: Span,
}

/// proc definition: proc name(...) { body }
#[derive(Clone, Debug, Serialize)]
pub struct ProcDef {
    pub name: String,
    pub params: Vec<ProcParam>,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProcParam {
    pub name: String,
    pub default: Option<Expression>,
}

/// return statement
#[derive(Clone, Debug, Serialize)]
pub struct ReturnStatement {
    pub value: Option<Expression>,
    pub span: Span,
}

/// Brace group
#[derive(Clone, Debug, Serialize)]
pub struct BraceGroup {
    pub body: Vec<Statement>,
    pub redirects: Vec<Redirect>,
    pub span: Span,
}

/// Subshell
#[derive(Clone, Debug, Serialize)]
pub struct Subshell {
    pub body: Vec<Statement>,
    pub redirects: Vec<Redirect>,
    pub span: Span,
}

/// Expression node
#[derive(Clone, Debug, Serialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum UnaryOp {
    /// - (negation)
    Neg,
    /// not, !
    Not,
    /// ~ (bitwise not)
    BitNot,
}
