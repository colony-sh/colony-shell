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
pub mod pathname;
mod quote;
mod context;
mod brace;

pub use context::{ExpansionContext, ShellOptions, ShoptOptions, Variable, VarAttributes, TrapSignal, TrapAction, CompletionSpec, CompletionAction};
pub use tilde::expand_tilde;
pub use parameter::{expand_parameter, expand_parameter_multi, ExpandedValue};
pub use arithmetic::expand_arithmetic;
pub use field_split::split_fields;
pub use pathname::{expand_pathname, expand_pathname_with_options};
pub use brace::expand_braces;

use colony_parser::{Word, WordPart, ProcessSubDirection};
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

/// Process substitution executor callback
///
/// Takes a statement AST and expansion context, runs the command without capturing
/// Returns exit code
pub type ProcessSubExecutor =
    Box<dyn Fn(&colony_parser::Statement, &mut ExpansionContext) -> i32>;

/// Main expansion orchestrator
pub struct Expander {
    context: ExpansionContext,
    /// Optional command executor for command substitution
    /// If None, command substitution returns empty string
    command_executor: Option<CommandExecutor>,
    /// Optional executor for process substitution (runs commands without capturing)
    process_sub_executor: Option<ProcessSubExecutor>,
}

impl Expander {
    pub fn new(context: ExpansionContext) -> Self {
        Self {
            context,
            command_executor: None,
            process_sub_executor: None,
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
            process_sub_executor: None,
        }
    }

