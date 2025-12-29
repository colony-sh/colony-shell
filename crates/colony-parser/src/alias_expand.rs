//! Alias expansion preprocessing for parse-time alias substitution
//!
//! Per POSIX 2.3.1, alias expansion occurs during tokenization, after a token
//! is recognized but before parsing. This module implements source-level
//! alias substitution that runs before the pest parser.

use std::collections::{HashMap, HashSet};

pub struct AliasContext {
    aliases: HashMap<String, String>,
    expanding: HashSet<String>,
}

impl AliasContext {
    pub fn new() -> Self {
        Self {
            aliases: HashMap::new(),
            expanding: HashSet::new(),
        }
    }

    pub fn from_map(aliases: HashMap<String, String>) -> Self {
        Self {
            aliases,
            expanding: HashSet::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.aliases.get(name).map(|s| s.as_str())
    }

    pub fn is_expanding(&self, name: &str) -> bool {
        self.expanding.contains(name)
    }

    pub fn start_expanding(&mut self, name: &str) {
        self.expanding.insert(name.to_string());
    }

    pub fn stop_expanding(&mut self, name: &str) {
        self.expanding.remove(name);
    }
}

impl Default for AliasContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Preprocess source code to expand aliases at command positions
///
/// Returns the expanded source code where aliases in command position
/// have been substituted with their values.
pub fn preprocess_aliases(source: &str, ctx: &mut AliasContext) -> String {
    if ctx.aliases.is_empty() {
        return source.to_string();
    }

    let mut result = String::with_capacity(source.len() * 2);
    let mut remaining = source;

    while !remaining.is_empty() {
        let (before, cmd_start, after) = find_command_position(remaining);
        result.push_str(before);

        if cmd_start.is_empty() && after.is_empty() {
            break;
        }

        if !cmd_start.is_empty() {
            let expanded = expand_at_command_position(cmd_start, after, ctx);
            result.push_str(&expanded.0);
            remaining = expanded.1;
        } else {
            remaining = after;
        }
    }

    result
}

fn find_command_position(source: &str) -> (&str, &str, &str) {
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < bytes.len() {
        let c = bytes[i];

        if c == b'\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            i += 1;
            continue;
        }

        if c == b'"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            i += 1;
            continue;
        }

        if in_single_quote || in_double_quote {
            i += 1;
            continue;
        }

        if is_command_starter(bytes, i) {
            let start = skip_whitespace(&source[i..]);
            let word_start = i + (source[i..].len() - start.len());
            return (&source[..word_start], start, "");
        }

        i += 1;
    }

    (source, "", "")
}

fn is_command_starter(bytes: &[u8], pos: usize) -> bool {
    if pos == 0 {
        return !bytes[0].is_ascii_whitespace();
    }

    let prev = bytes[pos - 1];
    let curr = bytes[pos];

    if curr.is_ascii_whitespace() {
        return false;
    }

    matches!(prev, b';' | b'|' | b'&' | b'\n' | b'(' | b'{' | b'`')
        || (pos >= 2 && bytes[pos - 2] == b'&' && prev == b'&')
        || (pos >= 2 && bytes[pos - 2] == b'|' && prev == b'|')
}

fn skip_whitespace(s: &str) -> &str {
    s.trim_start_matches(|c: char| c == ' ' || c == '\t')
}

fn expand_at_command_position<'a>(
    word_and_rest: &'a str,
    _after: &'a str,
    ctx: &mut AliasContext,
) -> (String, &'a str) {
    let (word, rest) = extract_word(word_and_rest);

    if word.is_empty() {
        return (String::new(), rest);
    }

    if ctx.is_expanding(&word) {
        return (word.to_string() + rest, "");
    }

    if let Some(alias_value) = ctx.get(&word).map(|s| s.to_string()) {
        ctx.start_expanding(&word);

        let ends_with_space = alias_value.ends_with(' ') || alias_value.ends_with('\t');

        let mut expanded = alias_value.clone();

        if ends_with_space {
            let next_rest = skip_whitespace(rest);
            let (next_word, next_rest_after) = extract_word(next_rest);
            if !next_word.is_empty() && !ctx.is_expanding(&next_word) {
                if let Some(next_alias) = ctx.get(&next_word).map(|s| s.to_string()) {
                    expanded.push_str(&next_alias);
                    expanded.push_str(next_rest_after);
                    ctx.stop_expanding(&word);
                    return (expanded, "");
                }
            }
        }

        expanded.push_str(rest);
        ctx.stop_expanding(&word);
        (expanded, "")
    } else {
        (word.to_string() + rest, "")
    }
}

