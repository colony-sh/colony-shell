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

/// Result of parameter expansion - can be single value or multiple (for array @ expansion)
#[derive(Debug, Clone)]
pub enum ExpandedValue {
    Single(String),
    Multiple(Vec<String>),
}

impl ExpandedValue {
    pub fn as_single(&self) -> String {
        match self {
            ExpandedValue::Single(s) => s.clone(),
            ExpandedValue::Multiple(v) => v.join(" "),
        }
    }
}

/// Expand a parameter, returning multiple values for ${arr[@]} expansion
pub fn expand_parameter_multi(var_ref: &VariableRef, context: &ExpansionContext) -> Result<ExpandedValue> {
    use colony_parser::{ArraySubscript, VarPrefix};
    
    let variable = context.get_variable(&var_ref.name);
    
    if let (Some(crate::Variable::Array(arr)), Some(ArraySubscript::All)) = (&variable, &var_ref.subscript) {
        if var_ref.prefix.is_none() && var_ref.modifier.is_none() {
            return Ok(ExpandedValue::Multiple(arr.clone()));
        }
    }
    
    expand_parameter(var_ref, context).map(ExpandedValue::Single)
}

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
                return match (&variable, &var_ref.subscript) {
                    (Some(crate::Variable::Array(arr)), Some(colony_parser::ArraySubscript::All | colony_parser::ArraySubscript::Star)) => {
                        Ok(arr.len().to_string())
                    }
                    (Some(crate::Variable::AssocArray(map)), Some(colony_parser::ArraySubscript::All | colony_parser::ArraySubscript::Star)) => {
                        Ok(map.len().to_string())
                    }
                    (Some(crate::Variable::Array(arr)), None) => {
                        Ok(arr.first().map(|s| s.len()).unwrap_or(0).to_string())
                    }
                    (Some(crate::Variable::String(s)), _) => {
                        Ok(s.len().to_string())
                    }
                    (None, _) => Ok("0".to_string()),
                    _ => Ok("0".to_string()),
                };
            }
            VarPrefix::Indices => {
                return match variable {
                    Some(crate::Variable::Array(arr)) => {
                        let indices: Vec<String> = arr.iter()
                            .enumerate()
                            .filter(|(_, v)| !v.is_empty())
                            .map(|(i, _)| i.to_string())
                            .collect();
                        Ok(indices.join(" "))
                    }
                    Some(crate::Variable::AssocArray(map)) => {
                        let keys: Vec<String> = map.keys().cloned().collect();
                        Ok(keys.join(" "))
                    }
                    _ => Ok(String::new()),
                };
            }
        }
    }

    // Handle array subscripts (without prefix)
    if let Some(subscript) = &var_ref.subscript {
        if let Some(crate::Variable::AssocArray(map)) = variable {
            return match subscript {
                colony_parser::ArraySubscript::Index(expr) => {
                    let key = get_subscript_key(expr, context);
                    Ok(map.get(&key).cloned().unwrap_or_default())
                }
                colony_parser::ArraySubscript::All | colony_parser::ArraySubscript::Star => {
                    Ok(map.values().cloned().collect::<Vec<_>>().join(" "))
                }
            };
        }
        
        let array = match variable {
            Some(crate::Variable::Array(arr)) => arr,
            Some(crate::Variable::String(s)) => {
                return match subscript {
                    colony_parser::ArraySubscript::Index(_) => Ok(s.clone()),
                    colony_parser::ArraySubscript::All | colony_parser::ArraySubscript::Star => Ok(s.clone()),
                };
            }
            Some(crate::Variable::AssocArray(_)) => unreachable!(),
            None => return Ok(String::new()),
        };

        let result = match subscript {
            colony_parser::ArraySubscript::Index(expr) => {
                match **expr {
                    colony_parser::Expression::Int(idx, _) => {
                        let index = if idx < 0 {
                            let len = array.len() as i64;
                            (len + idx) as usize
                        } else {
                            idx as usize
                        };
                        array.get(index).cloned().unwrap_or_default()
                    }
                    colony_parser::Expression::Unary { op: colony_parser::UnaryOp::Neg, ref operand, .. } => {
                        if let colony_parser::Expression::Int(n, _) = **operand {
                            let len = array.len() as i64;
                            let index = (len - n) as usize;
                            array.get(index).cloned().unwrap_or_default()
                        } else {
                            String::new()
                        }
                    }
                    colony_parser::Expression::Variable { ref name, .. } => {
                        if let Some(val) = context.get_var_or_env(name) {
                            if let Ok(idx) = val.parse::<i64>() {
                                let index = if idx < 0 {
                                    let len = array.len() as i64;
                                    (len + idx) as usize
                                } else {
                                    idx as usize
                                };
                                array.get(index).cloned().unwrap_or_default()
                            } else {
                                String::new()
                            }
                        } else {
                            String::new()
                        }
                    }
                    _ => String::new()
                }
            }
            colony_parser::ArraySubscript::All | colony_parser::ArraySubscript::Star => {
                if let Some(VarModifier::Slice { offset, length }) = &var_ref.modifier {
                    let len = array.len() as i64;
                    let start = if *offset < 0 {
                        (len + *offset).max(0) as usize
                    } else {
                        (*offset as usize).min(array.len())
                    };
                    let end = match length {
                        Some(l) => (start + l).min(array.len()),
                        None => array.len(),
                    };
                    return Ok(array[start..end].join(" "));
                }
                array.join(" ")
            }
        };
        return Ok(result);
    }

    // No subscript - handle modifiers
    match &var_ref.modifier {
        None => {
            // Simple ${var} or $var
            Ok(var_value.unwrap_or_default())
        }

        Some(VarModifier::Length) => {
            match variable {
                Some(crate::Variable::Array(arr)) => Ok(arr.len().to_string()),
                Some(crate::Variable::AssocArray(map)) => Ok(map.len().to_string()),
                Some(crate::Variable::String(s)) => Ok(s.len().to_string()),
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
            let should_use_alternate = if *check_null {
                var_value.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
            } else {
                var_value.is_some()
            };

            if should_use_alternate {
                Ok(word
                    .as_literal()
                    .unwrap_or("")
                    .to_string())
            } else {
                Ok(String::new())
            }
        }

        Some(VarModifier::Slice { offset, length }) => {
            match variable {
                Some(crate::Variable::Array(arr)) => {
                    let len = arr.len() as i64;
                    let start = if *offset < 0 {
                        (len + *offset).max(0) as usize
                    } else {
                        (*offset as usize).min(arr.len())
                    };
                    let end = match length {
                        Some(l) => (start + l).min(arr.len()),
                        None => arr.len(),
                    };
                    Ok(arr[start..end].join(" "))
                }
                Some(crate::Variable::AssocArray(map)) => {
                    let values: Vec<_> = map.values().cloned().collect();
                    let len = values.len() as i64;
                    let start = if *offset < 0 { (len + *offset).max(0) as usize } else { (*offset as usize).min(values.len()) };
                    let end = match length { Some(l) => (start + l).min(values.len()), None => values.len() };
                    Ok(values[start..end].join(" "))
                }
                Some(crate::Variable::String(s)) => {
                    let chars: Vec<char> = s.chars().collect();
                    let len = chars.len() as i64;
                    let start = if *offset < 0 {
                        (len + *offset).max(0) as usize
                    } else {
                        (*offset as usize).min(chars.len())
                    };
                    let end = match length {
                        Some(l) => (start + l).min(chars.len()),
                        None => chars.len(),
                    };
                    Ok(chars[start..end].iter().collect())
                }
                None => Ok(String::new()),
            }
        }
    }
}

fn get_subscript_key(expr: &colony_parser::Expression, context: &ExpansionContext) -> String {
    match expr {
        colony_parser::Expression::Variable { name, .. } => name.clone(),
        colony_parser::Expression::Str(s, _) => s.clone(),
        colony_parser::Expression::Int(n, _) => n.to_string(),
        _ => String::new()
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
