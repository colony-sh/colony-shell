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
mod brace;

pub use context::{ExpansionContext, Variable};
pub use tilde::expand_tilde;
pub use parameter::{expand_parameter, expand_parameter_multi, ExpandedValue};
pub use arithmetic::expand_arithmetic;
pub use field_split::split_fields;
pub use pathname::expand_pathname;
pub use brace::expand_braces;

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

/// Command executor callback
///
/// Takes a statement AST and expansion context, returns stdout output
pub type CommandExecutor =
    Box<dyn Fn(&colony_parser::Statement, &mut ExpansionContext) -> std::result::Result<String, String>>;

/// Main expansion orchestrator
pub struct Expander {
    context: ExpansionContext,
    /// Optional command executor for command substitution
    /// If None, command substitution returns empty string
    command_executor: Option<CommandExecutor>,
}

impl Expander {
    pub fn new(context: ExpansionContext) -> Self {
        Self {
            context,
            command_executor: None,
        }
    }

    /// Create expander with command executor for command substitution
    pub fn with_executor<F>(context: ExpansionContext, executor: F) -> Self
    where
        F: Fn(&colony_parser::Statement, &mut ExpansionContext) -> std::result::Result<String, String>
            + 'static,
    {
        Self {
            context,
            command_executor: Some(Box::new(executor)),
        }
    }

    pub fn expand_word(&mut self, word: &Word) -> Result<Vec<Field>> {
        if let Some(fields) = self.try_expand_array_at_word(word)? {
            return Ok(fields);
        }
        
        if let Some(brace_results) = self.try_expand_brace_word(word)? {
            return Ok(brace_results);
        }
        
        let mut intermediate = IntermediateWord::new();

        for part in &word.parts {
            self.expand_part(part, &mut intermediate)?;
        }

        let fields = split_fields(&intermediate, &self.context);

        let mut expanded_fields = Vec::new();
        for field in fields {
            if field.quoted {
                expanded_fields.push(field);
            } else {
                let paths = expand_pathname(&field.text)?;
                expanded_fields.extend(paths.into_iter().map(Field::unquoted));
            }
        }

        Ok(expanded_fields)
    }
    
    pub fn expand_word_no_glob(&mut self, word: &Word) -> Result<Vec<Field>> {
        if let Some(fields) = self.try_expand_array_at_word(word)? {
            return Ok(fields);
        }
        
        if let Some(brace_results) = self.try_expand_brace_word(word)? {
            return Ok(brace_results);
        }
        
        let mut intermediate = IntermediateWord::new();

        for part in &word.parts {
            self.expand_part(part, &mut intermediate)?;
        }

        let fields = split_fields(&intermediate, &self.context);
        Ok(fields)
    }
    
    /// Expand a word for variable assignment RHS
    /// POSIX: No field splitting or pathname expansion on assignment RHS
    pub fn expand_word_for_assignment(&mut self, word: &Word) -> Result<String> {
        let mut intermediate = IntermediateWord::new();

        for part in &word.parts {
            self.expand_part(part, &mut intermediate)?;
        }

        Ok(intermediate.as_single_string())
    }
    
