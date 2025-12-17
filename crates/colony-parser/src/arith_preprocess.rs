//! Arithmetic expansion preprocessing
//!
//! Finds $((expr)) in source and normalizes for pest parsing.
//! Needed because compound_atomic Word rules can't handle spaces inside $((  )).

/// Find and normalize arithmetic expansions
///
/// Returns (normalized_source, arith_map)
/// where arith_map: placeholder -> original_expression
pub fn preprocess_arithmetic(source: &str) -> (String, Vec<(String, String)>) {
    let mut result = String::new();
    let mut arith_map = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut arith_counter = 0;


    while i < chars.len() {
        // Look for $((
        if i + 2 < chars.len() && chars[i] == '$' && chars[i+1] == '(' && chars[i+2] == '(' {
            // Found $(( - find matching ))
            i += 3; // Skip $((

            let mut paren_count = 0;
            let mut expr_chars = Vec::new();
            let mut found_closing = false;

            // Collect characters until we find matching ))
            while i < chars.len() {
                let ch = chars[i];

                if ch == '(' {
                    paren_count += 1;
                    expr_chars.push(ch);
                    i += 1;
                } else if ch == ')' {
                    if paren_count == 0 {
                        // Could be closing ))
                        if i + 1 < chars.len() && chars[i+1] == ')' {
                            // Found matching ))!
                            i += 2; // Skip ))

                            let expr: String = expr_chars.iter().collect();
                            let placeholder = format!("__ARITH_{}__", arith_counter);
                            arith_counter += 1;


                            // Store original expression
                            arith_map.push((placeholder.clone(), expr));

                            // Replace with placeholder (no spaces, safe for Word parsing)
                            let replacement = format!("${{{}}}", placeholder);
                            result.push_str(&replacement);
                            found_closing = true;
                            break;
                        } else {
                            // Single ) but not ))
                            expr_chars.push(ch);
                            i += 1;
                        }
                    } else {
                        // Closing an inner (
                        paren_count -= 1;
                        expr_chars.push(ch);
                        i += 1;
                    }
                } else {
                    expr_chars.push(ch);
                    i += 1;
                }
            }

            // If we didn't find )), append literal $((
            if !found_closing {
                result.push_str("$((");
                for ch in expr_chars {
                    result.push(ch);
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    (result, arith_map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_arithmetic() {
        let source = "echo $((2+3))";
        let (normalized, map) = preprocess_arithmetic(source);

        assert_eq!(normalized, "echo ${__ARITH_0__}");
        assert_eq!(map.len(), 1);
        assert_eq!(map[0], ("__ARITH_0__".to_string(), "2+3".to_string()));
    }

    #[test]
    fn test_arithmetic_with_spaces() {
        let source = "echo $((2 + 3))";
        let (normalized, map) = preprocess_arithmetic(source);

        assert_eq!(normalized, "echo ${__ARITH_0__}");
        assert_eq!(map[0].1, "2 + 3");
    }

    #[test]
    fn test_nested_parens() {
        let source = "echo $(((2+3)*4))";
        let (normalized, map) = preprocess_arithmetic(source);

        assert_eq!(normalized, "echo ${__ARITH_0__}");
        assert_eq!(map[0].1, "(2+3)*4");
    }

    #[test]
    fn test_multiple_arithmetic() {
        let source = "echo $((1+1)) $((2*2))";
        let (normalized, map) = preprocess_arithmetic(source);

        assert_eq!(normalized, "echo ${__ARITH_0__} ${__ARITH_1__}");
        assert_eq!(map.len(), 2);
        assert_eq!(map[0].1, "1+1");
        assert_eq!(map[1].1, "2*2");
    }
}
