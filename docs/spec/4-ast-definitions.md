# Part 4: AST Definitions

## 4.1 Overview

The Abstract Syntax Tree (AST) represents parsed Colony code. All nodes include source span information for error reporting and tooling.

```rust
// crates/colony-parser/src/ast.rs

use crate::span::Span;
use colony_core::types::{TypeAnnotation, PipelineType};
```

## 4.2 Program and Statements

```rust
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
    /// match expression { arms }
    Match(MatchStatement),
    
    // === Declarations ===
    /// var x = value
    VarDecl(VarDecl),
    /// function name() { }
    FunctionDef(FunctionDef),
    /// proc name(...) { }
    ProcDef(ProcDef),
    /// func name(...) -> Type { }
    FuncDef(FuncDef),
    /// agent name { }
    AgentDef(AgentDef),
    
    // === Error Handling ===
    /// try { } catch { } finally { }
    Try(TryStatement),
    
    // === Control Flow ===
    /// return [value]
    Return(ReturnStatement),
    /// break [label]
    Break(BreakStatement),
    /// continue [label]
    Continue(ContinueStatement),
    
    // === Expressions ===
    /// Expression used as statement
    Expression(Expression),
    
    // === Assignment ===
    /// name = value (POSIX style)
    Assignment(Assignment),
    
    // === Empty ===
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
            Statement::Match(m) => &m.span,
            Statement::VarDecl(v) => &v.span,
            Statement::FunctionDef(f) => &f.span,
            Statement::ProcDef(p) => &p.span,
            Statement::FuncDef(f) => &f.span,
            Statement::AgentDef(a) => &a.span,
            Statement::Try(t) => &t.span,
            Statement::Return(r) => &r.span,
            Statement::Break(b) => &b.span,
            Statement::Continue(c) => &c.span,
            Statement::Expression(e) => e.span(),
            Statement::Assignment(a) => &a.span,
            Statement::Empty => &Span::DUMMY,
        }
    }
}
```

## 4.3 Simple Command

```rust
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
    Assign,      // =
    Append,      // +=
    Default,     // :=
    IfUnset,     // ?=
}
```

## 4.4 Pipeline

```rust
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
    /// Time prefix (time)
    pub timed: bool,
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
```

## 4.5 Command List

```rust
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
```

## 4.6 Redirections

```rust
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
```

## 4.7 Word and Expansions

```rust
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
    /// Raw string: r'text'
    RawString(String),
    /// Triple-quoted: '''text''' or """text"""
    TripleQuoted {
        content: String,
        interpolate: bool,
        parts: Vec<WordPart>,
    },
    /// Variable reference: $var, ${var}, ${var:-default}
    Variable(VariableRef),
    /// Command substitution: $(cmd) or `cmd`
    CommandSub(Box<Statement>),
    /// Expression substitution: $[expr]
    ExpressionSub(Box<Expression>),
    /// Arithmetic substitution: $((expr))
    ArithmeticSub(Box<Expression>),
    /// Process substitution: <(cmd) or >(cmd)
    ProcessSub {
        kind: ProcessSubKind,
        command: Box<Statement>,
    },
    /// Brace expansion: {a,b,c} or {1..10}
    BraceExpansion(BraceExpansion),
    /// Glob pattern
    Glob(GlobPattern),
    /// Tilde expansion: ~ or ~user
    Tilde(Option<String>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProcessSubKind {
    /// <(cmd) - Input from command
    Input,
    /// >(cmd) - Output to command
    Output,
}
```

## 4.8 Variable References

```rust
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
        check_null: bool,  // : prefix
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
    /// ${var#pattern} or ${var##pattern}
    StripPrefix {
        pattern: Box<Word>,
        greedy: bool,
    },
    /// ${var%pattern} or ${var%%pattern}
    StripSuffix {
        pattern: Box<Word>,
        greedy: bool,
    },
    /// ${var/pattern/replacement} or ${var//pattern/replacement}
    Replace {
        pattern: Box<Word>,
        replacement: Box<Word>,
        all: bool,
    },
    /// ${var^}, ${var^^}, ${var,}, ${var,,}
    Case(CaseModifier),
    /// ${var:start} or ${var:start:length}
    Slice {
        start: Box<Expression>,
        length: Option<Box<Expression>>,
    },
    /// ${var[index]}
    Index(Box<Expression>),
    /// ${!var} - Indirect reference
    Indirect,
    /// ${!prefix*} or ${!prefix@} - Names matching prefix
    Names {
        prefix: String,
        separate: bool,
    },
    /// ${var@operator}
    Transform(TransformOp),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CaseModifier {
    /// ^ - Uppercase first
    UpperFirst,
    /// ^^ - Uppercase all
    UpperAll,
    /// , - Lowercase first
    LowerFirst,
    /// ,, - Lowercase all
    LowerAll,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformOp {
    /// @U - Uppercase
    Upper,
    /// @u - Uppercase first
    UpperFirst,
    /// @L - Lowercase
    Lower,
    /// @Q - Quote for reuse
    Quote,
    /// @E - Escape
    Escape,
    /// @P - Prompt expansion
    Prompt,
    /// @A - Assignment format
    Assign,
    /// @K - Key-value format
    KeyValue,
    /// @a - Attributes
    Attributes,
}
```

