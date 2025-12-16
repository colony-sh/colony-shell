//! POSIX shell expansion
//!
//! Implements the POSIX expansion order:
//! 1. Tilde expansion (~, ~user)
//! 2. Parameter expansion (${var}, ${var:-default}, etc.)
//! 3. Command substitution ($(cmd))
//! 4. Arithmetic expansion ($((expr)))
//! 5. Field splitting (IFS-based)
//! 6. Pathname expansion (glob)
//! 7. Quote removal
//!
//! Each expansion phase takes input from the previous phase and produces
//! output for the next. Field splitting is the first phase that can produce
//! multiple fields from one input.

mod tilde;
mod parameter;
mod command;
mod arithmetic;
mod field_split;
mod pathname;
mod quote;
mod context;

pub use context::{ExpansionContext, Variable};
pub use tilde::expand_tilde;
pub use parameter::expand_parameter;
pub use field_split::split_fields;
pub use pathname::expand_pathname;

use colony_parser::{Word, WordPart};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExpansionError {
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),

    #[error("Invalid parameter expansion: {0}")]
    InvalidParameter(String),

    #[error("Command substitution failed: {0}")]
    CommandFailed(String),

    #[error("Arithmetic error: {0}")]
    ArithmeticError(String),

    #[error("Glob pattern error: {0}")]
    GlobError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ExpansionError>;

/// A field is the result of expansion and field splitting
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The expanded text
    pub text: String,
    /// Whether this field should be subject to pathname expansion
    pub quoted: bool,
}

impl Field {
    pub fn new(text: String, quoted: bool) -> Self {
        Self { text, quoted }
    }

    pub fn quoted(text: String) -> Self {
        Self { text, quoted: true }
    }

    pub fn unquoted(text: String) -> Self {
        Self { text, quoted: false }
    }
}

/// Main expansion orchestrator
pub struct Expander {
    context: ExpansionContext,
}

impl Expander {
    pub fn new(context: ExpansionContext) -> Self {
        Self { context }
    }

    /// Expand a word into zero or more fields
    ///
    /// This executes all expansion phases in POSIX order:
    /// 1. Tilde expansion
    /// 2. Parameter expansion
    /// 3. Command substitution
    /// 4. Arithmetic expansion
    /// 5. Field splitting
    /// 6. Pathname expansion
    /// 7. Quote removal
    pub fn expand_word(&mut self, word: &Word) -> Result<Vec<Field>> {
        // Phase 1-4: Word-level expansions produce an intermediate string
        let mut intermediate = IntermediateWord::new();

        for part in &word.parts {
            self.expand_part(part, &mut intermediate)?;
        }

        // Phase 5: Field splitting - one intermediate word → multiple fields
        let fields = split_fields(&intermediate, &self.context);

        // Phase 6: Pathname expansion - each unquoted field may glob to multiple paths
        let mut expanded_fields = Vec::new();
        for field in fields {
            if field.quoted {
                expanded_fields.push(field);
            } else {
                let paths = expand_pathname(&field.text)?;
                expanded_fields.extend(paths.into_iter().map(Field::unquoted));
            }
        }

        // Phase 7: Quote removal (already handled by tracking quoted state)
        Ok(expanded_fields)
    }

    /// Expand a single word part into the intermediate representation
    fn expand_part(&mut self, part: &WordPart, out: &mut IntermediateWord) -> Result<()> {
        match part {
            WordPart::Literal(s) => {
                out.push_unquoted(s);
            }

            WordPart::SingleQuoted(s) => {
                out.push_quoted(s);
            }

            WordPart::DoubleQuoted(parts) => {
                out.begin_quoted();
                for inner_part in parts {
                    self.expand_part(inner_part, out)?;
                }
                out.end_quoted();
            }

            WordPart::Tilde(user) => {
                let expanded = expand_tilde(user.as_deref(), &self.context)?;
                out.push_unquoted(&expanded);
            }

            WordPart::Variable(var_ref) => {
                let expanded = expand_parameter(var_ref, &self.context)?;
                out.push_unquoted(&expanded);
            }

            WordPart::CommandSub(_cmd) => {
                // TODO: Execute command and capture output
                // This requires integration with executor
                out.push_unquoted("");
            }
        }

        Ok(())
    }
}

/// Intermediate representation during expansion
///
/// Tracks which portions are quoted to prevent field splitting
/// on those portions later
#[derive(Debug, Clone)]
pub(crate) struct IntermediateWord {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone)]
pub(crate) struct Segment {
    pub text: String,
    pub quoted: bool,
}

impl IntermediateWord {
    fn new() -> Self {
        Self { segments: Vec::new() }
    }

    fn push_quoted(&mut self, text: &str) {
        if let Some(last) = self.segments.last_mut() {
            if last.quoted {
                last.text.push_str(text);
                return;
            }
        }
        self.segments.push(Segment {
            text: text.to_string(),
            quoted: true,
        });
    }

    fn push_unquoted(&mut self, text: &str) {
        if let Some(last) = self.segments.last_mut() {
            if !last.quoted {
                last.text.push_str(text);
                return;
            }
        }
        self.segments.push(Segment {
            text: text.to_string(),
            quoted: false,
        });
    }

    fn begin_quoted(&mut self) {
        self.segments.push(Segment {
            text: String::new(),
            quoted: true,
        });
    }

    fn end_quoted(&mut self) {
        // Nothing to do - quoted segments are already marked
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
}