    /// Create expander with both command and process substitution executors
    pub fn with_executors<F, G>(
        context: ExpansionContext,
        cmd_executor: F,
        procsub_executor: G,
    ) -> Self
    where
        F: Fn(&colony_parser::Statement, &mut ExpansionContext) -> std::result::Result<String, String>
            + 'static,
        G: Fn(&colony_parser::Statement, &mut ExpansionContext) -> i32 + 'static,
    {
        Self {
            context,
            command_executor: Some(Box::new(cmd_executor)),
            process_sub_executor: Some(Box::new(procsub_executor)),
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
        let shopt = self.context.shopt_options();
        for field in fields {
            if field.quoted {
                expanded_fields.push(field);
            } else {
                let paths = expand_pathname_with_options(&field.text, shopt)?;
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
    
    /// Expand a word for variable assignment RHS (no field splitting, tilde after colon)
    pub fn expand_word_for_assignment(&mut self, word: &Word) -> Result<String> {
        let mut segments: Vec<(String, bool)> = Vec::new();
        
        for part in &word.parts {
            match part {
                WordPart::Literal(s) => {
                    segments.push((s.clone(), false));
                }
                WordPart::SingleQuoted(s) => {
                    segments.push((s.clone(), true));
                }
                WordPart::DoubleQuoted(inner) => {
                    let mut inner_result = String::new();
                    for inner_part in inner {
                        match inner_part {
                            WordPart::Literal(s) => inner_result.push_str(s),
                            WordPart::Variable(var_ref) => {
                                let expanded = expand_parameter(var_ref, &self.context)?;
                                inner_result.push_str(&expanded);
                            }
                            WordPart::Tilde(user) => {
                                inner_result.push('~');
                                if let Some(u) = user {
                                    inner_result.push_str(u);
                                }
                            }
                            _ => {
                                let mut temp = IntermediateWord::new();
                                self.expand_part_with_quoted(inner_part, &mut temp, true)?;
                                inner_result.push_str(&temp.as_single_string());
                            }
                        }
                    }
                    segments.push((inner_result, true));
                }
                WordPart::Tilde(user) => {
                    let expanded = expand_tilde(user.as_deref(), &self.context)?;
                    segments.push((expanded, false));
                }
                WordPart::Variable(var_ref) => {
                    let expanded = expand_parameter(var_ref, &self.context)?;
                    segments.push((expanded, true));
                }
                _ => {
                    let mut intermediate = IntermediateWord::new();
                    self.expand_part(part, &mut intermediate)?;
                    segments.push((intermediate.as_single_string(), true));
                }
            }
        }
        
        let mut result = String::new();
        for (text, quoted) in segments {
            if quoted {
                result.push_str(&text);
            } else {
                for (i, segment) in text.split(':').enumerate() {
                    if i > 0 {
                        result.push(':');
                    }
                    if segment.starts_with('~') {
                        let slash_pos = segment.find('/').unwrap_or(segment.len());
                        let (tilde_part, rest) = segment.split_at(slash_pos);
                        let user = if tilde_part.len() > 1 { Some(&tilde_part[1..]) } else { None };
                        match expand_tilde(user, &self.context) {
                            Ok(expanded) => {
                                result.push_str(&expanded);
                                result.push_str(rest);
                            }
                            Err(_) => result.push_str(segment),
                        }
                    } else {
                        result.push_str(segment);
                    }
                }
            }
        }
        
        Ok(result)
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
                // Handle "$@" - special: each positional param becomes separate quoted field
                if var_ref.name == "@" && var_ref.subscript.is_none() && var_ref.modifier.is_none() {
                    let params = self.context.get_all_positional_params();
                    if params.is_empty() {
                        // "$@" with no params expands to nothing (not even empty string)
                        return Ok(Some(Vec::new()));
                    }
                    let fields: Vec<Field> = params.iter().map(|s| Field::quoted(s.clone())).collect();
                    return Ok(Some(fields));
                }
                
                // Handle "$*" - join with first char of IFS
                if var_ref.name == "*" && var_ref.subscript.is_none() && var_ref.modifier.is_none() {
                    let params = self.context.get_all_positional_params();
                    let ifs = self.context.ifs();
                    let sep = ifs.chars().next().unwrap_or(' ');
                    let joined: String = params.iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(&sep.to_string());
                    return Ok(Some(vec![Field::quoted(joined)]));
                }
                
                // Handle "${array[@]}" - each element becomes separate quoted field  
                // But NOT if there's a prefix like # (length) or ! (indices)
                let has_no_modifier = var_ref.modifier.is_none();
                let has_no_prefix = var_ref.prefix.is_none();
                if matches!(var_ref.subscript, Some(ArraySubscript::All)) && has_no_modifier && has_no_prefix {
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
            
            let paths = expand_pathname_with_options(&expanded, self.context.shopt_options())?;
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
                    out.push_literal(s);
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
                    out.push_literal(&expanded);
                }
            }

            WordPart::Variable(var_ref) => {
                let expanded = expand_parameter(var_ref, &self.context)?;
                if in_double_quote {
                    out.push_quoted(&expanded);
                } else {
                    out.push_expanded(&expanded);
                }
            }

            WordPart::ArithRaw(expr_str) => {
                let result = expand_arithmetic(expr_str, &self.context)?;
                if in_double_quote {
                    out.push_quoted(&result);
                } else {
                    out.push_expanded(&result);
                }
            }

            WordPart::CommandSubRaw(cmd_str) => {
                let trimmed_cmd = cmd_str.trim();
                if trimmed_cmd.is_empty() {
                } else if let Some(ref executor) = self.command_executor {
                    match colony_parser::pest_api::parse(trimmed_cmd) {
                        Ok(program) => {
                            let mut combined_output = String::new();
                            for stmt in &program.statements {
                                match executor(stmt, &mut self.context) {
                                    Ok(output) => {
                                        combined_output.push_str(&output);
                                    }
                                    Err(e) => {
                                        return Err(ExpansionError::CommandFailed(e));
                                    }
                                }
                            }
                            let trimmed = combined_output.trim_end_matches('\n');
                            if in_double_quote {
                                out.push_quoted(trimmed);
                            } else {
                                out.push_expanded(trimmed);
                            }
                        }
                        Err(e) => {
                            return Err(ExpansionError::CommandFailed(format!("parse error: {}", e)));
                        }
                    }
                } else {
                    out.push_expanded("");
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
                                out.push_expanded(trimmed);
                            }
                        }
                        Err(e) => {
                            return Err(ExpansionError::CommandFailed(e));
                        }
                    }
                } else {
                    out.push_expanded("");
                }
            }
            
            WordPart::BraceExpansion(content) => {
                if in_double_quote {
                    out.push_quoted(content);
                } else {
                    out.push_literal(content);
                }
            }
            
            WordPart::AnsiCQuoted(content) => {
                let expanded = expand_ansi_c_string(content);
                out.push_quoted(&expanded);
            }
            
            WordPart::ProcessSub { direction, command } => {
                if let Some(ref procsub_exec) = self.process_sub_executor {
                    let path = expand_process_sub(*direction, command, procsub_exec, &mut self.context)?;
                    if in_double_quote {
                        out.push_quoted(&path);
                    } else {
                        out.push_literal(&path);
                    }
                } else if let Some(ref executor) = self.command_executor {
                    let path = expand_process_sub_fallback(*direction, command, executor, &mut self.context)?;
                    if in_double_quote {
                        out.push_quoted(&path);
                    } else {
                        out.push_literal(&path);
                    }
                } else {
                    return Err(ExpansionError::CommandFailed(
                        "Process substitution requires command executor".to_string()
                    ));
                }
            }
            
            WordPart::ExtGlob { operator, pattern_list } => {
                let op_char = match operator {
                    colony_parser::ExtGlobOp::ZeroOrOne => '?',
                    colony_parser::ExtGlobOp::ZeroOrMore => '*',
                    colony_parser::ExtGlobOp::OneOrMore => '+',
                    colony_parser::ExtGlobOp::ExactlyOne => '@',
                    colony_parser::ExtGlobOp::Not => '!',
                };
                let reconstructed = format!("{}({})", op_char, pattern_list);
                if in_double_quote {
                    out.push_quoted(&reconstructed);
                } else {
                    out.push_literal(&reconstructed);
                }
            }
        }