## 4.9 Glob Patterns

```rust
/// Glob pattern
#[derive(Clone, Debug)]
pub struct GlobPattern {
    pub parts: Vec<GlobPart>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum GlobPart {
    /// Literal text
    Literal(String),
    /// ? - Single character
    Question,
    /// * - Any characters (not /)
    Star,
    /// ** - Any characters including /
    DoubleStar,
    /// [abc] or [a-z] or [!abc]
    CharClass {
        negated: bool,
        items: Vec<CharClassItem>,
    },
    /// @(pat|pat) - Extended glob
    ExtGlob(ExtGlobKind, Vec<GlobPattern>),
}

#[derive(Clone, Debug)]
pub enum CharClassItem {
    Single(char),
    Range(char, char),
    Class(CharClassName),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CharClassName {
    Alnum,
    Alpha,
    Ascii,
    Blank,
    Cntrl,
    Digit,
    Graph,
    Lower,
    Print,
    Punct,
    Space,
    Upper,
    Word,
    Xdigit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExtGlobKind {
    /// ?(pat) - Zero or one
    Optional,
    /// *(pat) - Zero or more
    Star,
    /// +(pat) - One or more
    Plus,
    /// @(pat) - Exactly one
    Exact,
    /// !(pat) - Not matching
    Not,
}
```

## 4.10 Brace Expansion

```rust
/// Brace expansion: {a,b,c} or {1..10}
#[derive(Clone, Debug)]
pub enum BraceExpansion {
    /// {item1,item2,item3}
    List(Vec<Vec<WordPart>>),
    /// {start..end} or {start..end..step}
    Sequence {
        start: BraceEndpoint,
        end: BraceEndpoint,
        step: Option<i64>,
    },
}

#[derive(Clone, Debug)]
pub enum BraceEndpoint {
    Number(i64),
    Char(char),
}
```

## 4.11 Control Flow Statements

```rust
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
    pub value: CaseValue,
    pub arms: Vec<CaseArm>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum CaseValue {
    Word(Word),
    Expression(Expression),
}

#[derive(Clone, Debug)]
pub struct CaseArm {
    pub patterns: Vec<CasePattern>,
    pub body: Vec<Statement>,
    pub terminator: CaseTerminator,
}

#[derive(Clone, Debug)]
pub enum CasePattern {
    /// Glob pattern (POSIX)
    Glob(GlobPattern),
    /// Literal value
    Literal(Expression),
    /// Range: 1..10
    Range(Expression, Expression),
    /// Guard: pattern if condition
    Guard {
        pattern: Box<CasePattern>,
        condition: Expression,
    },
    /// Default/else
    Default,
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
    pub kind: ForKind,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ForKind {
    /// for var in words
    Words {
        var: String,
        words: Vec<Word>,
    },
    /// for var in (expr)
    Iterate {
        var: String,
        expr: Expression,
    },
    /// for i, var in (expr)
    Enumerate {
        index: String,
        var: String,
        expr: Expression,
    },
    /// for ((init; cond; step))
    CStyle {
        init: Option<Expression>,
        condition: Option<Expression>,
        step: Option<Expression>,
    },
}

/// while/until loop
#[derive(Clone, Debug)]
pub struct WhileStatement {
    pub condition: Condition,
    pub body: Vec<Statement>,
    pub span: Span,
}

/// match expression { arms }
#[derive(Clone, Debug)]
pub struct MatchStatement {
    pub value: Expression,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<Expression>,
    pub body: MatchBody,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum MatchBody {
    Block(Vec<Statement>),
    Expr(Expression),
}

#[derive(Clone, Debug)]
pub enum MatchPattern {
    List(ListPattern),
    Dict(DictPattern),
    Tuple(TuplePattern),
    Literal(Expression),
    Binding(String),
    Wildcard,
}

#[derive(Clone, Debug)]
pub struct ListPattern {
    pub items: Vec<MatchPattern>,
    pub rest: ListRest,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ListRest {
    None,
    Ignore,
    Bind(String),
}

#[derive(Clone, Debug)]
pub struct DictPattern {
    pub fields: Vec<DictPatternField>,
    pub allow_extra: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct DictPatternField {
    pub name: String,
    pub pattern: Option<MatchPattern>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TuplePattern {
    pub items: Vec<MatchPattern>,
    pub span: Span,
}
```

## 4.12 Declarations