fn extract_word(s: &str) -> (&str, &str) {
    let s = skip_whitespace(s);
    if s.is_empty() {
        return ("", "");
    }

    let bytes = s.as_bytes();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < bytes.len() {
        let c = bytes[i];

        if c == b'\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            i += 1;
            continue;
        }

        if c == b'"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            i += 1;
            continue;
        }

        if in_single_quote || in_double_quote {
            i += 1;
            continue;
        }

        if c.is_ascii_whitespace() || is_metachar(c) {
            break;
        }

        i += 1;
    }

    (&s[..i], &s[i..])
}

fn is_metachar(c: u8) -> bool {
    matches!(
        c,
        b';' | b'|' | b'&' | b'<' | b'>' | b'(' | b')' | b'`' | b'\n'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_alias() {
        let mut ctx = AliasContext::from_map(
            [("ll".to_string(), "ls -la".to_string())]
                .into_iter()
                .collect(),
        );
        let result = preprocess_aliases("ll", &mut ctx);
        assert_eq!(result, "ls -la");
    }

    #[test]
    fn test_alias_with_args() {
        let mut ctx = AliasContext::from_map(
            [("ll".to_string(), "ls -la".to_string())]
                .into_iter()
                .collect(),
        );
        let result = preprocess_aliases("ll /tmp", &mut ctx);
        assert_eq!(result, "ls -la /tmp");
    }

    #[test]
    fn test_alias_after_semicolon() {
        let mut ctx = AliasContext::from_map(
            [("e".to_string(), "echo".to_string())]
                .into_iter()
                .collect(),
        );
        let result = preprocess_aliases("true; e hello", &mut ctx);
        assert_eq!(result, "true; echo hello");
    }

    #[test]
    fn test_alias_after_pipe() {
        let mut ctx = AliasContext::from_map(
            [("e".to_string(), "echo".to_string())]
                .into_iter()
                .collect(),
        );
        let result = preprocess_aliases("true | e hello", &mut ctx);
        assert_eq!(result, "true | echo hello");
    }

    #[test]
    fn test_no_alias_in_quotes() {
        let mut ctx = AliasContext::from_map(
            [("e".to_string(), "echo".to_string())]
                .into_iter()
                .collect(),
        );
        let result = preprocess_aliases("echo 'e hello'", &mut ctx);
        assert_eq!(result, "echo 'e hello'");
    }

    #[test]
    fn test_self_referential_alias() {
        let mut ctx =
            AliasContext::from_map([("ls".to_string(), "ls".to_string())].into_iter().collect());
        let result = preprocess_aliases("ls -la", &mut ctx);
        assert_eq!(result, "ls -la");
    }

    #[test]
    fn test_trailing_space_expansion() {
        let mut ctx = AliasContext::from_map(
            [
                ("prefix".to_string(), "echo ".to_string()),
                ("msg".to_string(), "hello".to_string()),
            ]
            .into_iter()
            .collect(),
        );
        let result = preprocess_aliases("prefix msg", &mut ctx);
        assert_eq!(result, "echo hello");
    }

    #[test]
    fn test_alias_with_pipe_in_value() {
        let mut ctx = AliasContext::from_map(
            [("countlines".to_string(), "wc -l".to_string())]
                .into_iter()
                .collect(),
        );
        let result = preprocess_aliases("cat file | countlines", &mut ctx);
        assert_eq!(result, "cat file | wc -l");
    }
}
