//! Custom filter expression parser and evaluator for event listeners.
//!
//! Evaluates filter expressions like `$4 >= 500 AND $2 == "GET"` against
//! tuple values without invoking a shell. This prevents injection attacks
//! and provides zero-process-overhead filtering in the daemon.
//!
//! ## Numbering
//!
//! `$0` is the tuple key, `$1` is the first value field, etc. The same
//! numbering is used in handler environment variables (`$TUPLE_0`, `$TUPLE_1`).
//!
//! ## Supported Operators
//!
//! - `==`, `!=` — string equality (or numeric if both sides parse as numbers)
//! - `>`, `>=`, `<`, `<=` — numeric comparison (coerces strings to f64)
//! - `~` — substring/contains check
//! - `AND`, `OR` — boolean combinators
//! - `NOT` — boolean negation
//! - Parentheses for grouping

/// Errors from filter parsing or evaluation.
#[derive(Debug, thiserror::Error)]
pub enum FilterError {
    #[error("unexpected token at position {pos}: {message}")]
    UnexpectedToken { pos: usize, message: String },

    #[error("unexpected end of expression")]
    UnexpectedEnd,

    #[error("invalid variable reference: {0}")]
    InvalidVar(String),

    #[error("type error: {0}")]
    TypeError(String),
}

/// A parsed filter expression AST node.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterExpr {
    /// Comparison: `$N op value`
    Compare {
        var_index: usize,
        op: CompareOp,
        value: LiteralValue,
    },
    /// Boolean AND
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// Boolean OR
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// Boolean NOT
    Not(Box<FilterExpr>),
    /// Always true (empty filter)
    True,
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,       // ==
    NotEq,    // !=
    Gt,       // >
    Gte,      // >=
    Lt,       // <
    Lte,      // <=
    Contains, // ~
}

/// A literal value in a filter expression.
#[derive(Debug, Clone, PartialEq)]
pub enum LiteralValue {
    Str(String),
    Num(f64),
}

