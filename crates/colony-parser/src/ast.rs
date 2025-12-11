//! Abstract syntax tree for shell commands

/// Top-level command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Simple command (e.g., `ls -l`)
    Simple(SimpleCommand),
    /// Pipeline (e.g., `ls | grep foo`)
    Pipeline(Vec<Command>),
    /// List with && or ||
    List(Box<Command>, ListOp, Box<Command>),
    /// Subshell ( ... )
    Subshell(Box<Command>),
    /// Brace group { ...; }
    BraceGroup(Vec<Command>),
}

/// Simple command with optional assignments and redirects
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleCommand {
    /// Variable assignments (VAR=value)
    pub assignments: Vec<Assignment>,
    /// Command words (command + arguments)
    pub words: Vec<Word>,
    /// Redirections
    pub redirects: Vec<Redirect>,
}

/// Variable assignment
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub name: String,
    pub value: Word,
}

/// Word (possibly with expansions)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub parts: Vec<WordPart>,
}

/// Parts of a word
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordPart {
    /// Literal text
    Literal(String),
    /// Parameter expansion ${var}
    Parameter(String),
    /// Command substitution $(cmd) or `cmd`
    CommandSub(Box<Command>),
    /// Arithmetic expansion $((expr))
    Arithmetic(String),
}

/// List operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListOp {
    /// &&
    And,
    /// ||
    Or,
    /// ;
    Seq,
    /// &
    Async,
}

/// Redirection
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    /// File descriptor (default 0 for <, 1 for >)
    pub fd: Option<u32>,
    /// Redirection type
    pub kind: RedirectKind,
    /// Target (filename or fd)
    pub target: Word,
}

/// Redirection types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectKind {
    Input,      // <
    Output,     // >
    Append,     // >>
    Heredoc,    // <<
    InputDup,   // <&
    OutputDup,  // >&
}