        Ok(())
    }
}

/// Intermediate representation during expansion
#[derive(Debug, Clone)]
pub(crate) struct IntermediateWord {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentKind {
    Quoted,
    Literal,
    Expanded,
}

#[derive(Debug, Clone)]
pub(crate) struct Segment {
    pub text: String,
    pub kind: SegmentKind,
}

impl Segment {
    pub fn quoted(&self) -> bool {
        self.kind == SegmentKind::Quoted
    }
    
    pub fn should_field_split(&self) -> bool {
        self.kind == SegmentKind::Expanded
    }
    
    pub fn should_glob(&self) -> bool {
        self.kind != SegmentKind::Quoted
    }
}

impl IntermediateWord {
    fn new() -> Self {
        Self { segments: Vec::new() }
    }

    fn push_quoted(&mut self, text: &str) {
        if let Some(last) = self.segments.last_mut() {
            if last.kind == SegmentKind::Quoted {
                last.text.push_str(text);
                return;
            }
        }
        self.segments.push(Segment {
            text: text.to_string(),
            kind: SegmentKind::Quoted,
        });
    }
    
    fn push_literal(&mut self, text: &str) {
        if let Some(last) = self.segments.last_mut() {
            if last.kind == SegmentKind::Literal {
                last.text.push_str(text);
                return;
            }
        }
        self.segments.push(Segment {
            text: text.to_string(),
            kind: SegmentKind::Literal,
        });
    }

    fn push_expanded(&mut self, text: &str) {
        if let Some(last) = self.segments.last_mut() {
            if last.kind == SegmentKind::Expanded {
                last.text.push_str(text);
                return;
            }
        }
        self.segments.push(Segment {
            text: text.to_string(),
            kind: SegmentKind::Expanded,
        });
    }

    fn begin_quoted(&mut self) {
        self.segments.push(Segment {
            text: String::new(),
            kind: SegmentKind::Quoted,
        });
    }