/// Token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Var(usize),         // $N
    StringLit(String),  // "..."
    NumberLit(f64),     // 123 or 123.45
    Eq,                 // ==
    NotEq,              // !=
    Gt,                 // >
    Gte,                // >=
    Lt,                 // <
    Lte,                // <=
    Contains,           // ~
    And,                // AND
    Or,                 // OR
    Not,                // NOT
    LParen,             // (
    RParen,             // )
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// Tokenize a filter expression string.
fn tokenize(input: &str) -> Result<Vec<Token>, FilterError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace
        if chars[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        match chars[i] {
            '$' => {
                i += 1;
                let start = i;
                while i < len && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i == start {
                    return Err(FilterError::InvalidVar("$ not followed by number".to_string()));
                }
                let num_str: String = chars[start..i].iter().collect();
                let index: usize = num_str.parse().map_err(|_| {
                    FilterError::InvalidVar(format!("${num_str}"))
                })?;
                tokens.push(Token::Var(index));
            }

            '"' => {
                i += 1; // skip opening quote
                let mut s = String::new();
                while i < len && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < len {
                        i += 1;
                        match chars[i] {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            '\\' => s.push('\\'),
                            '"' => s.push('"'),
                            other => {
                                s.push('\\');
                                s.push(other);
                            }
                        }
                    } else {
                        s.push(chars[i]);
                    }
                    i += 1;
                }
                if i >= len {
                    return Err(FilterError::UnexpectedEnd);
                }
                i += 1; // skip closing quote
                tokens.push(Token::StringLit(s));
            }

            '\'' => {
                i += 1; // skip opening quote
                let mut s = String::new();
                while i < len && chars[i] != '\'' {
                    s.push(chars[i]);
                    i += 1;
                }
                if i >= len {
                    return Err(FilterError::UnexpectedEnd);
                }
                i += 1; // skip closing quote
                tokens.push(Token::StringLit(s));
            }

            '=' if i + 1 < len && chars[i + 1] == '=' => {
                tokens.push(Token::Eq);
                i += 2;
            }

            '!' if i + 1 < len && chars[i + 1] == '=' => {
                tokens.push(Token::NotEq);
                i += 2;
            }

            '>' if i + 1 < len && chars[i + 1] == '=' => {
                tokens.push(Token::Gte);
                i += 2;
            }

            '>' => {
                tokens.push(Token::Gt);
                i += 1;
            }

            '<' if i + 1 < len && chars[i + 1] == '=' => {
                tokens.push(Token::Lte);
                i += 2;
            }

            '<' => {
                tokens.push(Token::Lt);
                i += 1;
            }

            '~' => {
                tokens.push(Token::Contains);
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

            c if c.is_ascii_digit() || (c == '-' && i + 1 < len && chars[i + 1].is_ascii_digit()) => {
                let start = i;
                if c == '-' {
                    i += 1;
                }
                while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let num: f64 = num_str.parse().map_err(|_| {
                    FilterError::UnexpectedToken {
                        pos: start,
                        message: format!("invalid number: {num_str}"),
                    }
                })?;
                tokens.push(Token::NumberLit(num));
            }

            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < len && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                match word.to_uppercase().as_str() {
                    "AND" => tokens.push(Token::And),
                    "OR" => tokens.push(Token::Or),
                    "NOT" => tokens.push(Token::Not),
                    _ => {
                        return Err(FilterError::UnexpectedToken {
                            pos: start,
                            message: format!("unknown keyword: {word}"),
                        });
                    }
                }
            }

            _ => {
                return Err(FilterError::UnexpectedToken {
                    pos: i,
                    message: format!("unexpected character: {:?}", chars[i]),
                });
            }
        }
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Parser (recursive descent)
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        tok
    }

    /// Parse a complete filter expression.
    ///
    /// Grammar:
    ///   expr     = or_expr
    ///   or_expr  = and_expr ("OR" and_expr)*
    ///   and_expr = not_expr ("AND" not_expr)*
    ///   not_expr = "NOT" not_expr | primary
    ///   primary  = comparison | "(" expr ")"
    ///   comparison = VAR op literal
    ///   op       = "==" | "!=" | ">" | ">=" | "<" | "<=" | "~"
    ///   literal  = STRING | NUMBER
    fn parse(&mut self) -> Result<FilterExpr, FilterError> {
        let expr = self.parse_or()?;

        if self.pos < self.tokens.len() {
            return Err(FilterError::UnexpectedToken {
                pos: self.pos,
                message: format!("unexpected token after expression: {:?}", self.tokens[self.pos]),
            });
        }

        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<FilterExpr, FilterError> {
        let mut left = self.parse_and()?;

        while self.peek() == Some(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = FilterExpr::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<FilterExpr, FilterError> {
        let mut left = self.parse_not()?;

        while self.peek() == Some(&Token::And) {
            self.advance();
            let right = self.parse_not()?;
            left = FilterExpr::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_not(&mut self) -> Result<FilterExpr, FilterError> {
        if self.peek() == Some(&Token::Not) {
            self.advance();
            let expr = self.parse_not()?;
            return Ok(FilterExpr::Not(Box::new(expr)));
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<FilterExpr, FilterError> {
        if self.peek() == Some(&Token::LParen) {
            self.advance();
            let expr = self.parse_or()?;
            if self.advance() != Some(Token::RParen) {
                return Err(FilterError::UnexpectedEnd);
            }
            return Ok(expr);
        }

        // Must be a comparison: $N op literal
        let Some(Token::Var(index)) = self.advance() else {
            return Err(FilterError::UnexpectedToken {
                pos: self.pos.saturating_sub(1),
                message: "expected variable ($N)".to_string(),
            });
        };

        let op = match self.advance() {
            Some(Token::Eq) => CompareOp::Eq,
            Some(Token::NotEq) => CompareOp::NotEq,
            Some(Token::Gt) => CompareOp::Gt,
            Some(Token::Gte) => CompareOp::Gte,
            Some(Token::Lt) => CompareOp::Lt,
            Some(Token::Lte) => CompareOp::Lte,
            Some(Token::Contains) => CompareOp::Contains,
            _ => {
                return Err(FilterError::UnexpectedToken {
                    pos: self.pos.saturating_sub(1),
                    message: "expected comparison operator".to_string(),
                });
            }
        };

        let value = match self.advance() {
            Some(Token::StringLit(s)) => LiteralValue::Str(s),
            Some(Token::NumberLit(n)) => LiteralValue::Num(n),
            _ => {
                return Err(FilterError::UnexpectedToken {
                    pos: self.pos.saturating_sub(1),
                    message: "expected string or number literal".to_string(),
                });
            }
        };

        Ok(FilterExpr::Compare {
            var_index: index,
            op,
            value,
        })
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a filter expression string into an AST.
///
/// Returns `FilterExpr::True` for empty strings.
pub fn parse_filter(input: &str) -> Result<FilterExpr, FilterError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(FilterExpr::True);
    }

    let tokens = tokenize(trimmed)?;
    if tokens.is_empty() {
        return Ok(FilterExpr::True);
    }

    let mut parser = Parser::new(tokens);
    parser.parse()
}

/// Evaluate a parsed filter expression against tuple values.
///
/// `values` is indexed by `$N`: `values[0]` is `$0` (the key),
/// `values[1]` is `$1` (first value field), etc.
///
/// Returns `true` if the filter matches, `false` otherwise.
/// Out-of-bounds variable references evaluate to empty string.
pub fn evaluate_filter(expr: &FilterExpr, values: &[String]) -> bool {
    match expr {
        FilterExpr::True => true,

        FilterExpr::Compare { var_index, op, value } => {
            let var_str = values.get(*var_index).map(String::as_str).unwrap_or("");
            eval_compare(var_str, *op, value)
        }

        FilterExpr::And(left, right) => {
            evaluate_filter(left, values) && evaluate_filter(right, values)
        }

        FilterExpr::Or(left, right) => {
            evaluate_filter(left, values) || evaluate_filter(right, values)
        }

        FilterExpr::Not(inner) => !evaluate_filter(inner, values),
    }
}

/// Convenience: parse and evaluate in one call.
///
/// Returns `true` if the filter string is empty.
pub fn matches_filter(filter: &str, values: &[String]) -> Result<bool, FilterError> {
    let expr = parse_filter(filter)?;
    Ok(evaluate_filter(&expr, values))
}

/// Evaluate a single comparison.
fn eval_compare(var_str: &str, op: CompareOp, value: &LiteralValue) -> bool {
    match op {
        CompareOp::Contains => {
            let pattern = match value {
                LiteralValue::Str(s) => s.as_str(),
                LiteralValue::Num(n) => return var_str.contains(&n.to_string()),
            };
            var_str.contains(pattern)
        }

        CompareOp::Eq => match value {
            LiteralValue::Str(s) => var_str == s,
            LiteralValue::Num(n) => var_str.parse::<f64>().is_ok_and(|v| (v - n).abs() < f64::EPSILON),
        },

        CompareOp::NotEq => match value {
            LiteralValue::Str(s) => var_str != s,
            LiteralValue::Num(n) => var_str
                .parse::<f64>()
                .map_or(true, |v| (v - n).abs() >= f64::EPSILON),
        },

        CompareOp::Gt | CompareOp::Gte | CompareOp::Lt | CompareOp::Lte => {
            let target = match value {
                LiteralValue::Num(n) => *n,
                LiteralValue::Str(s) => {
                    let Ok(n) = s.parse::<f64>() else {
                        // Fall back to string comparison
                        return match op {
                            CompareOp::Gt => var_str > s.as_str(),
                            CompareOp::Gte => var_str >= s.as_str(),
                            CompareOp::Lt => var_str < s.as_str(),
                            CompareOp::Lte => var_str <= s.as_str(),
                            _ => false,
                        };
                    };
                    n
                }
            };

            let Ok(var_num) = var_str.parse::<f64>() else {
                return false; // non-numeric var with numeric comparison = false
            };

            match op {
                CompareOp::Gt => var_num > target,
                CompareOp::Gte => var_num >= target,
                CompareOp::Lt => var_num < target,
                CompareOp::Lte => var_num <= target,
                _ => false,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // --- Lexer tests ---

    #[test]
    fn test_tokenize_simple() {
        let tokens = tokenize("$4 >= 500").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], Token::Var(4));
        assert_eq!(tokens[1], Token::Gte);
        assert_eq!(tokens[2], Token::NumberLit(500.0));
    }

    #[test]
    fn test_tokenize_string_literal() {
        let tokens = tokenize("$2 == \"GET\"").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], Token::Var(2));
        assert_eq!(tokens[1], Token::Eq);
        assert_eq!(tokens[2], Token::StringLit("GET".to_string()));
    }

    #[test]
    fn test_tokenize_and_or() {
        let tokens = tokenize("$1 == \"a\" AND $2 == \"b\"").unwrap();
        assert_eq!(tokens.len(), 7);
        assert_eq!(tokens[3], Token::And);
    }

    #[test]
    fn test_tokenize_single_quoted() {
        let tokens = tokenize("$1 ~ '/api/'").unwrap();
        assert_eq!(tokens[2], Token::StringLit("/api/".to_string()));
    }

    // --- Parser tests ---

    #[test]
    fn test_parse_simple_comparison() {
        let expr = parse_filter("$4 >= 500").unwrap();
        assert!(matches!(
            expr,
            FilterExpr::Compare {
                var_index: 4,
                op: CompareOp::Gte,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_and() {
        let expr = parse_filter("$2 == \"POST\" AND $4 >= 400").unwrap();
        assert!(matches!(expr, FilterExpr::And(_, _)));
    }

    #[test]
    fn test_parse_or() {
        let expr = parse_filter("$1 == \"a\" OR $1 == \"b\"").unwrap();
        assert!(matches!(expr, FilterExpr::Or(_, _)));
    }

    #[test]
    fn test_parse_not() {
        let expr = parse_filter("NOT $1 == \"skip\"").unwrap();
        assert!(matches!(expr, FilterExpr::Not(_)));
    }

    #[test]
    fn test_parse_parentheses() {
        let expr = parse_filter("($1 == \"a\" OR $1 == \"b\") AND $2 >= 100").unwrap();
        assert!(matches!(expr, FilterExpr::And(_, _)));
    }

    #[test]
    fn test_parse_empty() {
        let expr = parse_filter("").unwrap();
        assert_eq!(expr, FilterExpr::True);
    }

    #[test]
    fn test_parse_whitespace_only() {
        let expr = parse_filter("   ").unwrap();
        assert_eq!(expr, FilterExpr::True);
    }

    // --- Evaluator tests ---

    #[test]
    fn test_eval_true() {
        assert!(evaluate_filter(&FilterExpr::True, &[]));
    }

    #[test]
    fn test_eval_eq_string() {
        let expr = parse_filter("$2 == \"GET\"").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["http_request", "sess-1", "GET", "/api"])));
        assert!(!evaluate_filter(&expr, &vals(&["http_request", "sess-1", "POST", "/api"])));
    }

    #[test]
    fn test_eval_gte_numeric() {
        let expr = parse_filter("$4 >= 500").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["http", "s", "GET", "/", "500"])));
        assert!(evaluate_filter(&expr, &vals(&["http", "s", "GET", "/", "503"])));
        assert!(!evaluate_filter(&expr, &vals(&["http", "s", "GET", "/", "200"])));
    }

    #[test]
    fn test_eval_contains() {
        let expr = parse_filter("$3 ~ \"/api/\"").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["http", "s", "GET", "/api/status"])));
        assert!(!evaluate_filter(&expr, &vals(&["http", "s", "GET", "/index.html"])));
    }

    #[test]
    fn test_eval_and() {
        let expr = parse_filter("$2 == \"POST\" AND $4 >= 400").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["http", "s", "POST", "/api", "500"])));
        assert!(!evaluate_filter(&expr, &vals(&["http", "s", "GET", "/api", "500"])));
        assert!(!evaluate_filter(&expr, &vals(&["http", "s", "POST", "/api", "200"])));
    }

    #[test]
    fn test_eval_or() {
        let expr = parse_filter("$1 == \"alert\" OR $1 == \"critical\"").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["hb", "alert"])));
        assert!(evaluate_filter(&expr, &vals(&["hb", "critical"])));
        assert!(!evaluate_filter(&expr, &vals(&["hb", "info"])));
    }

    #[test]
    fn test_eval_not() {
        let expr = parse_filter("NOT $1 == \"skip\"").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["ev", "process"])));
        assert!(!evaluate_filter(&expr, &vals(&["ev", "skip"])));
    }

    #[test]
    fn test_eval_complex() {
        let expr = parse_filter("($2 == \"POST\" OR $2 == \"PUT\") AND $4 >= 400").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["http", "s", "POST", "/", "500"])));
        assert!(evaluate_filter(&expr, &vals(&["http", "s", "PUT", "/", "400"])));
        assert!(!evaluate_filter(&expr, &vals(&["http", "s", "GET", "/", "500"])));
        assert!(!evaluate_filter(&expr, &vals(&["http", "s", "POST", "/", "200"])));
    }

    #[test]
    fn test_eval_out_of_bounds_var() {
        let expr = parse_filter("$99 == \"\"").unwrap();
        // Out-of-bounds var should be empty string
        assert!(evaluate_filter(&expr, &vals(&["only_key"])));
    }

    #[test]
    fn test_eval_not_eq() {
        let expr = parse_filter("$1 != \"skip\"").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["ev", "process"])));
        assert!(!evaluate_filter(&expr, &vals(&["ev", "skip"])));
    }

    #[test]
    fn test_eval_lt() {
        let expr = parse_filter("$1 < 100").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["ev", "50"])));
        assert!(!evaluate_filter(&expr, &vals(&["ev", "100"])));
        assert!(!evaluate_filter(&expr, &vals(&["ev", "200"])));
    }

    #[test]
    fn test_eval_lte() {
        let expr = parse_filter("$1 <= 100").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["ev", "100"])));
        assert!(!evaluate_filter(&expr, &vals(&["ev", "101"])));
    }

    #[test]
    fn test_matches_filter_convenience() {
        assert!(matches_filter("", &vals(&["a", "b"])).unwrap());
        assert!(matches_filter("$0 == \"a\"", &vals(&["a", "b"])).unwrap());
        assert!(!matches_filter("$0 == \"c\"", &vals(&["a", "b"])).unwrap());
    }

    // --- Error tests ---

    #[test]
    fn test_parse_error_bad_var() {
        assert!(parse_filter("$ == 1").is_err());
    }

    #[test]
    fn test_parse_error_unclosed_string() {
        assert!(parse_filter("$1 == \"hello").is_err());
    }

    #[test]
    fn test_parse_error_missing_operator() {
        assert!(parse_filter("$1 \"hello\"").is_err());
    }

    #[test]
    fn test_parse_error_unknown_keyword() {
        assert!(parse_filter("$1 XOR $2").is_err());
    }

    #[test]
    fn test_negative_number() {
        let expr = parse_filter("$1 > -5").unwrap();
        assert!(evaluate_filter(&expr, &vals(&["ev", "0"])));
        assert!(!evaluate_filter(&expr, &vals(&["ev", "-10"])));
    }
}