    fn try_expand_array_at_word(&self, word: &Word) -> Result<Option<Vec<Field>>> {
        use colony_parser::ArraySubscript;
        
        if word.parts.len() != 1 {
            return Ok(None);
        }
        
        if let WordPart::DoubleQuoted(inner_parts) = &word.parts[0] {
            if inner_parts.len() != 1 {
                return Ok(None);
            }
            if let WordPart::Variable(var_ref) = &inner_parts[0] {
                if matches!(var_ref.subscript, Some(ArraySubscript::All)) {
                    if let Some(Variable::Array(arr)) = self.context.get_variable(&var_ref.name) {
                        let fields: Vec<Field> = arr.iter().map(|s| Field::quoted(s.clone())).collect();
                        return Ok(Some(fields));
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    fn try_expand_brace_word(&mut self, word: &Word) -> Result<Option<Vec<Field>>> {
        let mut brace_parts: Vec<Vec<String>> = Vec::new();
        let mut literal_parts: Vec<String> = Vec::new();
        let mut current_literal = String::new();
        
        for part in &word.parts {
            match part {
                WordPart::BraceExpansion(content) => {
                    if !current_literal.is_empty() {
                        literal_parts.push(current_literal.clone());
                        current_literal.clear();
                    } else if literal_parts.is_empty() || brace_parts.len() >= literal_parts.len() {
                        literal_parts.push(String::new());
                    }
                    
                    let inner = content.trim_start_matches('{').trim_end_matches('}');
                    let expansions = brace::expand_braces(inner);
                    brace_parts.push(expansions);
                }
                WordPart::Literal(s) | WordPart::SingleQuoted(s) => {
                    current_literal.push_str(s);
                }
                _ => {
                    return Ok(None);
                }
            }
        }
        
        if brace_parts.is_empty() {
            return Ok(None);
        }
        
        if !current_literal.is_empty() {
            literal_parts.push(current_literal);
        }
        
        fn cartesian_product(lists: &[Vec<String>]) -> Vec<Vec<String>> {
            if lists.is_empty() {
                return vec![vec![]];
            }
            
            let first = &lists[0];
            let rest = cartesian_product(&lists[1..]);
            
            let mut result = Vec::new();
            for item in first {
                for r in &rest {
                    let mut combo = vec![item.clone()];
                    combo.extend(r.iter().cloned());
                    result.push(combo);
                }
            }
            result
        }
        
        let combinations = cartesian_product(&brace_parts);
        
        let mut results = Vec::new();
        for combo in combinations {
            let mut expanded = String::new();
            for (i, brace_val) in combo.iter().enumerate() {
                if i < literal_parts.len() {
                    expanded.push_str(&literal_parts[i]);
                }
                expanded.push_str(brace_val);
            }
            if literal_parts.len() > combo.len() {
                expanded.push_str(&literal_parts[combo.len()]);
            }
            
            let paths = expand_pathname(&expanded)?;
            for path in paths {
                results.push(Field::unquoted(path));
            }
        }
        
        Ok(Some(results))
    }

    fn expand_part(&mut self, part: &WordPart, out: &mut IntermediateWord) -> Result<()> {
        self.expand_part_with_quoted(part, out, false)
    }

    fn expand_part_with_quoted(&mut self, part: &WordPart, out: &mut IntermediateWord, in_double_quote: bool) -> Result<()> {
        match part {
            WordPart::Literal(s) => {
                if in_double_quote {
                    out.push_quoted(s);
                } else {
                    out.push_unquoted(s);
                }
            }

            WordPart::SingleQuoted(s) => {
                out.push_quoted(s);
            }

            WordPart::DoubleQuoted(parts) => {
                out.begin_quoted();
                for inner_part in parts {
                    self.expand_part_with_quoted(inner_part, out, true)?;
                }
                out.end_quoted();
            }

            WordPart::Tilde(user) => {
                let expanded = expand_tilde(user.as_deref(), &self.context)?;
                if in_double_quote {
                    out.push_quoted(&expanded);
                } else {
                    out.push_unquoted(&expanded);
                }
            }

            WordPart::Variable(var_ref) => {
                let expanded = expand_parameter(var_ref, &self.context)?;
                if in_double_quote {
                    out.push_quoted(&expanded);
                } else {
                    out.push_unquoted(&expanded);
                }
            }

            WordPart::ArithRaw(expr_str) => {
                let result = expand_arithmetic(expr_str, &self.context)?;
                if in_double_quote {
                    out.push_quoted(&result);
                } else {
                    out.push_unquoted(&result);
                }
            }

            WordPart::CommandSubRaw(cmd_str) => {
                if let Some(ref executor) = self.command_executor {
                    match colony_parser::pest_api::parse_statement(cmd_str) {
                        Ok(stmt) => {
                            match executor(&stmt, &mut self.context) {
                                Ok(output) => {
                                    let trimmed = output.trim_end_matches('\n');
                                    if in_double_quote {
                                        out.push_quoted(trimmed);
                                    } else {
                                        out.push_unquoted(trimmed);
                                    }
                                }
                                Err(e) => {
                                    return Err(ExpansionError::CommandFailed(e));
                                }
                            }
                        }
                        Err(e) => {
                            return Err(ExpansionError::CommandFailed(format!("parse error: {}", e)));
                        }
                    }
                } else {
                    out.push_unquoted("");
                }
            }

            WordPart::CommandSub(cmd) => {
                if let Some(ref executor) = self.command_executor {
                    match executor(cmd, &mut self.context) {
                        Ok(output) => {
                            let trimmed = output.trim_end_matches('\n');
                            if in_double_quote {
                                out.push_quoted(trimmed);
                            } else {
                                out.push_unquoted(trimmed);
                            }
                        }
                        Err(e) => {
                            return Err(ExpansionError::CommandFailed(e));
                        }
                    }
                } else {
                    out.push_unquoted("");
                }
            }
            
            WordPart::BraceExpansion(content) => {
                if in_double_quote {
                    out.push_quoted(content);
                } else {
                    out.push_unquoted(content);
                }
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

    fn as_single_string(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }
}
