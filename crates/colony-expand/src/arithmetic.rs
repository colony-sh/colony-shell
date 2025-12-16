//! Arithmetic expansion
//!
//! Evaluates $((expression)) as arithmetic
//! POSIX supports: +, -, *, /, %, (), variables

use crate::{ExpansionContext, Result};

/// Evaluate an arithmetic expression
///
/// TODO: Implement proper arithmetic parser and evaluator
/// For now, returns "0"
pub fn expand_arithmetic(_expr: &str, _context: &ExpansionContext) -> Result<String> {
    // Placeholder - requires arithmetic evaluator
    Ok("0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithmetic_placeholder() {
        let context = ExpansionContext::new();
        let result = expand_arithmetic("1 + 1", &context).unwrap();
        assert_eq!(result, "0"); // Placeholder always returns "0"
    }
}
