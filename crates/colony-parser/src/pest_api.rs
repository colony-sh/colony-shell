//! Public API for pest-based parser
//!
//! This module provides the main entry point for parsing Colony shell source
//! directly into AST using pure PEG (no preprocessing required for most constructs).
//!
//! # Architecture
//!
//! Pure PEG approach - pest operates on source text directly:
//! ```text
//! Source Text → pest Grammar → Parse Tree → AST Builder → AST
//! ```
//!
//! Context sensitivity is handled by **grammar structure**:
//! - `SimpleCommand` expects `Word` (barewords allowed)
//! - `Expression` expects `Identifier` (stricter)
//! - `<` is redirect in `SimpleCommand`, comparison in `Expression`
//!
//! # What Doesn't Need Preprocessing
//!
//! - ✅ Context-sensitive barewords (`echo hello` vs `var x = hello`)
//! - ✅ Context-sensitive operators (`<` as redirect vs comparison)
//! - ✅ Variable expansions (`$var`, `${var}`)
//! - ✅ Command substitution (`$(cmd)`, nested)
//! - ✅ Quote interpolation (`"Count: $count"`)
//! - ✅ Nested structures (`"outer $(echo "inner")"`)
//!
//! # What Still Needs Preprocessing
//!
//! - ⏳ Heredocs (content appears after command line)
//! - ⏳ Complex escape sequences in some contexts
//!
//! See `docs/peg-proposal.md` for full architectural discussion.

use crate::ast::Program;
use crate::builder::AstBuilder;
use crate::heredoc::preprocess_heredocs;
use crate::pest_parser::{ColonyParser, Rule};
use pest::Parser;

/// Parse Colony shell source code into AST
///
/// This is the main entry point for parsing. It operates directly on
/// source text using the pest PEG grammar, with minimal preprocessing
/// for heredocs and arithmetic expansions.
///
/// # Heredoc Handling
///
/// Heredocs (`<<EOF`) require preprocessing because their content appears
/// after the command line. This function automatically detects and handles them.
///
/// # Arithmetic Expansion Handling
///
/// Arithmetic expansions (`$((expr))`) with nested parentheses require
/// preprocessing because the pest grammar cannot handle balanced parens inside.
///
/// # Example
/// ```ignore
/// use colony_parser::parse;
///
/// let source = r#"
///     if (x > 10) {
///         echo "big"
///     }
/// "#;
///
/// let program = parse(source)?;
/// assert_eq!(program.statements.len(), 1);
/// ```
///
/// # Errors
///
/// Returns `ParseError` if:
/// - Syntax is invalid (pest parsing fails)
/// - AST building fails (malformed parse tree)
/// - Heredoc preprocessing fails
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let source = preprocess_line_continuations(source);

    let (arith_normalized, arith_map) = if source.contains("$((") {
        crate::arith_preprocess::preprocess_arithmetic(&source)
    } else {
        (source.to_string(), Vec::new())
    };

    let mut program = if arith_normalized.contains("<<") {
        parse_with_heredocs(&arith_normalized)?
    } else {
        parse_direct(&arith_normalized)?
    };

    if !arith_map.is_empty() {
        for stmt in &mut program.statements {
            inject_arithmetic(stmt, &arith_map);
        }
    }

    Ok(program)
}

/// Parse with alias expansion (POSIX 2.3.1)
///
/// This function performs parse-time alias expansion before parsing.
/// Aliases are expanded at command positions (after ;, |, &&, ||, etc.)
pub fn parse_with_aliases(
    source: &str,
    alias_ctx: &mut crate::alias_expand::AliasContext,
) -> Result<Program, ParseError> {
    let source = preprocess_line_continuations(source);
    let source = crate::alias_expand::preprocess_aliases(&source, alias_ctx);

    let (arith_normalized, arith_map) = if source.contains("$((") {
        crate::arith_preprocess::preprocess_arithmetic(&source)
    } else {
        (source.to_string(), Vec::new())
    };

    let mut program = if arith_normalized.contains("<<") {
        parse_with_heredocs(&arith_normalized)?
    } else {
        parse_direct(&arith_normalized)?
    };

    if !arith_map.is_empty() {
        for stmt in &mut program.statements {
            inject_arithmetic(stmt, &arith_map);
        }
    }

    Ok(program)
}

