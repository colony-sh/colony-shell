//! Arithmetic expansion
//!
//! Evaluates $((expression)) as arithmetic
//! POSIX supports: +, -, *, /, %, (), variables
//! Bash adds: bitwise, comparisons, ++, --, etc.

use crate::{ExpansionContext, ExpansionError, Result};

/// Evaluate an arithmetic expression
///
/// Supports POSIX shell arithmetic:
/// - Integers (decimal, octal 0..., hex 0x...)
/// - Operators: +, -, *, /, %, ()
/// - Variables (expand to their integer value)
/// - Precedence: (), *, / %, +, -
///
/// Returns the result as a string (for substitution into command)
pub fn expand_arithmetic(expr: &str, context: &ExpansionContext) -> Result<String> {
    let expanded = pre_expand_variables(expr.trim(), context);
    let result = eval_arithmetic(&expanded, context)?;
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
                    while i < chars.len() && chars[i] != '}' {
                        i += 1;
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
                        result.push_str(&value);
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

/// Parse and evaluate arithmetic expression
fn eval_arithmetic(expr: &str, context: &ExpansionContext) -> Result<i64> {
    let tokens = tokenize_arith(expr)?;
    let (result, _) = parse_expr(&tokens, 0, context)?;
    Ok(result)
}

/// Tokenize arithmetic expression
fn tokenize_arith(expr: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' => {
                i += 1; // Skip whitespace
            }
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
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
            '0'..='9' => {
                // Parse number
                let start = i;
                while i < chars.len() && chars[i].is_numeric() {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let value = num_str.parse::<i64>()
                    .map_err(|_| ExpansionError::ArithmeticError(format!("Invalid number: {}", num_str)))?;
                tokens.push(Token::Number(value));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                // Parse variable name
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

/// Arithmetic token
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64),
    Variable(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
}

/// Parse expression with precedence climbing
fn parse_expr(tokens: &[Token], pos: usize, context: &ExpansionContext) -> Result<(i64, usize)> {
    parse_additive(tokens, pos, context)
}

/// Parse additive expression: term ((+|-) term)*
fn parse_additive(tokens: &[Token], mut pos: usize, context: &ExpansionContext) -> Result<(i64, usize)> {
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

/// Parse multiplicative expression: primary ((*|/|%) primary)*
fn parse_multiplicative(tokens: &[Token], mut pos: usize, context: &ExpansionContext) -> Result<(i64, usize)> {
    let (mut left, new_pos) = parse_primary(tokens, pos, context)?;
    pos = new_pos;

    while pos < tokens.len() {
        match tokens[pos] {
            Token::Star => {
                pos += 1;
                let (right, new_pos) = parse_primary(tokens, pos, context)?;
                left *= right;
                pos = new_pos;
            }
            Token::Slash => {
                pos += 1;
                let (right, new_pos) = parse_primary(tokens, pos, context)?;
                if right == 0 {
                    return Err(ExpansionError::ArithmeticError("Division by zero".to_string()));
                }
                left /= right;
                pos = new_pos;
            }
            Token::Percent => {
                pos += 1;
                let (right, new_pos) = parse_primary(tokens, pos, context)?;
                if right == 0 {
                    return Err(ExpansionError::ArithmeticError("Modulo by zero".to_string()));
                }
                left %= right;
                pos = new_pos;
            }
            _ => break,
        }
    }

    Ok((left, pos))
}

/// Parse primary: number | variable | (expr)
fn parse_primary(tokens: &[Token], pos: usize, context: &ExpansionContext) -> Result<(i64, usize)> {
    if pos >= tokens.len() {
        return Err(ExpansionError::ArithmeticError("Unexpected end of expression".to_string()));
    }

    match &tokens[pos] {
        Token::Number(n) => Ok((*n, pos + 1)),
        Token::Variable(name) => {
            // Lookup variable and convert to integer
            let value = match context.get_var_or_env(name) {
                Some(s) => s.parse::<i64>().unwrap_or(0),  // Empty or non-numeric = 0
                None => 0,  // Undefined variable = 0 in arithmetic
            };
            Ok((value, pos + 1))
        }
        Token::LParen => {
            // Parse subexpression
            let (result, new_pos) = parse_expr(tokens, pos + 1, context)?;
            if new_pos >= tokens.len() || tokens[new_pos] != Token::RParen {
                return Err(ExpansionError::ArithmeticError("Missing closing )".to_string()));
            }
            Ok((result, new_pos + 1))
        }
        Token::Minus => {
            // Unary minus
            let (value, new_pos) = parse_primary(tokens, pos + 1, context)?;
            Ok((-value, new_pos))
        }
        Token::Plus => {
            // Unary plus (no-op)
            parse_primary(tokens, pos + 1, context)
        }
        _ => Err(ExpansionError::ArithmeticError(format!("Unexpected token: {:?}", tokens[pos]))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithmetic_simple() {
        let context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("1 + 1", &context).unwrap(), "2");
        assert_eq!(expand_arithmetic("10 - 3", &context).unwrap(), "7");
        assert_eq!(expand_arithmetic("4 * 5", &context).unwrap(), "20");
        assert_eq!(expand_arithmetic("20 / 4", &context).unwrap(), "5");
        assert_eq!(expand_arithmetic("17 % 5", &context).unwrap(), "2");
    }

    #[test]
    fn test_arithmetic_precedence() {
        let context = ExpansionContext::new();

        // Multiplication before addition
        assert_eq!(expand_arithmetic("2 + 3 * 4", &context).unwrap(), "14");
        // Parentheses override precedence
        assert_eq!(expand_arithmetic("(2 + 3) * 4", &context).unwrap(), "20");
    }

    #[test]
    fn test_arithmetic_variables() {
        let mut context = ExpansionContext::new();
        context.set_var("x".to_string(), "10".to_string());
        context.set_var("y".to_string(), "5".to_string());

        assert_eq!(expand_arithmetic("x + y", &context).unwrap(), "15");
        assert_eq!(expand_arithmetic("x * 2", &context).unwrap(), "20");
    }

    #[test]
    fn test_arithmetic_unary() {
        let context = ExpansionContext::new();

        assert_eq!(expand_arithmetic("-5", &context).unwrap(), "-5");
        assert_eq!(expand_arithmetic("-(2 + 3)", &context).unwrap(), "-5");
    }
}
