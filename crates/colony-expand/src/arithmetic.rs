//! Arithmetic expansion
//!
//! Evaluates $((expression)) as arithmetic
//! Supports full bash arithmetic including:
//! - Basic: +, -, *, /, %, ()
//! - Exponentiation: ** (right-associative)
//! - Comparison: <, <=, >, >=, ==, !=
//! - Bitwise: &, |, ^, ~, <<, >>
//! - Logical: !, &&, ||
//! - Ternary: ?:
//! - Assignment: =, +=, -=, *=, /=, %=, &=, |=, ^=, <<=, >>=
//! - Increment/Decrement: ++, --

use crate::{ExpansionContext, ExpansionError, Result};

/// Evaluate an arithmetic expression
///
/// Supports full bash shell arithmetic with proper precedence.
/// Returns the result as a string (for substitution into command)
pub fn expand_arithmetic(expr: &str, context: &mut ExpansionContext) -> Result<String> {
    let expanded = pre_expand_variables(expr.trim(), context);
    let result = eval_arithmetic(&expanded, context)?;
    Ok(result.to_string())
}

/// Immutable version for compatibility - creates a temporary mutable context
pub fn expand_arithmetic_immut(expr: &str, context: &ExpansionContext) -> Result<String> {
    let expanded = pre_expand_variables(expr.trim(), context);
    // For expressions without assignment, we can use a clone
    let mut temp_context = context.clone();
    let result = eval_arithmetic(&expanded, &mut temp_context)?;
    Ok(result.to_string())
}

fn pre_expand_variables(expr: &str, context: &ExpansionContext) -> String {
    let mut result = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            i += 1;
            if i < chars.len() {
                if chars[i] == '{' {
                    i += 1;
                    let start = i;
                    let mut brace_depth = 1;
                    while i < chars.len() && brace_depth > 0 {
                        if chars[i] == '{' {
                            brace_depth += 1;
                        } else if chars[i] == '}' {
                            brace_depth -= 1;
                        }
                        if brace_depth > 0 {
                            i += 1;
                        }
                    }
                    let var_name: String = chars[start..i].iter().collect();
                    let value = context.get_var_or_env(&var_name).unwrap_or_default();
                    result.push_str(&value);
                    if i < chars.len() {
                        i += 1;
                    }
                } else if chars[i].is_ascii_digit() {
                    let start = i;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    let param_str: String = chars[start..i].iter().collect();
                    if let Ok(param_num) = param_str.parse::<usize>() {
                        let value = context.get_positional_param(param_num).unwrap_or_default();
                        result.push_str(value);
                    }
                } else if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
                    let start = i;
                    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let var_name: String = chars[start..i].iter().collect();
                    let value = context.get_var_or_env(&var_name).unwrap_or_default();
                    result.push_str(&value);
                } else {
                    result.push('$');
                }
            } else {
                result.push('$');
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Arithmetic token
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64),
    Variable(String),
    // Basic operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    // Exponentiation
    Power, // **
    // Comparison
    Lt, // <
    Le, // <=
    Gt, // >
    Ge, // >=
    Eq, // ==
    Ne, // !=
    // Bitwise
    Amp,   // &
    Pipe,  // |
    Caret, // ^
    Tilde, // ~
    Shl,   // <<
    Shr,   // >>
    // Logical
    Not, // !
    And, // &&
    Or,  // ||
    // Ternary
    Question, // ?
    Colon,    // :
    // Increment/Decrement
    PlusPlus,   // ++
    MinusMinus, // --
    // Assignment
    Assign,    // =
    PlusEq,    // +=
    MinusEq,   // -=
    StarEq,    // *=
    SlashEq,   // /=
    PercentEq, // %=
    AmpEq,     // &=
    PipeEq,    // |=
    CaretEq,   // ^=
    ShlEq,     // <<=
    ShrEq,     // >>=
    // Grouping
    LParen,
    RParen,
    // Array subscript
    LBracket,
    RBracket,
    // Comma for expressions
    Comma,
}

