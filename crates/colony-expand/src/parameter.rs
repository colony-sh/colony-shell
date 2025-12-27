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

use crate::{expand_tilde, ExpansionContext, ExpansionError, Result};
use colony_parser::{VarModifier, VariableRef, Word, WordPart};

fn expand_word_to_i64(word: &Word, context: &ExpansionContext) -> i64 {
    let expanded = expand_simple_word(word, context);
    expanded.trim().parse().unwrap_or(0)
}

fn expand_word_to_usize(word: &Word, context: &ExpansionContext) -> usize {
    let expanded = expand_simple_word(word, context);
    expanded.trim().parse().unwrap_or(0)
}

fn expand_simple_word(word: &Word, context: &ExpansionContext) -> String {
    let mut result = String::new();
    for part in &word.parts {
        match part {
            WordPart::Literal(s) | WordPart::SingleQuoted(s) => result.push_str(s),
            WordPart::Tilde(user) => {
                if let Ok(expanded) = expand_tilde(user.as_deref(), context) {
                    result.push_str(&expanded);
                }
            }
            WordPart::Variable(var_ref) => {
                if let Some(val) = context.get_var_or_env(&var_ref.name) {
                    result.push_str(&val);
                }
            }
            WordPart::DoubleQuoted(inner) => {
                for inner_part in inner {
                    match inner_part {
                        WordPart::Literal(s) => result.push_str(s),
                        WordPart::Variable(var_ref) => {
                            if let Some(val) = context.get_var_or_env(&var_ref.name) {
                                result.push_str(&val);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    result
}

/// Result of parameter expansion - can be single value or multiple (for array @ expansion)
#[derive(Debug, Clone)]
pub enum ExpandedValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Default)]
pub struct ExpansionSideEffect {
    pub assignment: Option<(String, String)>,
}

impl ExpansionSideEffect {
    pub fn with_assignment(name: String, value: String) -> Self {
        Self { assignment: Some((name, value)) }
    }
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

    let variable = context.get_variable(&var_ref.name);
    let var_value = context.get_var_or_env(&var_ref.name);

    // Handle prefix modifiers (# and !)
    // These apply to the final result after subscript resolution
    if let Some(prefix) = var_ref.prefix {
        match prefix {
            VarPrefix::Length => {
                if var_ref.name == "@" || var_ref.name == "*" {
                    return Ok(context.positional_param_count().to_string());
                }
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
                    (None, _) => {
                        match &var_value {
                            Some(s) => Ok(s.len().to_string()),
                            None => Ok("0".to_string()),
                        }
                    }
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
            Some(crate::Variable::NameRef(target)) => {
                if let Some(crate::Variable::Array(arr)) = context.get_variable(target) {
                    arr
                } else {
                    return Ok(context.get_var_or_env(target).unwrap_or_default());
                }
            }
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
                    let offset_val = expand_word_to_i64(offset, context);
                    let len = array.len() as i64;
                    let start = if offset_val < 0 {
                        (len + offset_val).max(0) as usize
                    } else {
                        (offset_val as usize).min(array.len())
                    };
                    let end = match length {
                        Some(l) => (start + expand_word_to_usize(l, context)).min(array.len()),
                        None => array.len(),
                    };
                    return Ok(array[start..end].join(" "));
                }
                
                if let Some(case_mod) = &var_ref.modifier {
                    match case_mod {
                        VarModifier::UpperCase { all } => {
                            let modified: Vec<String> = array.iter()
                                .map(|s| apply_case_modification(s, CaseOp::Upper, *all))
                                .collect();
                            return Ok(modified.join(" "));
                        }
                        VarModifier::LowerCase { all } => {
                            let modified: Vec<String> = array.iter()
                                .map(|s| apply_case_modification(s, CaseOp::Lower, *all))
                                .collect();
                            return Ok(modified.join(" "));
                        }
                        VarModifier::ToggleCase { all } => {
                            let modified: Vec<String> = array.iter()
                                .map(|s| apply_case_modification(s, CaseOp::Toggle, *all))
                                .collect();
                            return Ok(modified.join(" "));
                        }
                        VarModifier::Substitute { pattern, replacement, all } => {
                            let pat_str = expand_simple_word(pattern, context);
                            let repl_str = expand_simple_word(replacement, context);
                            if !pat_str.is_empty() {
                                let modified: Vec<String> = array.iter()
                                    .map(|s| {
                                        if *all {
                                            s.replace(&pat_str, &repl_str)
                                        } else {
                                            s.replacen(&pat_str, &repl_str, 1)
                                        }
                                    })
                                    .collect();
                                return Ok(modified.join(" "));
                            }
                        }
                        _ => {}
                    }
                }
                
                array.join(" ")
            }
        };
        
        let result = if let Some(case_mod) = &var_ref.modifier {
            match case_mod {
                VarModifier::UpperCase { all } => apply_case_modification(&result, CaseOp::Upper, *all),
                VarModifier::LowerCase { all } => apply_case_modification(&result, CaseOp::Lower, *all),
                VarModifier::ToggleCase { all } => apply_case_modification(&result, CaseOp::Toggle, *all),
                _ => result,
            }
        } else {
            result
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
                Some(crate::Variable::NameRef(target)) => {
                    if let Some(v) = context.get_var_or_env(target) {
                        Ok(v.len().to_string())
                    } else {
                        Ok("0".to_string())
                    }
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
            let should_use_default = if *check_null {
                var_value.as_ref().map(|s| s.is_empty()).unwrap_or(true)
            } else {
                var_value.is_none()
            };

            if should_use_default {
                Ok(expand_simple_word(word, context))
            } else {
                Ok(var_value.unwrap())
            }
        }

        Some(VarModifier::Assign { word, check_null }) => {
            let should_assign = if *check_null {
                var_value.as_ref().map(|s| s.is_empty()).unwrap_or(true)
            } else {
                var_value.is_none()
            };

            if should_assign {
                Ok(expand_simple_word(word, context))
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
                Ok(expand_simple_word(word, context))
            } else {
                Ok(String::new())
            }
        }

        Some(VarModifier::Slice { offset, length }) => {
            let offset_val = expand_word_to_i64(offset, context);
            
            if var_ref.name == "@" || var_ref.name == "*" {
                let params = context.get_all_positional_params();
                let len = params.len() as i64;
                let positional_start = if offset_val < 0 {
                    (len + offset_val).max(0) as usize
                } else if offset_val == 0 {
                    0
                } else {
                    ((offset_val - 1) as usize).min(params.len())
                };
                let end = match length {
                    Some(l) => (positional_start + expand_word_to_usize(l, context)).min(params.len()),
                    None => params.len(),
                };
                return Ok(params[positional_start..end].join(" "));
            }
            
            match variable {
                Some(crate::Variable::Array(arr)) => {
                    let len = arr.len() as i64;
                    let start = if offset_val < 0 {
                        (len + offset_val).max(0) as usize
                    } else {
                        (offset_val as usize).min(arr.len())
                    };
                    let end = match length {
                        Some(l) => (start + expand_word_to_usize(l, context)).min(arr.len()),
                        None => arr.len(),
                    };
                    Ok(arr[start..end].join(" "))
                }
                Some(crate::Variable::AssocArray(map)) => {
                    let values: Vec<_> = map.values().cloned().collect();
                    let len = values.len() as i64;
                    let start = if offset_val < 0 { (len + offset_val).max(0) as usize } else { (offset_val as usize).min(values.len()) };
                    let end = match length { Some(l) => (start + expand_word_to_usize(l, context)).min(values.len()), None => values.len() };
                    Ok(values[start..end].join(" "))
                }
                Some(crate::Variable::String(s)) => {
                    let chars: Vec<char> = s.chars().collect();
                    let len = chars.len() as i64;
                    let start = if offset_val < 0 {
                        (len + offset_val).max(0) as usize
                    } else {
                        (offset_val as usize).min(chars.len())
                    };
                    let end = match length {
                        Some(l) => (start + expand_word_to_usize(l, context)).min(chars.len()),
                        None => chars.len(),
                    };
                    Ok(chars[start..end].iter().collect())
                }
                Some(crate::Variable::NameRef(target)) => {
                    if let Some(v) = context.get_var_or_env(target) {
                        let chars: Vec<char> = v.chars().collect();
                        let len = chars.len() as i64;
                        let start = if offset_val < 0 { (len + offset_val).max(0) as usize } else { (offset_val as usize).min(chars.len()) };
                        let end = match length { Some(l) => (start + expand_word_to_usize(l, context)).min(chars.len()), None => chars.len() };
                        Ok(chars[start..end].iter().collect())
                    } else {
                        Ok(String::new())
                    }
                }
                None => Ok(String::new()),
            }
        }

        Some(VarModifier::StripPrefix { pattern }) => {
            let pat = expand_simple_word(pattern, context);
            let value = var_value.unwrap_or_default();
            Ok(strip_prefix_shortest(&value, &pat))
        }

        Some(VarModifier::StripPrefixGreedy { pattern }) => {
            let pat = expand_simple_word(pattern, context);
            let value = var_value.unwrap_or_default();
            Ok(strip_prefix_longest(&value, &pat))
        }

        Some(VarModifier::StripSuffix { pattern }) => {
            let pat = expand_simple_word(pattern, context);
            let value = var_value.unwrap_or_default();
            Ok(strip_suffix_shortest(&value, &pat))
        }

        Some(VarModifier::StripSuffixGreedy { pattern }) => {
            let pat = expand_simple_word(pattern, context);
            let value = var_value.unwrap_or_default();
            Ok(strip_suffix_longest(&value, &pat))
        }

        Some(VarModifier::UpperCase { all }) => {
            let value = var_value.unwrap_or_default();
            Ok(apply_case_modification(&value, CaseOp::Upper, *all))
        }

        Some(VarModifier::LowerCase { all }) => {
            let value = var_value.unwrap_or_default();
            Ok(apply_case_modification(&value, CaseOp::Lower, *all))
        }

        Some(VarModifier::ToggleCase { all }) => {
            let value = var_value.unwrap_or_default();
            Ok(apply_case_modification(&value, CaseOp::Toggle, *all))
        }

        Some(VarModifier::Substitute { pattern, replacement, all: replace_all }) => {
            let pat_str = expand_simple_word(pattern, context);
            let repl_str = expand_simple_word(replacement, context);
            
            if pat_str.is_empty() {
                return Ok(var_value.unwrap_or_default());
            }
            
            if var_ref.name == "@" || var_ref.name == "*" {
                let params = context.get_all_positional_params();
                let modified: Vec<String> = params.iter()
                    .map(|s| {
                        if *replace_all {
                            s.replace(&pat_str, &repl_str)
                        } else {
                            s.replacen(&pat_str, &repl_str, 1)
                        }
                    })
                    .collect();
                return Ok(modified.join(" "));
            }
            
            let value = var_value.unwrap_or_default();
            if *replace_all {
                Ok(value.replace(&pat_str, &repl_str))
            } else {
                Ok(value.replacen(&pat_str, &repl_str, 1))
            }
        }
    }
}

fn strip_prefix_shortest(value: &str, pattern: &str) -> String {
    if pattern.is_empty() {
        return value.to_string();
    }
    let glob_pattern = glob::Pattern::new(pattern).ok();
    for i in 0..=value.len() {
        let prefix = &value[..i];
        if let Some(ref pat) = glob_pattern {
            if pat.matches(prefix) {
                return value[i..].to_string();
            }
        } else if prefix == pattern {
            return value[i..].to_string();
        }
    }
    value.to_string()
}

fn strip_prefix_longest(value: &str, pattern: &str) -> String {
    if pattern.is_empty() {
        return value.to_string();
    }
    let glob_pattern = glob::Pattern::new(pattern).ok();
    for i in (0..=value.len()).rev() {
        let prefix = &value[..i];
        if let Some(ref pat) = glob_pattern {
            if pat.matches(prefix) {
                return value[i..].to_string();
            }
        } else if prefix == pattern {
            return value[i..].to_string();
        }
    }
    value.to_string()
}

fn strip_suffix_shortest(value: &str, pattern: &str) -> String {
    if pattern.is_empty() {
        return value.to_string();
    }
    let glob_pattern = glob::Pattern::new(pattern).ok();
    for i in (0..=value.len()).rev() {
        let suffix = &value[i..];
        if let Some(ref pat) = glob_pattern {
            if pat.matches(suffix) {
                return value[..i].to_string();
            }
        } else if suffix == pattern {
            return value[..i].to_string();
        }
    }
    value.to_string()
}

fn strip_suffix_longest(value: &str, pattern: &str) -> String {
    if pattern.is_empty() {
        return value.to_string();
    }
    let glob_pattern = glob::Pattern::new(pattern).ok();
    for i in 0..=value.len() {
        let suffix = &value[i..];
        if let Some(ref pat) = glob_pattern {
            if pat.matches(suffix) {
                return value[..i].to_string();
            }
        } else if suffix == pattern {
            return value[..i].to_string();
        }
    }
    value.to_string()
}

enum CaseOp {
    Upper,
    Lower,
    Toggle,
}

fn apply_case_modification(value: &str, op: CaseOp, all: bool) -> String {
    if value.is_empty() {
        return String::new();
    }
    
    let mut chars: Vec<char> = value.chars().collect();
    
    if all {
        for c in chars.iter_mut() {
            *c = match op {
                CaseOp::Upper => c.to_uppercase().next().unwrap_or(*c),
                CaseOp::Lower => c.to_lowercase().next().unwrap_or(*c),
                CaseOp::Toggle => {
                    if c.is_uppercase() {
                        c.to_lowercase().next().unwrap_or(*c)
                    } else {
                        c.to_uppercase().next().unwrap_or(*c)
                    }
                }
            };
        }
    } else if let Some(first) = chars.first_mut() {
        *first = match op {
            CaseOp::Upper => first.to_uppercase().next().unwrap_or(*first),
            CaseOp::Lower => first.to_lowercase().next().unwrap_or(*first),
            CaseOp::Toggle => {
                if first.is_uppercase() {
                    first.to_lowercase().next().unwrap_or(*first)
                } else {
                    first.to_uppercase().next().unwrap_or(*first)
                }
            }
        };
    }
    
    chars.into_iter().collect()
}

fn get_subscript_key(expr: &colony_parser::Expression, _context: &ExpansionContext) -> String {
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