/// Parse source directly with pest (no heredocs)
fn parse_direct(source: &str) -> Result<Program, ParseError> {
    // Parse source directly with pest
    let pairs = ColonyParser::parse(Rule::Program, source).map_err(|e| {
        let (line, column) = match e.line_col {
            pest::error::LineColLocation::Pos((l, c)) => (l, c),
            pest::error::LineColLocation::Span((l, c), _) => (l, c),
        };
        ParseError::Syntax {
            message: e.to_string(),
            line,
            column,
        }
    })?;

    // Build AST from parse tree
    let program = AstBuilder::build_program(pairs).map_err(ParseError::Build)?;

    Ok(program)
}

fn parse_with_heredocs(source: &str) -> Result<Program, ParseError> {
    let (normalized, heredoc_contents) =
        preprocess_heredocs(source).map_err(ParseError::HeredocPreprocessing)?;

    let pairs = ColonyParser::parse(Rule::Program, &normalized).map_err(|e| {
        let (line, column) = match e.line_col {
            pest::error::LineColLocation::Pos((l, c)) => (l, c),
            pest::error::LineColLocation::Span((l, c), _) => (l, c),
        };
        ParseError::Syntax {
            message: e.to_string(),
            line,
            column,
        }
    })?;

    let mut program = AstBuilder::build_program(pairs).map_err(ParseError::Build)?;

    let mut heredoc_index = 0;
    for stmt in &mut program.statements {
        inject_heredoc_contents(stmt, &heredoc_contents, &mut heredoc_index);
    }

    Ok(program)
}

pub fn parse_statement(source: &str) -> Result<crate::ast::Statement, ParseError> {
    let (normalized_arith, _arith_map) = if source.contains("$((") {
        crate::arith_preprocess::preprocess_arithmetic(source)
    } else {
        (source.to_string(), Vec::new())
    };

    let (normalized, heredoc_contents) = if normalized_arith.contains("<<") {
        preprocess_heredocs(&normalized_arith).map_err(ParseError::HeredocPreprocessing)?
    } else {
        (normalized_arith, std::collections::HashMap::new())
    };

    let pairs = ColonyParser::parse(Rule::Statement, &normalized).map_err(|e| {
        let (line, column) = match e.line_col {
            pest::error::LineColLocation::Pos((l, c)) => (l, c),
            pest::error::LineColLocation::Span((l, c), _) => (l, c),
        };
        ParseError::Syntax {
            message: e.to_string(),
            line,
            column,
        }
    })?;

    let pair = pairs.into_iter().next().ok_or(ParseError::EmptyInput)?;

    let inner = pair.into_inner().next().ok_or(ParseError::EmptyInput)?;

    let mut stmt = AstBuilder::build_statement(inner).map_err(ParseError::Build)?;

    if !_arith_map.is_empty() {
        inject_arithmetic(&mut stmt, &_arith_map);
    }

    if !heredoc_contents.is_empty() {
        let mut heredoc_index = 0;
        inject_heredoc_contents(&mut stmt, &heredoc_contents, &mut heredoc_index);
    }

    Ok(stmt)
}