```rust
/// var/const declaration
#[derive(Clone, Debug)]
pub struct VarDecl {
    pub kind: VarKind,
    pub name: String,
    pub type_annotation: Option<TypeAnnotation>,
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
    pub pipeline_type: Option<ProcPipelineType>,
    pub params: ProcParams,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ProcPipelineType {
    pub input: PipelineType,
    pub output: PipelineType,
}

#[derive(Clone, Debug, Default)]
pub struct ProcParams {
    pub positional: Vec<ProcParam>,
    pub named: Vec<ProcParam>,
    pub rest_positional: Option<String>,
    pub rest_named: Option<String>,
    pub block: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProcParam {
    pub name: String,
    pub type_annotation: Option<TypeAnnotation>,
    pub default: Option<Expression>,
}

/// func definition: func name(...) -> Type { expr }
#[derive(Clone, Debug)]
pub struct FuncDef {
    pub name: String,
    pub params: Vec<FuncParam>,
    pub return_type: Option<TypeAnnotation>,
    pub body: FuncBody,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FuncParam {
    pub name: String,
    pub type_annotation: Option<TypeAnnotation>,
    pub default: Option<Expression>,
}

#[derive(Clone, Debug)]
pub enum FuncBody {
    /// Single expression: = expr
    Expression(Expression),
    /// Block with statements
    Block(Vec<Statement>),
}
```

## 4.13 Agent Definition

```rust
/// agent definition
#[derive(Clone, Debug)]
pub struct AgentDef {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub capabilities: AgentCapabilities,
    pub input_params: Vec<ProcParam>,
    pub output_type: Option<TypeAnnotation>,
    pub prompt: AgentPrompt,
    pub hooks: AgentHooks,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct AgentCapabilities {
    pub tools: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub model: Option<String>,
    pub context: ContextCapabilities,
}

#[derive(Clone, Debug, Default)]
pub struct ContextCapabilities {
    pub read: bool,
    pub write: bool,
    pub types: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum AgentPrompt {
    /// Literal string
    Literal(String),
    /// Template with {variable} interpolation
    Template {
        template: String,
        variables: Vec<String>,
    },
    /// External file
    File(String),
}

#[derive(Clone, Debug, Default)]
pub struct AgentHooks {
    pub on_start: Option<Vec<Statement>>,
    pub on_tool_call: Option<AgentToolCallHook>,
    pub on_message: Option<AgentMessageHook>,
    pub on_complete: Option<AgentCompleteHook>,
    pub on_error: Option<AgentErrorHook>,
}

#[derive(Clone, Debug)]
pub struct AgentToolCallHook {
    pub tool_var: String,
    pub args_var: String,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct AgentMessageHook {
    pub message_var: String,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct AgentCompleteHook {
    pub result_var: String,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct AgentErrorHook {
    pub error_var: String,
    pub body: Vec<Statement>,
}
```

## 4.14 Error Handling

```rust
/// try/catch/finally
#[derive(Clone, Debug)]
pub struct TryStatement {
    pub body: Vec<Statement>,
    pub catch: Option<CatchBlock>,
    pub finally: Option<Vec<Statement>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct CatchBlock {
    pub error_var: Option<String>,
    pub error_type: Option<String>,
    pub body: Vec<Statement>,
}
```

## 4.15 Control Flow

```rust
/// return statement
#[derive(Clone, Debug)]
pub struct ReturnStatement {
    pub value: Option<Expression>,
    pub span: Span,
}

/// break statement
#[derive(Clone, Debug)]
pub struct BreakStatement {
    pub label: Option<String>,
    pub span: Span,
}

/// continue statement
#[derive(Clone, Debug)]
pub struct ContinueStatement {
    pub label: Option<String>,
    pub span: Span,
}
```

## 4.16 Expressions

```rust
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
    Range {
        start: Box<Expression>,
        end: Box<Expression>,
        inclusive: bool,
        span: Span,
    },
    Tuple {
        items: Vec<Expression>,
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
    Field {
        target: Box<Expression>,
        field: String,
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
    
    // === Calls ===
    FuncCall {
        name: String,
        args: Vec<Expression>,
        named_args: Vec<(String, Expression)>,
        span: Span,
    },
    MethodCall {
        target: Box<Expression>,
        method: String,
        args: Vec<Expression>,
        span: Span,
    },
    
    // === Substitutions ===
    CommandSub {
        command: Box<Statement>,
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
            Expression::Range { span, .. } => span,
            Expression::Tuple { span, .. } => span,
            Expression::Variable { span, .. } => span,
            Expression::Index { span, .. } => span,
            Expression::Field { span, .. } => span,
            Expression::Binary { span, .. } => span,
            Expression::Unary { span, .. } => span,
            Expression::Ternary { span, .. } => span,
            Expression::NullCoalesce { span, .. } => span,
            Expression::FuncCall { span, .. } => span,
            Expression::MethodCall { span, .. } => span,
            Expression::CommandSub { span, .. } => span,
            Expression::Lambda { span, .. } => span,
            Expression::Paren { span, .. } => span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinaryOp {
    // Arithmetic
    Add, Sub, Mul, Div, IntDiv, Mod, Pow,
    // Comparison
    Eq, Ne, Lt, Le, Gt, Ge,
    // Pattern
    Match, NotMatch,
    // Logical
    And, Or,
    // String
    Concat,
    // Bitwise
    BitAnd, BitOr, BitXor, Shl, Shr,
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
```

---

*Continue to Part 5: Execution Model*