    fn end_quoted(&mut self) {
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    fn as_single_string(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }
}

fn expand_ansi_c_string(content: &str) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                chars.next();
                match next {
                    'a' => result.push('\x07'),
                    'b' => result.push('\x08'),
                    'e' | 'E' => result.push('\x1b'),
                    'f' => result.push('\x0c'),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    'v' => result.push('\x0b'),
                    '\\' => result.push('\\'),
                    '\'' => result.push('\''),
                    '"' => result.push('"'),
                    '?' => result.push('?'),
                    'x' => {
                        let mut hex = String::new();
                        for _ in 0..2 {
                            if let Some(&c) = chars.peek() {
                                if c.is_ascii_hexdigit() {
                                    hex.push(chars.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                        }
                        if !hex.is_empty() {
                            if let Ok(val) = u8::from_str_radix(&hex, 16) {
                                result.push(val as char);
                            }
                        }
                    }
                    'u' => {
                        let mut hex = String::new();
                        for _ in 0..4 {
                            if let Some(&c) = chars.peek() {
                                if c.is_ascii_hexdigit() {
                                    hex.push(chars.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                        }
                        if !hex.is_empty() {
                            if let Ok(val) = u32::from_str_radix(&hex, 16) {
                                if let Some(c) = char::from_u32(val) {
                                    result.push(c);
                                }
                            }
                        }
                    }
                    'U' => {
                        let mut hex = String::new();
                        for _ in 0..8 {
                            if let Some(&c) = chars.peek() {
                                if c.is_ascii_hexdigit() {
                                    hex.push(chars.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                        }
                        if !hex.is_empty() {
                            if let Ok(val) = u32::from_str_radix(&hex, 16) {
                                if let Some(c) = char::from_u32(val) {
                                    result.push(c);
                                }
                            }
                        }
                    }
                    'c' => {
                        if let Some(ctrl_char) = chars.next() {
                            let ctrl_val = (ctrl_char.to_ascii_uppercase() as u8) & 0x1f;
                            result.push(ctrl_val as char);
                        }
                    }
                    '0'..='7' => {
                        let mut octal = String::new();
                        octal.push(next);
                        for _ in 0..2 {
                            if let Some(&c) = chars.peek() {
                                if ('0'..='7').contains(&c) {
                                    octal.push(chars.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                        }
                        if let Ok(val) = u8::from_str_radix(&octal, 8) {
                            result.push(val as char);
                        }
                    }
                    _ => {
                        result.push(next);
                    }
                }
            } else {
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }
    
    result
}

fn expand_process_sub(
    direction: ProcessSubDirection,
    command: &str,
    executor: &ProcessSubExecutor,
    context: &mut ExpansionContext,
) -> Result<String> {
    use std::os::unix::io::AsRawFd;
    
    let trimmed_cmd = command.trim();
    if trimmed_cmd.is_empty() {
        return Err(ExpansionError::CommandFailed("Empty process substitution".to_string()));
    }
    
    let (read_fd, write_fd) = nix::unistd::pipe()
        .map_err(|e| ExpansionError::CommandFailed(format!("pipe() failed: {}", e)))?;
    
    match unsafe { nix::unistd::fork() } {
        Ok(nix::unistd::ForkResult::Child) => {
            match direction {
                ProcessSubDirection::Input => {
                    drop(read_fd);
                    nix::unistd::dup2(write_fd.as_raw_fd(), 1)
                        .expect("dup2 failed");
                    drop(write_fd);
                }
                ProcessSubDirection::Output => {
                    drop(write_fd);
                    nix::unistd::dup2(read_fd.as_raw_fd(), 0)
                        .expect("dup2 failed");
                    drop(read_fd);
                }
            }
            
            let exit_code = match colony_parser::pest_api::parse(trimmed_cmd) {
                Ok(program) => {
                    let mut last_code = 0;
                    for stmt in &program.statements {
                        last_code = executor(stmt, context);
                    }
                    last_code
                }
                Err(_) => 1,
            };
            
            std::process::exit(exit_code);
        }
        Ok(nix::unistd::ForkResult::Parent { child: _ }) => {
            match direction {
                ProcessSubDirection::Input => {
                    drop(write_fd);
                    let fd_num = read_fd.as_raw_fd();
                    std::mem::forget(read_fd);
                    Ok(format!("/dev/fd/{}", fd_num))
                }
                ProcessSubDirection::Output => {
                    drop(read_fd);
                    let fd_num = write_fd.as_raw_fd();
                    std::mem::forget(write_fd);
                    Ok(format!("/dev/fd/{}", fd_num))
                }
            }
        }
        Err(e) => {
            Err(ExpansionError::CommandFailed(format!("fork() failed: {}", e)))
        }
    }
}

fn expand_process_sub_fallback(
    direction: ProcessSubDirection,
    command: &str,
    executor: &CommandExecutor,
    context: &mut ExpansionContext,
) -> Result<String> {
    use std::os::unix::io::AsRawFd;
    use std::io::Write;
    
    let trimmed_cmd = command.trim();
    if trimmed_cmd.is_empty() {
        return Err(ExpansionError::CommandFailed("Empty process substitution".to_string()));
    }
    
    let (read_fd, write_fd) = nix::unistd::pipe()
        .map_err(|e| ExpansionError::CommandFailed(format!("pipe() failed: {}", e)))?;
    
    match unsafe { nix::unistd::fork() } {
        Ok(nix::unistd::ForkResult::Child) => {
            match direction {
                ProcessSubDirection::Input => {
                    drop(read_fd);
                    nix::unistd::dup2(write_fd.as_raw_fd(), 1)
                        .expect("dup2 failed");
                    drop(write_fd);
                }
                ProcessSubDirection::Output => {
                    drop(write_fd);
                    nix::unistd::dup2(read_fd.as_raw_fd(), 0)
                        .expect("dup2 failed");
                    drop(read_fd);
                }
            }
            
            match colony_parser::pest_api::parse(trimmed_cmd) {
                Ok(program) => {
                    for stmt in &program.statements {
                        match executor(stmt, context) {
                            Ok(output) => {
                                let _ = std::io::stdout().write_all(output.as_bytes());
                                let _ = std::io::stdout().flush();
                            }
                            Err(_) => {}
                        }
                    }
                }
                Err(_) => {}
            }
            
            std::process::exit(0);
        }
        Ok(nix::unistd::ForkResult::Parent { child: _ }) => {
            match direction {
                ProcessSubDirection::Input => {
                    drop(write_fd);
                    let fd_num = read_fd.as_raw_fd();
                    std::mem::forget(read_fd);
                    Ok(format!("/dev/fd/{}", fd_num))
                }
                ProcessSubDirection::Output => {
                    drop(read_fd);
                    let fd_num = write_fd.as_raw_fd();
                    std::mem::forget(write_fd);
                    Ok(format!("/dev/fd/{}", fd_num))
                }
            }
        }
        Err(e) => {
            Err(ExpansionError::CommandFailed(format!("fork() failed: {}", e)))
        }
    }
}