/// Tokenize arithmetic expression
fn tokenize_arith(expr: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' => {
                i += 1;
            }
            '+' => {
                if i + 1 < chars.len() && chars[i + 1] == '+' {
                    tokens.push(Token::PlusPlus);
                    i += 2;
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::PlusEq);
                    i += 2;
                } else {
                    tokens.push(Token::Plus);
                    i += 1;
                }
            }
            '-' => {
                if i + 1 < chars.len() && chars[i + 1] == '-' {
                    tokens.push(Token::MinusMinus);
                    i += 2;
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::MinusEq);
                    i += 2;
                } else {
                    tokens.push(Token::Minus);
                    i += 1;
                }
            }
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    tokens.push(Token::Power);
                    i += 2;
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::StarEq);
                    i += 2;
                } else {
                    tokens.push(Token::Star);
                    i += 1;
                }
            }
            '/' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::SlashEq);
                    i += 2;
                } else {
                    tokens.push(Token::Slash);
                    i += 1;
                }
            }
            '%' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::PercentEq);
                    i += 2;
                } else {
                    tokens.push(Token::Percent);
                    i += 1;
                }
            }
            '<' => {
                if i + 1 < chars.len() && chars[i + 1] == '<' {
                    if i + 2 < chars.len() && chars[i + 2] == '=' {
                        tokens.push(Token::ShlEq);
                        i += 3;
                    } else {
                        tokens.push(Token::Shl);
                        i += 2;
                    }
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Le);
                    i += 2;
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    if i + 2 < chars.len() && chars[i + 2] == '=' {
                        tokens.push(Token::ShrEq);
                        i += 3;
                    } else {
                        tokens.push(Token::Shr);
                        i += 2;
                    }
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Ge);
                    i += 2;
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            '=' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Eq);
                    i += 2;
                } else {
                    tokens.push(Token::Assign);
                    i += 1;
                }
            }
            '!' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Ne);
                    i += 2;
                } else {
                    tokens.push(Token::Not);
                    i += 1;
                }
            }
            '&' => {
                if i + 1 < chars.len() && chars[i + 1] == '&' {
                    tokens.push(Token::And);
                    i += 2;
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::AmpEq);
                    i += 2;
                } else {
                    tokens.push(Token::Amp);
                    i += 1;
                }
            }
            '|' => {
                if i + 1 < chars.len() && chars[i + 1] == '|' {
                    tokens.push(Token::Or);
                    i += 2;
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::PipeEq);
                    i += 2;
                } else {
                    tokens.push(Token::Pipe);
                    i += 1;
                }
            }
            '^' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::CaretEq);
                    i += 2;
                } else {
                    tokens.push(Token::Caret);
                    i += 1;
                }
            }
            '~' => {
                tokens.push(Token::Tilde);
                i += 1;
            }
            '?' => {
                tokens.push(Token::Question);
                i += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '[' => {
                tokens.push(Token::LBracket);
                i += 1;
            }
            ']' => {
                tokens.push(Token::RBracket);
                i += 1;
            }
            '0'..='9' => {
                let start = i;
                // Check for hex (0x), octal (0), or binary (0b)
                if chars[i] == '0' && i + 1 < chars.len() {
                    if chars[i + 1] == 'x' || chars[i + 1] == 'X' {
                        // Hexadecimal
                        i += 2;
                        let hex_start = i;
                        while i < chars.len() && chars[i].is_ascii_hexdigit() {
                            i += 1;
                        }
                        let hex_str: String = chars[hex_start..i].iter().collect();
                        let value = i64::from_str_radix(&hex_str, 16).map_err(|_| {
                            ExpansionError::ArithmeticError(format!("Invalid hex: 0x{}", hex_str))
                        })?;
                        tokens.push(Token::Number(value));
                        continue;
                    } else if chars[i + 1] == 'b' || chars[i + 1] == 'B' {
                        // Binary
                        i += 2;
                        let bin_start = i;
                        while i < chars.len() && (chars[i] == '0' || chars[i] == '1') {
                            i += 1;
                        }
                        let bin_str: String = chars[bin_start..i].iter().collect();
                        let value = i64::from_str_radix(&bin_str, 2).map_err(|_| {
                            ExpansionError::ArithmeticError(format!(
                                "Invalid binary: 0b{}",
                                bin_str
                            ))
                        })?;
                        tokens.push(Token::Number(value));
                        continue;
                    } else if chars[i + 1].is_ascii_digit() {
                        // Octal
                        i += 1;
                        let oct_start = i;
                        while i < chars.len() && chars[i] >= '0' && chars[i] <= '7' {
                            i += 1;
                        }
                        let oct_str: String = chars[oct_start..i].iter().collect();
                        let value = i64::from_str_radix(&oct_str, 8).map_err(|_| {
                            ExpansionError::ArithmeticError(format!("Invalid octal: 0{}", oct_str))
                        })?;
                        tokens.push(Token::Number(value));
                        continue;
                    }
                }
                // Decimal
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let value = num_str.parse::<i64>().map_err(|_| {
                    ExpansionError::ArithmeticError(format!("Invalid number: {}", num_str))
                })?;
                tokens.push(Token::Number(value));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let var_name: String = chars[start..i].iter().collect();
                tokens.push(Token::Variable(var_name));
            }
            _ => {
                return Err(ExpansionError::ArithmeticError(format!(
                    "Unexpected character: {}",
                    chars[i]
                )));
            }
        }
    }

    Ok(tokens)
}

/// Parse and evaluate arithmetic expression
fn eval_arithmetic(expr: &str, context: &mut ExpansionContext) -> Result<i64> {
    let tokens = tokenize_arith(expr)?;
    if tokens.is_empty() {
        return Ok(0);
    }
    let (result, _) = parse_comma(&tokens, 0, context)?;
    Ok(result)
}

// Operator precedence (lowest to highest):
// 1. Comma
// 2. Assignment (=, +=, etc.) - right associative
// 3. Ternary (?:) - right associative
// 4. Logical OR (||)
// 5. Logical AND (&&)
// 6. Bitwise OR (|)
// 7. Bitwise XOR (^)
// 8. Bitwise AND (&)
// 9. Equality (==, !=)
// 10. Comparison (<, <=, >, >=)
// 11. Shift (<<, >>)
// 12. Additive (+, -)
// 13. Multiplicative (*, /, %)
// 14. Exponentiation (**) - right associative
// 15. Unary (!, ~, -, +, ++, --)
// 16. Postfix (++, --)
// 17. Primary (numbers, variables, parentheses)

// =============================================================================
// Skip functions for short-circuit evaluation
// These advance past tokens WITHOUT evaluating them (no side effects)
// =============================================================================

