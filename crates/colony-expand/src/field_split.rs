//! Field splitting
//!
//! POSIX rules:
//! - Split on IFS characters (default: space, tab, newline)
//! - Only split unquoted portions
//! - Leading/trailing IFS whitespace is removed
//! - Multiple consecutive IFS whitespace chars count as one separator
//! - Non-whitespace IFS chars are delimiters even if consecutive

use crate::{context::ExpansionContext, Field, IntermediateWord};

/// Split an intermediate word into fields based on IFS
///
/// POSIX field splitting rules:
/// 1. Fields are split on IFS characters, but only in unquoted portions
/// 2. IFS whitespace (space, tab, newline) at start/end is stripped
/// 3. Consecutive IFS whitespace is treated as single separator
/// 4. Non-whitespace IFS characters always create field boundaries
pub fn split_fields(word: &IntermediateWord, context: &ExpansionContext) -> Vec<Field> {
    let ifs = context.ifs();
    let ifs_whitespace: Vec<char> = ifs
        .chars()
        .filter(|c| c.is_whitespace())
        .collect();
    let ifs_non_whitespace: Vec<char> = ifs
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    let mut fields = Vec::new();
    let mut current = String::new();
    let mut current_should_glob = true;

    for segment in word.segments() {
        if !segment.should_field_split() {
            current.push_str(&segment.text);
            if !segment.should_glob() {
                current_should_glob = false;
            }
        } else {
            let parts = split_on_ifs(
                &segment.text,
                &ifs_whitespace,
                &ifs_non_whitespace,
            );

            for (i, part) in parts.into_iter().enumerate() {
                if i > 0 {
                    fields.push(Field::new(current, !current_should_glob));
                    current = String::new();
                    current_should_glob = segment.should_glob();
                }
                current.push_str(&part);
            }
        }
    }

    if !current.is_empty() || word.segments().iter().any(|s| s.quoted()) {
        fields.push(Field::new(current, !current_should_glob));
    }

    if fields.is_empty() && word.segments().iter().any(|s| s.quoted()) {
        fields.push(Field::quoted(String::new()));
    }

    fields
}

/// Split a string on IFS characters
fn split_on_ifs(
    text: &str,
    ifs_whitespace: &[char],
    ifs_non_whitespace: &[char],
) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    let mut ended_with_non_ws_ifs = false;

    while let Some(&c) = chars.peek() {
        if ifs_whitespace.contains(&c) {
            chars.next();
        } else {
            break;
        }
    }

    while let Some(c) = chars.next() {
        if ifs_non_whitespace.contains(&c) {
            fields.push(current);
            current = String::new();
            ended_with_non_ws_ifs = true;
        } else if ifs_whitespace.contains(&c) {
            if !current.is_empty() {
                fields.push(current);
                current = String::new();
            }
            ended_with_non_ws_ifs = false;
            while let Some(&next) = chars.peek() {
                if ifs_whitespace.contains(&next) {
                    chars.next();
                } else {
                    break;
                }
            }
        } else {
            current.push(c);
            ended_with_non_ws_ifs = false;
        }
    }

    if !current.is_empty() || ended_with_non_ws_ifs {
        fields.push(current);
    }

    if fields.is_empty() {
        vec![String::new()]
    } else {
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_on_spaces() {
        let result = split_on_ifs("one two three", &[' ', '\t', '\n'], &[]);
        assert_eq!(result, vec!["one", "two", "three"]);
    }

    #[test]
    fn test_leading_trailing_whitespace() {
        let result = split_on_ifs("  one two  ", &[' ', '\t', '\n'], &[]);
        assert_eq!(result, vec!["one", "two"]);
    }

    #[test]
    fn test_consecutive_whitespace() {
        let result = split_on_ifs("one    two", &[' ', '\t', '\n'], &[]);
        assert_eq!(result, vec!["one", "two"]);
    }

    #[test]
    fn test_non_whitespace_ifs() {
        let result = split_on_ifs("one:two::three", &[], &[':']);
        // Note: consecutive : creates empty fields
        assert_eq!(result, vec!["one", "two", "", "three"]);
    }
}