/// Parse a single statement with alias expansion (POSIX 2.3.1)
///
/// This function performs parse-time alias expansion before parsing.
/// Aliases are expanded at command positions (after ;, |, &&, ||, etc.)
pub fn parse_statement_with_aliases(
    source: &str,
    alias_ctx: &mut crate::alias_expand::AliasContext,
) -> Result<crate::ast::Statement, ParseError> {
    let source = preprocess_line_continuations(source);
    let source = crate::alias_expand::preprocess_aliases(&source, alias_ctx);

    let (normalized_arith, _arith_map) = if source.contains("$((") {
        crate::arith_preprocess::preprocess_arithmetic(&source)
    } else {
        (source.to_string(), Vec::new())
    };

    let (normalized, heredoc_contents) = if normalized_arith.contains("<<") {
        preprocess_heredocs(&normalized_arith).map_err(ParseError::HeredocPreprocessing)?
    } else {
        (normalized_arith, std::collections::HashMap::new())
    };

    let pairs = ColonyParser::parse(Rule::Statement, &normalized).map_err(|e| {
        let (line, column) = match e.line_col {
            pest::error::LineColLocation::Pos((l, c)) => (l, c),
            pest::error::LineColLocation::Span((l, c), _) => (l, c),
        };
        ParseError::Syntax {
            message: e.to_string(),
            line,
            column,
        }
    })?;

    let pair = pairs.into_iter().next().ok_or(ParseError::EmptyInput)?;

    let inner = pair.into_inner().next().ok_or(ParseError::EmptyInput)?;

    let mut stmt = AstBuilder::build_statement(inner).map_err(ParseError::Build)?;

    if !_arith_map.is_empty() {
        inject_arithmetic(&mut stmt, &_arith_map);
    }

    if !heredoc_contents.is_empty() {
        let mut heredoc_index = 0;
        inject_heredoc_contents(&mut stmt, &heredoc_contents, &mut heredoc_index);
    }

    Ok(stmt)
}

/// Parse an expression (for evaluation)
pub fn parse_expression(source: &str) -> Result<crate::ast::Expression, ParseError> {
    let pairs = ColonyParser::parse(Rule::Expression, source).map_err(|e| {
        let (line, column) = match e.line_col {
            pest::error::LineColLocation::Pos((l, c)) => (l, c),
            pest::error::LineColLocation::Span((l, c), _) => (l, c),
        };
        ParseError::Syntax {
            message: e.to_string(),
            line,
            column,
        }
    })?;

    let pair = pairs.into_iter().next().ok_or(ParseError::EmptyInput)?;

    AstBuilder::build_expression(pair).map_err(ParseError::Build)
}

fn preprocess_line_continuations(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_heredoc: Option<(String, bool)> = None; // (delimiter, is_quoted_delimiter)

    while i < bytes.len() {
        // Check for heredoc start
        if in_heredoc.is_none()
            && !in_single_quote
            && i + 1 < bytes.len()
            && bytes[i] == b'<'
            && bytes[i + 1] == b'<'
        {
            let start = i;
            i += 2;
            if i < bytes.len() && bytes[i] == b'-' {
                i += 1;
            }
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            let delim_start = i;
            if i < bytes.len() && (bytes[i] == b'\'' || bytes[i] == b'"') {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                let delim = String::from_utf8_lossy(&bytes[delim_start + 1..i]).to_string();
                if i < bytes.len() {
                    i += 1;
                }
                in_heredoc = Some((delim, true));
            } else {
                while i < bytes.len()
                    && bytes[i] != b'\n'
                    && bytes[i] != b' '
                    && bytes[i] != b'\t'
                    && bytes[i] != b';'
                    && bytes[i] != b'&'
                    && bytes[i] != b'|'
                    && bytes[i] != b'>'
                    && bytes[i] != b'<'
                    && bytes[i] != b')'
                {
                    i += 1;
                }
                let delim = String::from_utf8_lossy(&bytes[delim_start..i]).to_string();
                if !delim.is_empty() {
                    in_heredoc = Some((delim, false));
                }
            }
            result.push_str(&String::from_utf8_lossy(&bytes[start..i]));
            continue;
        }

        // Check for heredoc end
        if let Some((ref delim, _is_quoted)) = in_heredoc {
            if bytes[i] == b'\n' {
                result.push('\n');
                i += 1;
                let line_start = i;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                let line = String::from_utf8_lossy(&bytes[line_start..i]).to_string();
                if line.trim() == delim {
                    in_heredoc = None;
                }
                result.push_str(&line);
                continue;
            }
            result.push(bytes[i] as char);
            i += 1;
            continue;
        }

        if bytes[i] == b'\'' && !in_single_quote {
            in_single_quote = true;
            result.push('\'');
            i += 1;
            continue;
        }

        if bytes[i] == b'\'' && in_single_quote {
            in_single_quote = false;
            result.push('\'');
            i += 1;
            continue;
        }

        if in_single_quote {
            result.push(bytes[i] as char);
            i += 1;
            continue;
        }

        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
            i += 2;
            continue;
        }

        result.push(bytes[i] as char);
        i += 1;
    }

    result
}