/// Skip primary expression (numbers, variables, parentheses) without evaluation
fn skip_primary(tokens: &[Token], pos: usize) -> Result<usize> {
    if pos >= tokens.len() {
        return Err(ExpansionError::ArithmeticError(
            "Unexpected end of expression".to_string(),
        ));
    }

    match &tokens[pos] {
        Token::Number(_) => Ok(pos + 1),
        Token::Variable(_) => {
            // Check for array subscript
            if pos + 1 < tokens.len() && tokens[pos + 1] == Token::LBracket {
                let mut bracket_pos = pos + 2;
                let mut depth = 1;
                while bracket_pos < tokens.len() && depth > 0 {
                    match &tokens[bracket_pos] {
                        Token::LBracket => depth += 1,
                        Token::RBracket => depth -= 1,
                        _ => {}
                    }
                    bracket_pos += 1;
                }
                Ok(bracket_pos)
            } else {
                Ok(pos + 1)
            }
        }
        Token::LParen => {
            // Skip past matching closing paren
            let mut paren_pos = pos + 1;
            let mut depth = 1;
            while paren_pos < tokens.len() && depth > 0 {
                match &tokens[paren_pos] {
                    Token::LParen => depth += 1,
                    Token::RParen => depth -= 1,
                    _ => {}
                }
                paren_pos += 1;
            }
            Ok(paren_pos)
        }
        _ => Err(ExpansionError::ArithmeticError(format!(
            "Unexpected token in skip: {:?}",
            tokens[pos]
        ))),
    }
}

/// Skip postfix operators (++, --)
fn skip_postfix(tokens: &[Token], pos: usize) -> Result<usize> {
    let new_pos = skip_primary(tokens, pos)?;

    // Check for postfix operators
    if new_pos < tokens.len() {
        match &tokens[new_pos] {
            Token::PlusPlus | Token::MinusMinus => Ok(new_pos + 1),
            _ => Ok(new_pos),
        }
    } else {
        Ok(new_pos)
    }
}

/// Skip unary expression (!, ~, -, +, ++var, --var)
fn skip_unary(tokens: &[Token], pos: usize) -> Result<usize> {
    if pos >= tokens.len() {
        return Err(ExpansionError::ArithmeticError(
            "Unexpected end of expression".to_string(),
        ));
    }

    match &tokens[pos] {
        Token::Not | Token::Tilde | Token::Minus | Token::Plus => skip_unary(tokens, pos + 1),
        Token::PlusPlus | Token::MinusMinus => {
            // Pre-increment/decrement: skip ++ and the variable
            if pos + 1 < tokens.len() {
                if let Token::Variable(_) = tokens[pos + 1] {
                    // Skip the ++ and variable, and check for array subscript
                    if pos + 2 < tokens.len() && tokens[pos + 2] == Token::LBracket {
                        let mut bracket_pos = pos + 3;
                        let mut depth = 1;
                        while bracket_pos < tokens.len() && depth > 0 {
                            match &tokens[bracket_pos] {
                                Token::LBracket => depth += 1,
                                Token::RBracket => depth -= 1,
                                _ => {}
                            }
                            bracket_pos += 1;
                        }
                        return Ok(bracket_pos);
                    }
                    return Ok(pos + 2);
                }
            }
            Err(ExpansionError::ArithmeticError(
                "++ requires variable".to_string(),
            ))
        }
        _ => skip_postfix(tokens, pos),
    }
}

/// Skip exponentiation expression
fn skip_exponent(tokens: &[Token], pos: usize) -> Result<usize> {
    let new_pos = skip_unary(tokens, pos)?;

    if new_pos < tokens.len() && tokens[new_pos] == Token::Power {
        // Skip the ** operator and parse the right side recursively
        skip_exponent(tokens, new_pos + 1)
    } else {
        Ok(new_pos)
    }
}

/// Skip multiplicative expression (*, /, %)
fn skip_multiplicative(tokens: &[Token], mut pos: usize) -> Result<usize> {
    pos = skip_exponent(tokens, pos)?;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Star | Token::Slash | Token::Percent => {
                pos = skip_exponent(tokens, pos + 1)?;
            }
            _ => break,
        }
    }

    Ok(pos)
}

/// Skip additive expression (+, -)
fn skip_additive(tokens: &[Token], mut pos: usize) -> Result<usize> {
    pos = skip_multiplicative(tokens, pos)?;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Plus | Token::Minus => {
                pos = skip_multiplicative(tokens, pos + 1)?;
            }
            _ => break,
        }
    }

    Ok(pos)
}

/// Skip shift expression (<<, >>)
fn skip_shift(tokens: &[Token], mut pos: usize) -> Result<usize> {
    pos = skip_additive(tokens, pos)?;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Shl | Token::Shr => {
                pos = skip_additive(tokens, pos + 1)?;
            }
            _ => break,
        }
    }

    Ok(pos)
}

/// Skip comparison expression (<, <=, >, >=)
fn skip_comparison(tokens: &[Token], mut pos: usize) -> Result<usize> {
    pos = skip_shift(tokens, pos)?;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Lt | Token::Le | Token::Gt | Token::Ge => {
                pos = skip_shift(tokens, pos + 1)?;
            }
            _ => break,
        }
    }

    Ok(pos)
}

