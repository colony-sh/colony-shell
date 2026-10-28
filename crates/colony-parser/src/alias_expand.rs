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
/// (start of input, or after `;`, `|`, `&`, `&&`, `||`, newline, `(`, `{`
/// or a backtick) have been substituted with their values.
pub fn preprocess_aliases(source: &str, ctx: &mut AliasContext) -> String {
    if ctx.aliases.is_empty() {
        return source.to_string();
    }

    let mut result = String::with_capacity(source.len() * 2);
    let mut rest = source;

    // Each pass starts at a command position: expand its first word, then
    // copy everything up to and including the next command separator.
    loop {
        let trimmed = skip_whitespace(rest);
        result.push_str(&rest[..rest.len() - trimmed.len()]);

        let (word, after) = extract_word(trimmed);
        let (expanded, after) = expand_command_word(word, after, ctx);
        result.push_str(&expanded);

        let end = next_command_position(after);
        result.push_str(&after[..end]);
        if end == after.len() {
            break;
        }
        rest = &after[end..];
    }

    result
}

/// Byte index just past the next unquoted command separator, or `s.len()`.
fn next_command_position(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for (i, &c) in bytes.iter().enumerate() {
        match c {
            b'\'' if !in_double_quote => in_single_quote = !in_single_quote,
            b'"' if !in_single_quote => in_double_quote = !in_double_quote,
            _ if in_single_quote || in_double_quote => {}
            // `2>&1`, `>|` are redirections, not separators
            b'&' | b'|' if i > 0 && matches!(bytes[i - 1], b'>' | b'<') => {}
            // `${var}` is parameter expansion, not a brace group
            b'{' if i > 0 && bytes[i - 1] == b'$' => {}
            b';' | b'|' | b'&' | b'\n' | b'(' | b'{' | b'`' => return i + 1,
            _ => {}
        }
    }

    bytes.len()
}

/// Expand `word` if it is an alias. Returns the replacement text and the
/// input that remains after it.
fn expand_command_word<'a>(word: &str, rest: &'a str, ctx: &AliasContext) -> (String, &'a str) {
    let Some(mut expanded) = ctx.get(word).map(str::to_string) else {
        return (word.to_string(), rest);
    };

    // POSIX 2.3.1: a value ending in a blank makes the next word alias-eligible
    if expanded.ends_with([' ', '\t']) {
        let (next_word, next_rest) = extract_word(rest);
        if let Some(next_alias) = ctx.get(next_word) {
            expanded.push_str(next_alias);
            return (expanded, next_rest);
        }
    }

    (expanded, rest)
}

fn skip_whitespace(s: &str) -> &str {
    s.trim_start_matches([' ', '\t'])
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

    #[test]
    fn test_alias_after_and_or_and_newline() {
        let mut ctx = AliasContext::from_map(
            [("e".to_string(), "echo".to_string())]
                .into_iter()
                .collect(),
        );
        let result = preprocess_aliases("true && e a || e b\ne c", &mut ctx);
        assert_eq!(result, "true && echo a || echo b\necho c");
    }

    #[test]
    fn test_redirect_and_param_expansion_are_not_command_positions() {
        let mut ctx =
            AliasContext::from_map([("x".to_string(), "BAD".to_string())].into_iter().collect());
        let result = preprocess_aliases("echo ${x} 2>&x >|x", &mut ctx);
        assert_eq!(result, "echo ${x} 2>&x >|x");
    }
}
