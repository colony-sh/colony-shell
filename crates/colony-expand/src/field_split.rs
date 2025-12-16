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
    let mut is_quoted_field = false;

    for segment in word.segments() {
        if segment.quoted {
            // Quoted segments don't split
            current.push_str(&segment.text);
            is_quoted_field = true;
        } else {
            // Unquoted segments split on IFS
            let parts = split_on_ifs(
                &segment.text,
                &ifs_whitespace,
                &ifs_non_whitespace,
            );

            for (i, part) in parts.into_iter().enumerate() {
                if i > 0 && !current.is_empty() {
                    // New field boundary
                    fields.push(Field::new(current, is_quoted_field));
                    current = String::new();
                    is_quoted_field = false;
                }
                current.push_str(&part);
            }
        }
    }

    // Push final field if non-empty
    if !current.is_empty() {
        fields.push(Field::new(current, is_quoted_field));
    }

    // If no fields generated, return empty string as single field
    if fields.is_empty() {
        fields.push(Field::new(String::new(), is_quoted_field));
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

    // Skip leading IFS whitespace
    while let Some(&c) = chars.peek() {
        if ifs_whitespace.contains(&c) {
            chars.next();
        } else {
            break;
        }
    }

    while let Some(c) = chars.next() {
        if ifs_non_whitespace.contains(&c) {
            // Non-whitespace IFS: always creates field boundary (even empty fields)
            fields.push(current);
            current = String::new();
        } else if ifs_whitespace.contains(&c) {
            // Whitespace IFS: skip consecutive whitespace
            if !current.is_empty() {
                fields.push(current);
                current = String::new();
            }
            // Skip additional whitespace
            while let Some(&next) = chars.peek() {
                if ifs_whitespace.contains(&next) {
                    chars.next();
                } else {
                    break;
                }
            }
        } else {
            // Regular character
            current.push(c);
        }
    }

    // Push final field
    if !current.is_empty() {
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