/// Skip equality expression (==, !=)
fn skip_equality(tokens: &[Token], mut pos: usize) -> Result<usize> {
    pos = skip_comparison(tokens, pos)?;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Eq | Token::Ne => {
                pos = skip_comparison(tokens, pos + 1)?;
            }
            _ => break,
        }
    }

    Ok(pos)
}

/// Skip bitwise AND expression (&)
fn skip_bitwise_and(tokens: &[Token], mut pos: usize) -> Result<usize> {
    pos = skip_equality(tokens, pos)?;

    while pos < tokens.len() && tokens[pos] == Token::Amp {
        pos = skip_equality(tokens, pos + 1)?;
    }

    Ok(pos)
}

/// Skip bitwise XOR expression (^)
fn skip_bitwise_xor(tokens: &[Token], mut pos: usize) -> Result<usize> {
    pos = skip_bitwise_and(tokens, pos)?;

    while pos < tokens.len() && tokens[pos] == Token::Caret {
        pos = skip_bitwise_and(tokens, pos + 1)?;
    }

    Ok(pos)
}

/// Skip bitwise OR expression (|)
fn skip_bitwise_or(tokens: &[Token], mut pos: usize) -> Result<usize> {
    pos = skip_bitwise_xor(tokens, pos)?;

    while pos < tokens.len() && tokens[pos] == Token::Pipe {
        pos = skip_bitwise_xor(tokens, pos + 1)?;
    }

    Ok(pos)
}

/// Skip logical AND expression (&&) - used by logical OR short-circuit
fn skip_logical_and(tokens: &[Token], mut pos: usize) -> Result<usize> {
    pos = skip_bitwise_or(tokens, pos)?;

    while pos < tokens.len() && tokens[pos] == Token::And {
        pos = skip_bitwise_or(tokens, pos + 1)?;
    }

    Ok(pos)
}

/// Parse comma expression (lowest precedence)
fn parse_comma(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut result, new_pos) = parse_assignment(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() && tokens[pos] == Token::Comma {
        pos += 1;
        let (val, new_pos) = parse_assignment(tokens, pos, context)?;
        result = val;
        pos = new_pos;
    }

    Ok((result, pos))
}

/// Parse assignment expression (right associative)
fn parse_assignment(
    tokens: &[Token],
    pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    // Check if this is an assignment (variable followed by assignment operator)
    if pos < tokens.len() {
        if let Token::Variable(ref name) = tokens[pos] {
            // Check for array subscript
            let (var_name, subscript, after_var) =
                if pos + 1 < tokens.len() && tokens[pos + 1] == Token::LBracket {
                    let mut bracket_pos = pos + 2;
                    let subscript_start = bracket_pos;
                    let mut depth = 1;
                    while bracket_pos < tokens.len() && depth > 0 {
                        match &tokens[bracket_pos] {
                            Token::LBracket => depth += 1,
                            Token::RBracket => depth -= 1,
                            _ => {}
                        }
                        if depth > 0 {
                            bracket_pos += 1;
                        }
                    }
                    // Evaluate subscript
                    let subscript_tokens = &tokens[subscript_start..bracket_pos];
                    let (idx, _) = if subscript_tokens.is_empty() {
                        (0i64, 0)
                    } else {
                        parse_comma(subscript_tokens, 0, context)?
                    };
                    (name.clone(), Some(idx as usize), bracket_pos + 1)
                } else {
                    (name.clone(), None, pos + 1)
                };

            if after_var < tokens.len() {
                let (assign_op, op_end) = match &tokens[after_var] {
                    Token::Assign => (Some(AssignOp::Set), after_var + 1),
                    Token::PlusEq => (Some(AssignOp::Add), after_var + 1),
                    Token::MinusEq => (Some(AssignOp::Sub), after_var + 1),
                    Token::StarEq => (Some(AssignOp::Mul), after_var + 1),
                    Token::SlashEq => (Some(AssignOp::Div), after_var + 1),
                    Token::PercentEq => (Some(AssignOp::Mod), after_var + 1),
                    Token::AmpEq => (Some(AssignOp::BitAnd), after_var + 1),
                    Token::PipeEq => (Some(AssignOp::BitOr), after_var + 1),
                    Token::CaretEq => (Some(AssignOp::BitXor), after_var + 1),
                    Token::ShlEq => (Some(AssignOp::Shl), after_var + 1),
                    Token::ShrEq => (Some(AssignOp::Shr), after_var + 1),
                    _ => (None, after_var),
                };

                if let Some(op) = assign_op {
                    // Parse the right side (recursively for right associativity)
                    let (rhs, final_pos) = parse_assignment(tokens, op_end, context)?;

                    // Get current value for compound assignment
                    let current = get_var_value(&var_name, subscript, context);

                    let new_value = match op {
                        AssignOp::Set => rhs,
                        AssignOp::Add => current + rhs,
                        AssignOp::Sub => current - rhs,
                        AssignOp::Mul => current * rhs,
                        AssignOp::Div => {
                            if rhs == 0 {
                                return Err(ExpansionError::ArithmeticError(
                                    "Division by zero".to_string(),
                                ));
                            }
                            current / rhs
                        }
                        AssignOp::Mod => {
                            if rhs == 0 {
                                return Err(ExpansionError::ArithmeticError(
                                    "Modulo by zero".to_string(),
                                ));
                            }
                            current % rhs
                        }
                        AssignOp::BitAnd => current & rhs,
                        AssignOp::BitOr => current | rhs,
                        AssignOp::BitXor => current ^ rhs,
                        AssignOp::Shl => current << rhs,
                        AssignOp::Shr => current >> rhs,
                    };

                    // Set the variable
                    set_var_value(&var_name, subscript, new_value, context);

                    return Ok((new_value, final_pos));
                }
            }
        }
    }

    // Not an assignment, parse as ternary
    parse_ternary(tokens, pos, context)
}