/// Inject arithmetic expressions back into AST
///
/// Walks the AST and replaces ${__ARITH_N__} variable references
/// with ArithRaw(expr) WordParts.
fn inject_arithmetic(stmt: &mut crate::ast::Statement, arith_map: &[(String, String)]) {
    use crate::ast::{Statement, WordPart};

    fn inject_into_word(word: &mut crate::ast::Word, arith_map: &[(String, String)]) {
        for part in &mut word.parts {
            match part {
                WordPart::Variable(var_ref) => {
                    if var_ref.name.starts_with("__ARITH_") && var_ref.modifier.is_none() {
                        for (placeholder, expr) in arith_map {
                            if var_ref.name == *placeholder {
                                *part = WordPart::ArithRaw(expr.clone());
                                break;
                            }
                        }
                    }
                }
                WordPart::DoubleQuoted(parts) => {
                    // Recursively inject into double-quoted parts
                    for inner_part in parts {
                        if let WordPart::Variable(var_ref) = inner_part {
                            if var_ref.name.starts_with("__ARITH_") {
                                for (placeholder, expr) in arith_map {
                                    if var_ref.name == *placeholder {
                                        *inner_part = WordPart::ArithRaw(expr.clone());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Helper to inject into an assignment value
    fn inject_into_assignment(
        assignment: &mut crate::ast::Assignment,
        arith_map: &[(String, String)],
    ) {
        use crate::ast::AssignmentValue;
        match &mut assignment.value {
            AssignmentValue::Word(word) => {
                inject_into_word(word, arith_map);
            }
            AssignmentValue::Array(words) => {
                for word in words {
                    inject_into_word(word, arith_map);
                }
            }
            AssignmentValue::AssocArray(_) => {
                // Assoc arrays use string literals, no injection needed
            }
        }
    }

    // Walk the statement tree
    match stmt {
        Statement::SimpleCommand(cmd) => {
            inject_into_word(&mut cmd.name, arith_map);
            for arg in &mut cmd.args {
                inject_into_word(arg, arith_map);
            }
            // Also inject into env_assignments!
            for assignment in &mut cmd.env_assignments {
                inject_into_assignment(assignment, arith_map);
            }
        }
        Statement::Pipeline(pipeline) => {
            for element in &mut pipeline.elements {
                inject_arithmetic(&mut element.command, arith_map);
            }
        }
        Statement::List(list) => {
            inject_arithmetic(&mut list.first, arith_map);
            for (_, stmt) in &mut list.rest {
                inject_arithmetic(stmt, arith_map);
            }
        }
        Statement::If(if_stmt) => {
            for stmt in &mut if_stmt.then_body {
                inject_arithmetic(stmt, arith_map);
            }
            for elif in &mut if_stmt.elif_branches {
                for stmt in &mut elif.body {
                    inject_arithmetic(stmt, arith_map);
                }
            }
            if let Some(else_body) = &mut if_stmt.else_body {
                for stmt in else_body {
                    inject_arithmetic(stmt, arith_map);
                }
            }
        }
        Statement::For(for_stmt) => {
            for word in &mut for_stmt.words {
                inject_into_word(word, arith_map);
            }
            for stmt in &mut for_stmt.body {
                inject_arithmetic(stmt, arith_map);
            }
        }
        Statement::While(while_stmt) => {
            for stmt in &mut while_stmt.body {
                inject_arithmetic(stmt, arith_map);
            }
        }
        Statement::Until(until_stmt) => {
            for stmt in &mut until_stmt.body {
                inject_arithmetic(stmt, arith_map);
            }
        }
        Statement::Case(case_stmt) => {
            inject_into_word(&mut case_stmt.value, arith_map);
            for arm in &mut case_stmt.arms {
                for pattern in &mut arm.patterns {
                    inject_into_word(pattern, arith_map);
                }
                for stmt in &mut arm.body {
                    inject_arithmetic(stmt, arith_map);
                }
            }
        }
        Statement::Subshell(subshell) => {
            for stmt in &mut subshell.body {
                inject_arithmetic(stmt, arith_map);
            }
        }
        Statement::BraceGroup(group) => {
            for stmt in &mut group.body {
                inject_arithmetic(stmt, arith_map);
            }
        }
        Statement::FunctionDef(func) => {
            for stmt in &mut func.body {
                inject_arithmetic(stmt, arith_map);
            }
        }
        _ => {}
    }
}

fn inject_heredoc_contents(
    stmt: &mut crate::ast::Statement,
    heredoc_contents: &std::collections::HashMap<usize, crate::heredoc::HeredocContent>,
    heredoc_index: &mut usize,
) {
    use crate::ast::{HereDocKind, RedirectKind, RedirectTarget, Statement};

    fn inject_into_redirects(
        redirects: &mut Vec<crate::ast::Redirect>,
        heredoc_contents: &std::collections::HashMap<usize, crate::heredoc::HeredocContent>,
        heredoc_index: &mut usize,
    ) {
        use crate::ast::WordPart;

        for redirect in redirects {
            match (&redirect.kind, &redirect.target) {
                (RedirectKind::HereDoc(HereDocKind::Normal), RedirectTarget::File(word))
                | (RedirectKind::HereDoc(HereDocKind::StripTabs), RedirectTarget::File(word)) => {
                    let delimiter_key = if word.parts.len() == 1 {
                        match &word.parts[0] {
                            WordPart::Literal(s) => Some(s.as_str()),
                            WordPart::SingleQuoted(s) => Some(s.as_str()),
                            WordPart::DoubleQuoted(parts) if parts.len() == 1 => {
                                if let WordPart::Literal(s) = &parts[0] {
                                    Some(s.as_str())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };

                    if let Some(delimiter_key) = delimiter_key {
                        if let Some(content) = heredoc_contents.get(heredoc_index) {
                            redirect.target = RedirectTarget::HereDoc {
                                delimiter: delimiter_key.to_string(),
                                content: content.content.clone(),
                                expand: content.expand,
                                strip_tabs: content.strip_tabs,
                            };
                            *heredoc_index += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    match stmt {
        Statement::SimpleCommand(cmd) => {
            inject_into_redirects(&mut cmd.redirects, heredoc_contents, heredoc_index);
        }
        Statement::Pipeline(pipeline) => {
            for element in &mut pipeline.elements {
                inject_heredoc_contents(&mut element.command, heredoc_contents, heredoc_index);
            }
        }
        Statement::List(list) => {
            inject_heredoc_contents(&mut list.first, heredoc_contents, heredoc_index);
            for (_, stmt) in &mut list.rest {
                inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
            }
        }
        Statement::If(if_stmt) => {
            for stmt in &mut if_stmt.then_body {
                inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
            }
            for elif in &mut if_stmt.elif_branches {
                for stmt in &mut elif.body {
                    inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
                }
            }
            if let Some(else_body) = &mut if_stmt.else_body {
                for stmt in else_body {
                    inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
                }
            }
        }
        Statement::For(for_stmt) => {
            inject_into_redirects(&mut for_stmt.redirects, heredoc_contents, heredoc_index);
            for stmt in &mut for_stmt.body {
                inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
            }
        }
        Statement::While(while_stmt) => {
            inject_into_redirects(&mut while_stmt.redirects, heredoc_contents, heredoc_index);
            for stmt in &mut while_stmt.body {
                inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
            }
        }
        Statement::Until(until_stmt) => {
            inject_into_redirects(&mut until_stmt.redirects, heredoc_contents, heredoc_index);
            for stmt in &mut until_stmt.body {
                inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
            }
        }
        Statement::Case(case_stmt) => {
            for arm in &mut case_stmt.arms {
                for stmt in &mut arm.body {
                    inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
                }
            }
        }
        Statement::Subshell(subshell) => {
            for stmt in &mut subshell.body {
                inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
            }
        }
        Statement::BraceGroup(group) => {
            for stmt in &mut group.body {
                inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
            }
        }
        Statement::FunctionDef(func) => {
            for stmt in &mut func.body {
                inject_heredoc_contents(stmt, heredoc_contents, heredoc_index);
            }
        }
        _ => {}
    }
}

/// Errors during parsing
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("syntax error at line {line}, column {column}: {message}")]
    Syntax {
        message: String,
        line: usize,
        column: usize,
    },

    #[error("AST build error: {0}")]
    Build(#[from] crate::builder::BuildError),

    #[error("heredoc preprocessing error: {0}")]
    HeredocPreprocessing(String),

    #[error("empty input")]
    EmptyInput,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_program() {
        let source = "echo hello world";
        let result = parse(source);
        assert!(result.is_ok());

        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
    }

    #[test]
    fn test_parse_multi_statement() {
        let source = r#"
            echo "Starting"
            var count = 10
            if [ "$count" -gt 5 ]; then
                echo "big"
            fi
        "#;

        let result = parse(source);
        assert!(result.is_ok(), "Failed: {:?}", result.err());

        let program = result.unwrap();
        assert_eq!(program.statements.len(), 3);
    }

    #[test]
    fn test_parse_pipeline() {
        let source = "cat file.txt | grep pattern | wc -l";
        let result = parse(source);
        assert!(result.is_ok());

        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
    }

    #[test]
    fn test_parse_statement_api() {
        let source = "echo hello";
        let result = parse_statement(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_expression_api() {
        let source = "42";
        let result = parse_expression(source);
        assert!(result.is_ok());

        let source = "x + y";
        let result = parse_expression(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_context_from_grammar() {
        // This tests that grammar structure provides context

        // "hello" as bareword in command
        let source = "echo hello";
        let result = parse(source);
        assert!(result.is_ok(), "Bareword in command should work");

        // "hello" as identifier in expression
        let source = "var x = hello";
        let result = parse(source);
        assert!(result.is_ok(), "Identifier in expression should work");

        // Both parse successfully - context from grammar!
    }

    #[test]
    fn test_operator_context() {
        // '<' as redirect
        let source = "cat < input.txt";
        let result = parse(source);
        assert!(result.is_ok(), "< as redirect should work");

        let source = "if [ 1 -lt 10 ]; then echo small; fi";
        let result = parse(source);
        assert!(result.is_ok(), "POSIX if with test command should work");
    }

    #[test]
    fn test_dollar_expansion() {
        let source = "echo $HOME";
        let result = parse(source);
        assert!(result.is_ok(), "Should parse $var");

        let source = r#"echo "Path: $PATH""#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse $var in quotes");

        let source = "echo $(ls)";
        let result = parse(source);
        assert!(result.is_ok(), "Should parse $(cmd)");
    }

    #[test]
    fn test_command_sub_with_args() {
        use crate::pest_parser::{ColonyParser, Rule};
        use pest::Parser;

        // Test -1: Can we parse "world" as a Word?
        eprintln!("\n=== Test -1: Parse 'world' as Word ===");
        match ColonyParser::parse(Rule::Word, "world") {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    eprintln!("✓ Word parsed: {:?}", pair.as_str());
                }
            }
            Err(e) => {
                eprintln!("✗ Cannot parse as Word: {}", e);
            }
        }

        // Test 0: Can we parse "echo world" as CommandList?
        eprintln!("\n=== Test 0: Parse 'echo world' as CommandList ===");
        match ColonyParser::parse(Rule::CommandList, "echo world") {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!("✓ CommandList: {:?}", pair.as_str());
                    for inner in pair.into_inner() {
                        eprintln!("  {:?} = {:?}", inner.as_rule(), inner.as_str());
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ Cannot parse as CommandList: {}", e);
            }
        }

        // Test 0b: Can we parse "echo world" as SimpleCommand?
        eprintln!("\n=== Test 0b: Parse 'echo world' as SimpleCommand ===");
        match ColonyParser::parse(Rule::SimpleCommand, "echo world") {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!("✓ SimpleCommand: {:?}", pair.as_str());
                    for inner in pair.into_inner() {
                        eprintln!("  {:?} = {:?}", inner.as_rule(), inner.as_str());
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ Cannot parse as SimpleCommand: {}", e);
            }
        }

        // First test: Can we parse $(echo world) as a Word by itself?
        eprintln!("\n=== Test 1: Parse $(echo world) as Word ===");
        match ColonyParser::parse(Rule::Word, "$(echo world)") {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    eprintln!("✓ Word parsed: {:?}", pair.as_str());
                    for inner in pair.into_inner() {
                        eprintln!("  Part: {:?} = {:?}", inner.as_rule(), inner.as_str());
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ Cannot parse as Word: {}", e);
            }
        }

        // Second test: Can we parse "hello $(echo world)" as two Words?
        eprintln!("\n=== Test 2: Parse 'hello $(echo world)' as Words ===");
        let test2 = "hello $(echo world)";
        match ColonyParser::parse(Rule::SimpleCommand, test2) {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!("Matched: {:?} = {:?}", pair.as_rule(), pair.as_str());
                    for inner in pair.into_inner() {
                        eprintln!("  {:?} = {:?}", inner.as_rule(), inner.as_str());
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }

        let source = "echo hello $(echo world)";

        // Test raw pest parsing
        eprintln!("\n=== Test 3: Full command with $(echo world) ===");
        match ColonyParser::parse(Rule::SimpleCommand, source) {
            Ok(pairs) => {
                for pair in pairs {
                    eprintln!(
                        "Rule: {:?}, Text: {:?}, Span: {:?}",
                        pair.as_rule(),
                        pair.as_str(),
                        pair.as_span()
                    );
                    for inner in pair.into_inner() {
                        eprintln!(
                            "  Inner Rule: {:?}, Text: {:?}",
                            inner.as_rule(),
                            inner.as_str()
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("Pest parse error: {}", e);
            }
        }

        // Now test parse_statement
        let result = parse_statement(source);

        if let Err(ref e) = result {
            eprintln!("Parse error: {}", e);
        }
        assert!(result.is_ok(), "Should parse command sub with args");

        let stmt = result.unwrap();
        eprintln!("\n=== AST Structure ===");
        eprintln!("{:#?}", stmt);

        // Navigate through List/Pipeline to get to SimpleCommand
        let cmd = match stmt {
            crate::ast::Statement::SimpleCommand(ref cmd) => cmd,
            crate::ast::Statement::List(ref list) => match &*list.first {
                crate::ast::Statement::Pipeline(ref pipeline) => {
                    if let crate::ast::Statement::SimpleCommand(ref cmd) =
                        *pipeline.elements[0].command
                    {
                        cmd
                    } else {
                        panic!("Pipeline doesn't contain SimpleCommand");
                    }
                }
                crate::ast::Statement::SimpleCommand(ref cmd) => cmd,
                _ => panic!("List doesn't contain expected structure"),
            },
            _ => panic!("Unexpected statement type"),
        };

        eprintln!("\n=== SimpleCommand Details ===");
        eprintln!("Command name: {:?}", cmd.name);
        eprintln!("Number of args: {}", cmd.args.len());
        for (i, arg) in cmd.args.iter().enumerate() {
            eprintln!("Arg {}: {:?}", i, arg);
        }

        assert_eq!(
            cmd.args.len(),
            2,
            "Should have 2 arguments: 'hello' and $(echo world)"
        );
    }

    #[test]
    fn test_heredoc_basic() {
        let source = r#"cat <<EOF
line 1
line 2
EOF"#;

        let result = parse(source);
        assert!(
            result.is_ok(),
            "Should parse basic heredoc: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_heredoc_strip_tabs() {
        let source = "cat <<-EOF\n\tindented content\nEOF";

        let result = parse(source);
        assert!(
            result.is_ok(),
            "Should parse <<- heredoc: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_heredoc_quoted_delimiter() {
        let source = r#"cat <<'EOF'
$var should not expand
EOF"#;

        let result = parse(source);
        assert!(
            result.is_ok(),
            "Should parse quoted delimiter: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_heredoc_in_pipeline() {
        let source = r#"cat <<EOF | grep pattern
search this content
and this too
EOF"#;

        let result = parse(source);
        assert!(
            result.is_ok(),
            "Should parse heredoc in pipeline: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_arith_in_assignment() {
        use crate::ast::{AssignmentValue, Statement, WordPart};

        let source = "x=$((1+1))";
        let result = parse_statement(source);
        assert!(result.is_ok(), "Should parse: {:?}", result.err());

        let stmt = result.unwrap();

        if let Statement::List(list) = stmt {
            if let Statement::Pipeline(pipeline) = &*list.first {
                if let Statement::SimpleCommand(cmd) = &*pipeline.elements[0].command {
                    assert!(!cmd.env_assignments.is_empty(), "Should have assignment");
                    let assignment = &cmd.env_assignments[0];
                    assert_eq!(assignment.name, "x");

                    if let AssignmentValue::Word(word) = &assignment.value {
                        assert!(
                            word.parts
                                .iter()
                                .any(|p| matches!(p, WordPart::ArithRaw(_))),
                            "Assignment should contain ArithRaw, got: {:?}",
                            word.parts
                        );
                    } else {
                        panic!("Expected Word assignment value");
                    }
                    return;
                }
            }
        }
        panic!("Unexpected AST structure");
    }

    #[test]
    fn test_extglob_ast_building() {
        use crate::ast::{ExtGlobOp, Statement, WordPart};

        let source = "echo !(foo).txt";
        let result = parse_statement(source);
        assert!(result.is_ok(), "Should parse extglob: {:?}", result.err());

        let stmt = result.unwrap();
        println!("AST: {:?}", stmt);

        if let Statement::List(list) = stmt {
            if let Statement::Pipeline(pipeline) = &*list.first {
                if let Statement::SimpleCommand(cmd) = &*pipeline.elements[0].command {
                    assert_eq!(cmd.args.len(), 1, "Should have 1 argument (!(foo).txt)");
                    let arg_word = &cmd.args[0];
                    println!("Arg word parts: {:?}", arg_word.parts);

                    let has_extglob = arg_word.parts.iter().any(|p| {
                        matches!(
                            p,
                            WordPart::ExtGlob {
                                operator: ExtGlobOp::Not,
                                ..
                            }
                        )
                    });
                    assert!(
                        has_extglob,
                        "Should have ExtGlob Not part, got: {:?}",
                        arg_word.parts
                    );
                    return;
                }
            }
        }
        panic!("Unexpected AST structure");
    }
}
