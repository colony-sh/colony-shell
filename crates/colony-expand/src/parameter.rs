//! Parameter expansion
//!
//! POSIX forms:
//! - ${var} - expand variable
//! - ${var:-word} - use word if var unset or null
//! - ${var-word} - use word if var unset (but not if null)
//! - ${var:=word} - assign word if var unset or null
//! - ${var=word} - assign word if var unset
//! - ${var:?word} - error if var unset or null
//! - ${var?word} - error if var unset
//! - ${var:+word} - use word if var set and not null
//! - ${var+word} - use word if var set
//! - ${#var} - length of var

use crate::{ExpansionContext, ExpansionError, Result};
use colony_parser::{VarModifier, VariableRef};

/// Expand a parameter (variable) reference
pub fn expand_parameter(var_ref: &VariableRef, context: &ExpansionContext) -> Result<String> {
    use colony_parser::VarPrefix;

    // Get variable (may be scalar or array)
    let variable = context.get_variable(&var_ref.name);
    let var_value = context.get_var_or_env(&var_ref.name);

    // Handle prefix modifiers (# and !)
    // These apply to the final result after subscript resolution
    if let Some(prefix) = var_ref.prefix {
        match prefix {
            VarPrefix::Length => {
                // ${#var} or ${#arr[@]}
                return match (&variable, &var_ref.subscript) {
                    (Some(crate::Variable::Array(arr)), Some(colony_parser::ArraySubscript::All | colony_parser::ArraySubscript::Star)) => {
                        // ${#arr[@]} or ${#arr[*]} - number of elements
                        Ok(arr.len().to_string())
                    }
                    (Some(crate::Variable::Array(arr)), None) => {
                        // ${#arr} - length of first element
                        Ok(arr.first().map(|s| s.len()).unwrap_or(0).to_string())
                    }
                    (Some(crate::Variable::String(s)), _) => {
                        // ${#var} - string length
                        Ok(s.len().to_string())
                    }
                    (None, _) => Ok("0".to_string()),
                    _ => Ok("0".to_string()),
                };
            }
            VarPrefix::Indices => {
                // ${!arr[@]} - array indices
                return match variable {
                    Some(crate::Variable::Array(arr)) => {
                        let indices: Vec<String> = (0..arr.len()).map(|i| i.to_string()).collect();
                        Ok(indices.join(" "))
                    }
                    _ => Ok(String::new()),
                };
            }
        }
    }

    // Handle array subscripts (without prefix)
    if let Some(subscript) = &var_ref.subscript {
        let array = match variable {
            Some(crate::Variable::Array(arr)) => arr,
            Some(crate::Variable::String(s)) => {
                // Treat scalar as array with one element at index 0
                return match subscript {
                    colony_parser::ArraySubscript::Index(expr) => {
                        // TODO: Evaluate expression to get index
                        // For now, treat as index 0
                        Ok(s.clone())
                    }
                    colony_parser::ArraySubscript::All | colony_parser::ArraySubscript::Star => {
                        Ok(s.clone())
                    }
                };
            }
            None => {
                // Undefined variable - return empty
                return Ok(String::new());
            }
        };

        return match subscript {
            colony_parser::ArraySubscript::Index(expr) => {
                // TODO: Evaluate expression to get numeric index
                // For now, parse as integer if possible
                // This is a simplification - full implementation needs expression evaluator
                if let colony_parser::Expression::Int(idx, _) = **expr {
                    let index = if idx < 0 {
                        // Negative index counts from end
                        let len = array.len() as i64;
                        (len + idx) as usize
                    } else {
                        idx as usize
                    };
                    Ok(array.get(index).cloned().unwrap_or_default())
                } else {
                    // Complex expression - not yet supported
                    Ok(String::new())
                }
            }
            colony_parser::ArraySubscript::All => {
                // ${arr[@]} - all elements as separate words
                // Space-separated for now (caller should handle word splitting)
                Ok(array.join(" "))
            }
            colony_parser::ArraySubscript::Star => {
                // ${arr[*]} - all elements as single word
                Ok(array.join(" "))
            }
        };
    }

    // No subscript - handle modifiers
    match &var_ref.modifier {
        None => {
            // Simple ${var} or $var
            Ok(var_value.unwrap_or_default())
        }

        Some(VarModifier::Length) => {
            // ${#var} or ${#arr[@]}
            match variable {
                Some(crate::Variable::Array(arr)) => {
                    Ok(arr.len().to_string())
                }
                Some(crate::Variable::String(s)) => {
                    Ok(s.len().to_string())
                }
                None => Ok("0".to_string()),
            }
        }

        Some(VarModifier::Indices) => {
            // ${!arr[@]} - array indices
            match variable {
                Some(crate::Variable::Array(arr)) => {
                    // Return indices: 0 1 2 3 ...
                    let indices: Vec<String> = (0..arr.len()).map(|i| i.to_string()).collect();
                    Ok(indices.join(" "))
                }
                _ => Ok(String::new()),
            }
        }

        Some(VarModifier::Default { word, check_null }) => {
            // ${var:-word} or ${var-word}
            let should_use_default = if *check_null {
                // :-  check for unset OR null (empty string)
                var_value.as_ref().map(|s| s.is_empty()).unwrap_or(true)
            } else {
                // -   check for unset only
                var_value.is_none()
            };

            if should_use_default {
                // TODO: Expand the word recursively
                // For now, just use literal
                Ok(word
                    .as_literal()
                    .unwrap_or("")
                    .to_string())
            } else {
                Ok(var_value.unwrap())
            }
        }

        Some(VarModifier::Assign { word, check_null }) => {
            // ${var:=word} or ${var=word}
            let should_assign = if *check_null {
                var_value.as_ref().map(|s| s.is_empty()).unwrap_or(true)
            } else {
                var_value.is_none()
            };

            if should_assign {
                // TODO: Expand the word recursively
                let value = word
                    .as_literal()
                    .unwrap_or("")
                    .to_string();
                // Note: We can't mutate context here because we only have &ExpansionContext
                // This is a design issue - assign needs mutable context
                // For now, just return the value
                Ok(value)
            } else {
                Ok(var_value.unwrap())
            }
        }

        Some(VarModifier::Error { word, check_null }) => {
            // ${var:?word} or ${var?word}
            let should_error = if *check_null {
                var_value.as_ref().map(|s| s.is_empty()).unwrap_or(true)
            } else {
                var_value.is_none()
            };

            if should_error {
                let message = word
                    .as_literal()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("{}: parameter unset", var_ref.name));
                Err(ExpansionError::UndefinedVariable(message))
            } else {
                Ok(var_value.unwrap())
            }
        }

        Some(VarModifier::Alternate { word, check_null }) => {
            // ${var:+word} or ${var+word}
            let should_use_alternate = if *check_null {
                // :+  var must be set AND non-null
                var_value.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
            } else {
                // +   var must be set (can be null)
                var_value.is_some()
            };

            if should_use_alternate {
                // TODO: Expand the word recursively
                Ok(word
                    .as_literal()
                    .unwrap_or("")
                    .to_string())
            } else {
                Ok(String::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::{Span, VariableRef, VarModifier, Word};

    fn make_var_ref(name: &str, modifier: Option<VarModifier>) -> VariableRef {
        VariableRef {
            name: name.to_string(),
            prefix: None,
            subscript: None,
            modifier,
            span: Span::DUMMY,
        }
    }

    fn make_word(text: &str) -> Box<Word> {
        Box::new(Word::literal(text, Span::DUMMY))
    }

    #[test]
    fn test_simple_expansion() {
        let mut context = ExpansionContext::new();
        context.set_var("FOO".to_string(), "bar".to_string());

        let var_ref = make_var_ref("FOO", None);
        let result = expand_parameter(&var_ref, &context).unwrap();
        assert_eq!(result, "bar");
    }

    #[test]
    fn test_undefined_expands_to_empty() {
        let context = ExpansionContext::new();
        let var_ref = make_var_ref("UNDEFINED", None);
        let result = expand_parameter(&var_ref, &context).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_length_modifier() {
        let mut context = ExpansionContext::new();
        context.set_var("FOO".to_string(), "hello".to_string());

        let var_ref = make_var_ref("FOO", Some(VarModifier::Length));
        let result = expand_parameter(&var_ref, &context).unwrap();
        assert_eq!(result, "5");
    }

    #[test]
    fn test_default_with_check_null() {
        let mut context = ExpansionContext::new();
        context.set_var("EMPTY".to_string(), "".to_string());

        let var_ref = make_var_ref(
            "EMPTY",
            Some(VarModifier::Default {
                word: make_word("default"),
                check_null: true, // :- form
            }),
        );
        let result = expand_parameter(&var_ref, &context).unwrap();
        assert_eq!(result, "default");
    }

    #[test]
    fn test_default_without_check_null() {
        let mut context = ExpansionContext::new();
        context.set_var("EMPTY".to_string(), "".to_string());

        let var_ref = make_var_ref(
            "EMPTY",
            Some(VarModifier::Default {
                word: make_word("default"),
                check_null: false, // - form
            }),
        );
        let result = expand_parameter(&var_ref, &context).unwrap();
        // Empty string is SET, so - form doesn't use default
        assert_eq!(result, "");
    }

    #[test]
    fn test_error_modifier() {
        let context = ExpansionContext::new();
        let var_ref = make_var_ref(
            "UNDEFINED",
            Some(VarModifier::Error {
                word: make_word("variable not set"),
                check_null: true,
            }),
        );
        let result = expand_parameter(&var_ref, &context);
        assert!(result.is_err());
    }
}