#[derive(Clone, Copy)]
enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

fn get_var_value(name: &str, subscript: Option<usize>, context: &ExpansionContext) -> i64 {
    if let Some(idx) = subscript {
        // Array access
        if let Some(crate::Variable::Array(arr)) = context.get_variable(name) {
            arr.get(idx).and_then(|s| s.parse().ok()).unwrap_or(0)
        } else {
            0
        }
    } else {
        context
            .get_var_or_env(name)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}

fn set_var_value(name: &str, subscript: Option<usize>, value: i64, context: &mut ExpansionContext) {
    if let Some(idx) = subscript {
        // Array assignment
        if let Some(crate::Variable::Array(arr)) = context.get_variable(name) {
            let mut arr = arr.clone();
            while arr.len() <= idx {
                arr.push(String::new());
            }
            arr[idx] = value.to_string();
            context.set_variable(name.to_string(), crate::Variable::Array(arr));
        } else {
            // Create new array
            let mut arr = vec![String::new(); idx + 1];
            arr[idx] = value.to_string();
            context.set_variable(name.to_string(), crate::Variable::Array(arr));
        }
    } else {
        context.set_var(name.to_string(), value.to_string());
    }
}

/// Parse ternary expression (right associative)
fn parse_ternary(
    tokens: &[Token],
    pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (cond, mut new_pos) = parse_logical_or(tokens, pos, context)?;

    if new_pos < tokens.len() && tokens[new_pos] == Token::Question {
        new_pos += 1;
        let (true_val, after_true) = parse_ternary(tokens, new_pos, context)?;

        if after_true >= tokens.len() || tokens[after_true] != Token::Colon {
            return Err(ExpansionError::ArithmeticError(
                "Expected ':' in ternary".to_string(),
            ));
        }

        let (false_val, after_false) = parse_ternary(tokens, after_true + 1, context)?;

        let result = if cond != 0 { true_val } else { false_val };
        return Ok((result, after_false));
    }

    Ok((cond, new_pos))
}

fn parse_logical_or(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_logical_and(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() && tokens[pos] == Token::Or {
        pos += 1;
        if left != 0 {
            // Short-circuit: skip right side without evaluating (no side effects)
            pos = skip_logical_and(tokens, pos)?;
        } else {
            let (right, new_pos) = parse_logical_and(tokens, pos, context)?;
            left = if right != 0 { 1 } else { 0 };
            pos = new_pos;
        }
    }

    Ok((left, pos))
}

fn parse_logical_and(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_bitwise_or(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() && tokens[pos] == Token::And {
        pos += 1;
        if left == 0 {
            // Short-circuit: skip right side without evaluating (no side effects)
            pos = skip_bitwise_or(tokens, pos)?;
        } else {
            let (right, new_pos) = parse_bitwise_or(tokens, pos, context)?;
            left = if right != 0 { 1 } else { 0 };
            pos = new_pos;
        }
    }

    Ok((left, pos))
}

/// Parse bitwise OR (|)
fn parse_bitwise_or(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_bitwise_xor(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() && tokens[pos] == Token::Pipe {
        pos += 1;
        let (right, new_pos) = parse_bitwise_xor(tokens, pos, context)?;
        left |= right;
        pos = new_pos;
    }

    Ok((left, pos))
}

/// Parse bitwise XOR (^)
fn parse_bitwise_xor(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_bitwise_and(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() && tokens[pos] == Token::Caret {
        pos += 1;
        let (right, new_pos) = parse_bitwise_and(tokens, pos, context)?;
        left ^= right;
        pos = new_pos;
    }

    Ok((left, pos))
}

/// Parse bitwise AND (&)
fn parse_bitwise_and(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_equality(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() && tokens[pos] == Token::Amp {
        pos += 1;
        let (right, new_pos) = parse_equality(tokens, pos, context)?;
        left &= right;
        pos = new_pos;
    }

    Ok((left, pos))
}

/// Parse equality (==, !=)
fn parse_equality(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_comparison(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Eq => {
                pos += 1;
                let (right, new_pos) = parse_comparison(tokens, pos, context)?;
                left = if left == right { 1 } else { 0 };
                pos = new_pos;
            }
            Token::Ne => {
                pos += 1;
                let (right, new_pos) = parse_comparison(tokens, pos, context)?;
                left = if left != right { 1 } else { 0 };
                pos = new_pos;
            }
            _ => break,
        }
    }

    Ok((left, pos))
}

/// Parse comparison (<, <=, >, >=)
fn parse_comparison(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_shift(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Lt => {
                pos += 1;
                let (right, new_pos) = parse_shift(tokens, pos, context)?;
                left = if left < right { 1 } else { 0 };
                pos = new_pos;
            }
            Token::Le => {
                pos += 1;
                let (right, new_pos) = parse_shift(tokens, pos, context)?;
                left = if left <= right { 1 } else { 0 };
                pos = new_pos;
            }
            Token::Gt => {
                pos += 1;
                let (right, new_pos) = parse_shift(tokens, pos, context)?;
                left = if left > right { 1 } else { 0 };
                pos = new_pos;
            }
            Token::Ge => {
                pos += 1;
                let (right, new_pos) = parse_shift(tokens, pos, context)?;
                left = if left >= right { 1 } else { 0 };
                pos = new_pos;
            }
            _ => break,
        }
    }

    Ok((left, pos))
}

/// Parse shift (<<, >>)
fn parse_shift(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_additive(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Shl => {
                pos += 1;
                let (right, new_pos) = parse_additive(tokens, pos, context)?;
                left <<= right;
                pos = new_pos;
            }
            Token::Shr => {
                pos += 1;
                let (right, new_pos) = parse_additive(tokens, pos, context)?;
                left >>= right;
                pos = new_pos;
            }
            _ => break,
        }
    }

    Ok((left, pos))
}

/// Parse additive (+, -)
fn parse_additive(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_multiplicative(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() {
        match tokens[pos] {
            Token::Plus => {
                pos += 1;
                let (right, new_pos) = parse_multiplicative(tokens, pos, context)?;
                left += right;
                pos = new_pos;
            }
            Token::Minus => {
                pos += 1;
                let (right, new_pos) = parse_multiplicative(tokens, pos, context)?;
                left -= right;
                pos = new_pos;
            }
            _ => break,
        }
    }

    Ok((left, pos))
}

/// Parse multiplicative (*, /, %)
fn parse_multiplicative(
    tokens: &[Token],
    mut pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_exponent(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() {
        match tokens[pos] {
            Token::Star => {
                pos += 1;
                let (right, new_pos) = parse_exponent(tokens, pos, context)?;
                left *= right;
                pos = new_pos;
            }
            Token::Slash => {
                pos += 1;
                let (right, new_pos) = parse_exponent(tokens, pos, context)?;
                if right == 0 {
                    return Err(ExpansionError::ArithmeticError(
                        "Division by zero".to_string(),
                    ));
                }
                left /= right;
                pos = new_pos;
            }
            Token::Percent => {
                pos += 1;
                let (right, new_pos) = parse_exponent(tokens, pos, context)?;
                if right == 0 {
                    return Err(ExpansionError::ArithmeticError(
                        "Modulo by zero".to_string(),
                    ));
                }
                left %= right;
                pos = new_pos;
            }
            _ => break,
        }
    }

    Ok((left, pos))
}

/// Parse exponentiation (**) - right associative
fn parse_exponent(
    tokens: &[Token],
    pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (base, new_pos) = parse_unary(tokens, pos, context)?;

    if new_pos < tokens.len() && tokens[new_pos] == Token::Power {
        // Right associative: parse the rest as exponent recursively
        let (exp, final_pos) = parse_exponent(tokens, new_pos + 1, context)?;
        let result = pow(base, exp);
        return Ok((result, final_pos));
    }

    Ok((base, new_pos))
}

/// Integer power function
fn pow(base: i64, exp: i64) -> i64 {
    if exp < 0 {
        return 0; // Bash returns 0 for negative exponents
    }
    let mut result: i64 = 1;
    let mut b = base;
    let mut e = exp as u64;
    while e > 0 {
        if e & 1 == 1 {
            result = result.wrapping_mul(b);
        }
        b = b.wrapping_mul(b);
        e >>= 1;
    }
    result
}

/// Parse unary (!, ~, -, +, ++var, --var)
fn parse_unary(
    tokens: &[Token],
    pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    if pos >= tokens.len() {
        return Err(ExpansionError::ArithmeticError(
            "Unexpected end of expression".to_string(),
        ));
    }

    match &tokens[pos] {
        Token::Not => {
            let (val, new_pos) = parse_unary(tokens, pos + 1, context)?;
            Ok((if val == 0 { 1 } else { 0 }, new_pos))
        }
        Token::Tilde => {
            let (val, new_pos) = parse_unary(tokens, pos + 1, context)?;
            Ok((!val, new_pos))
        }
        Token::Minus => {
            let (val, new_pos) = parse_unary(tokens, pos + 1, context)?;
            Ok((-val, new_pos))
        }
        Token::Plus => parse_unary(tokens, pos + 1, context),
        Token::PlusPlus => {
            // Pre-increment
            if pos + 1 < tokens.len() {
                if let Token::Variable(ref name) = tokens[pos + 1] {
                    let current = context
                        .get_var_or_env(name)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let new_val = current + 1;
                    context.set_var(name.clone(), new_val.to_string());
                    return Ok((new_val, pos + 2));
                }
            }
            Err(ExpansionError::ArithmeticError(
                "++ requires variable".to_string(),
            ))
        }
        Token::MinusMinus => {
            // Pre-decrement
            if pos + 1 < tokens.len() {
                if let Token::Variable(ref name) = tokens[pos + 1] {
                    let current = context
                        .get_var_or_env(name)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let new_val = current - 1;
                    context.set_var(name.clone(), new_val.to_string());
                    return Ok((new_val, pos + 2));
                }
            }
            Err(ExpansionError::ArithmeticError(
                "-- requires variable".to_string(),
            ))
        }
        _ => parse_postfix(tokens, pos, context),
    }
}

/// Parse postfix (var++, var--, array[idx])
fn parse_postfix(
    tokens: &[Token],
    pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    let (val, new_pos) = parse_primary(tokens, pos, context)?;

    // Check for postfix operators on variables
    if let Some(Token::Variable(ref name)) = tokens.get(pos) {
        // Skip array subscript if present
        let mut after_var = pos + 1;
        if after_var < tokens.len() && tokens[after_var] == Token::LBracket {
            let mut depth = 1;
            after_var += 1;
            while after_var < tokens.len() && depth > 0 {
                match &tokens[after_var] {
                    Token::LBracket => depth += 1,
                    Token::RBracket => depth -= 1,
                    _ => {}
                }
                after_var += 1;
            }
        }

        if after_var < tokens.len() {
            match &tokens[after_var] {
                Token::PlusPlus => {
                    // Post-increment: return current value, then increment
                    let new_val = val + 1;
                    context.set_var(name.clone(), new_val.to_string());
                    return Ok((val, after_var + 1));
                }
                Token::MinusMinus => {
                    // Post-decrement: return current value, then decrement
                    let new_val = val - 1;
                    context.set_var(name.clone(), new_val.to_string());
                    return Ok((val, after_var + 1));
                }
                _ => {}
            }
        }
    }

    Ok((val, new_pos))
}

/// Parse primary (numbers, variables, parentheses)
fn parse_primary(
    tokens: &[Token],
    pos: usize,
    context: &mut ExpansionContext,
) -> Result<(i64, usize)> {
    if pos >= tokens.len() {
        return Err(ExpansionError::ArithmeticError(
            "Unexpected end of expression".to_string(),
        ));
    }

    match &tokens[pos] {
        Token::Number(n) => Ok((*n, pos + 1)),
        Token::Variable(name) => {
            // Check for array subscript
            if pos + 1 < tokens.len() && tokens[pos + 1] == Token::LBracket {
                let mut bracket_pos = pos + 2;
                let subscript_start = bracket_pos;
                let mut depth = 1;
                while bracket_pos < tokens.len() && depth > 0 {
                    match &tokens[bracket_pos] {
                        Token::LBracket => depth += 1,
                        Token::RBracket => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        bracket_pos += 1;
                    }
                }
                // Evaluate subscript
                let subscript_tokens = &tokens[subscript_start..bracket_pos];
                let idx = if subscript_tokens.is_empty() {
                    0
                } else {
                    let (idx, _) = parse_comma(subscript_tokens, 0, context)?;
                    idx as usize
                };

                // Get array value
                let value = if let Some(crate::Variable::Array(arr)) = context.get_variable(name) {
                    arr.get(idx).and_then(|s| s.parse().ok()).unwrap_or(0)
                } else {
                    0
                };

                return Ok((value, bracket_pos + 1));
            }

            // Simple variable
            let value = match context.get_var_or_env(name) {
                Some(s) => s.parse::<i64>().unwrap_or(0),
                None => 0,
            };
            Ok((value, pos + 1))
        }
        Token::LParen => {
            let (result, new_pos) = parse_comma(tokens, pos + 1, context)?;
            if new_pos >= tokens.len() || tokens[new_pos] != Token::RParen {
                return Err(ExpansionError::ArithmeticError(
                    "Missing closing )".to_string(),
                ));
            }
            Ok((result, new_pos + 1))
        }
        _ => Err(ExpansionError::ArithmeticError(format!(
            "Unexpected token: {:?}",
            tokens[pos]
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithmetic_simple() {
        let mut context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("1 + 1", &mut context).unwrap(), "2");
        assert_eq!(expand_arithmetic("10 - 3", &mut context).unwrap(), "7");
        assert_eq!(expand_arithmetic("4 * 5", &mut context).unwrap(), "20");
        assert_eq!(expand_arithmetic("20 / 4", &mut context).unwrap(), "5");
        assert_eq!(expand_arithmetic("17 % 5", &mut context).unwrap(), "2");
    }

    #[test]
    fn test_arithmetic_precedence() {
        let mut context = ExpansionContext::new();

        // Multiplication before addition
        assert_eq!(expand_arithmetic("2 + 3 * 4", &mut context).unwrap(), "14");
        // Parentheses override precedence
        assert_eq!(
            expand_arithmetic("(2 + 3) * 4", &mut context).unwrap(),
            "20"
        );
    }

    #[test]
    fn test_arithmetic_variables() {
        let mut context = ExpansionContext::new();
        context.set_var("x".to_string(), "10".to_string());
        context.set_var("y".to_string(), "5".to_string());

        assert_eq!(expand_arithmetic("x + y", &mut context).unwrap(), "15");
        assert_eq!(expand_arithmetic("x * 2", &mut context).unwrap(), "20");
    }

    #[test]
    fn test_arithmetic_unary() {
        let mut context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("-5", &mut context).unwrap(), "-5");
        assert_eq!(expand_arithmetic("-(2 + 3)", &mut context).unwrap(), "-5");
    }

    #[test]
    fn test_arithmetic_exponentiation() {
        let mut context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("2 ** 3", &mut context).unwrap(), "8");
        assert_eq!(
            expand_arithmetic("2 ** 3 ** 2", &mut context).unwrap(),
            "512"
        ); // 2^(3^2) = 2^9 = 512
        assert_eq!(expand_arithmetic("3 * 2 ** 3", &mut context).unwrap(), "24");
        // 3 * 8 = 24
    }

    #[test]
    fn test_arithmetic_comparison() {
        let mut context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("5 > 3", &mut context).unwrap(), "1");
        assert_eq!(expand_arithmetic("5 < 3", &mut context).unwrap(), "0");
        assert_eq!(expand_arithmetic("5 >= 5", &mut context).unwrap(), "1");
        assert_eq!(expand_arithmetic("5 <= 4", &mut context).unwrap(), "0");
        assert_eq!(expand_arithmetic("5 == 5", &mut context).unwrap(), "1");
        assert_eq!(expand_arithmetic("5 != 5", &mut context).unwrap(), "0");
    }

    #[test]
    fn test_arithmetic_logical() {
        let mut context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("!0", &mut context).unwrap(), "1");
        assert_eq!(expand_arithmetic("!1", &mut context).unwrap(), "0");
        assert_eq!(expand_arithmetic("1 && 1", &mut context).unwrap(), "1");
        assert_eq!(expand_arithmetic("1 && 0", &mut context).unwrap(), "0");
        assert_eq!(expand_arithmetic("0 || 1", &mut context).unwrap(), "1");
        assert_eq!(expand_arithmetic("0 || 0", &mut context).unwrap(), "0");
    }

    #[test]
    fn test_arithmetic_bitwise() {
        let mut context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("~0", &mut context).unwrap(), "-1");
        assert_eq!(expand_arithmetic("5 & 3", &mut context).unwrap(), "1");
        assert_eq!(expand_arithmetic("5 | 3", &mut context).unwrap(), "7");
        assert_eq!(expand_arithmetic("5 ^ 3", &mut context).unwrap(), "6");
        assert_eq!(expand_arithmetic("1 << 4", &mut context).unwrap(), "16");
        assert_eq!(expand_arithmetic("16 >> 2", &mut context).unwrap(), "4");
    }

    #[test]
    fn test_arithmetic_ternary() {
        let mut context = ExpansionContext::new();

        assert_eq!(
            expand_arithmetic("1 ? 10 : 20", &mut context).unwrap(),
            "10"
        );
        assert_eq!(
            expand_arithmetic("0 ? 10 : 20", &mut context).unwrap(),
            "20"
        );
        assert_eq!(
            expand_arithmetic("5 > 3 ? 100 : 200", &mut context).unwrap(),
            "100"
        );
    }

    #[test]
    fn test_arithmetic_assignment() {
        let mut context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("x = 5", &mut context).unwrap(), "5");
        assert_eq!(context.get_var_or_env("x").unwrap(), "5");

        assert_eq!(expand_arithmetic("x += 3", &mut context).unwrap(), "8");
        assert_eq!(context.get_var_or_env("x").unwrap(), "8");

        assert_eq!(expand_arithmetic("x -= 3", &mut context).unwrap(), "5");
        assert_eq!(context.get_var_or_env("x").unwrap(), "5");
    }

    #[test]
    fn test_arithmetic_increment() {
        let mut context = ExpansionContext::new();
        context.set_var("x".to_string(), "5".to_string());

        assert_eq!(expand_arithmetic("++x", &mut context).unwrap(), "6");
        assert_eq!(context.get_var_or_env("x").unwrap(), "6");

        assert_eq!(expand_arithmetic("x++", &mut context).unwrap(), "6");
        assert_eq!(context.get_var_or_env("x").unwrap(), "7");
    }

    #[test]
    fn test_arithmetic_hex_octal() {
        let mut context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("0x10", &mut context).unwrap(), "16");
        assert_eq!(expand_arithmetic("010", &mut context).unwrap(), "8");
        assert_eq!(expand_arithmetic("0b1010", &mut context).unwrap(), "10");
    }

    #[test]
    fn test_arithmetic_short_circuit() {
        let mut context = ExpansionContext::new();

        // AND short-circuit: if left is 0 (false), right should NOT be evaluated
        context.set_var("x".to_string(), "5".to_string());
        assert_eq!(expand_arithmetic("0 && (x=10)", &mut context).unwrap(), "0");
        assert_eq!(context.get_var_or_env("x").unwrap(), "5"); // x unchanged

        // AND non-short-circuit: if left is 1 (true), right IS evaluated
        context.set_var("x".to_string(), "5".to_string());
        assert_eq!(expand_arithmetic("1 && (x=10)", &mut context).unwrap(), "1");
        assert_eq!(context.get_var_or_env("x").unwrap(), "10"); // x changed

        // OR short-circuit: if left is 1 (true), right should NOT be evaluated
        context.set_var("x".to_string(), "5".to_string());
        assert_eq!(expand_arithmetic("1 || (x=10)", &mut context).unwrap(), "1");
        assert_eq!(context.get_var_or_env("x").unwrap(), "5"); // x unchanged

        // OR non-short-circuit: if left is 0 (false), right IS evaluated
        context.set_var("x".to_string(), "5".to_string());
        assert_eq!(expand_arithmetic("0 || (x=10)", &mut context).unwrap(), "1");
        assert_eq!(context.get_var_or_env("x").unwrap(), "10"); // x changed
    }
}
