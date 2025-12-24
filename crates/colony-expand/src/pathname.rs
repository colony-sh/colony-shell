//! Pathname expansion (globbing)
//!
//! POSIX glob patterns:
//! - * matches any string
//! - ? matches any single character
//! - [...] matches any character in brackets
//! - [!...] or [^...] matches any character NOT in brackets
//!
//! Extended glob patterns (extglob):
//! - ?(pattern-list) - matches zero or one occurrence
//! - *(pattern-list) - matches zero or more occurrences
//! - +(pattern-list) - matches one or more occurrences
//! - @(pattern-list) - matches exactly one of the patterns
//! - !(pattern-list) - matches anything except the patterns

use crate::Result;
use regex::Regex;
use fancy_regex::Regex as FancyRegex;
use std::path::Path;

fn normalize_pattern(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len());
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    
    while i < chars.len() {
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == '^' {
            result.push('[');
            result.push('!');
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    
    result
}

pub fn contains_extglob(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    for i in 0..chars.len() {
        if i + 1 < chars.len() && chars[i + 1] == '(' {
            match chars[i] {
                '?' | '*' | '+' | '@' | '!' => return true,
                _ => {}
            }
        }
    }
    false
}

fn contains_glob_chars(s: &str) -> bool {
    for c in s.chars() {
        match c {
            '*' | '?' | '[' | ']' => return true,
            _ => {}
        }
    }
    contains_extglob(s)
}

fn glob_to_regex_part(glob_part: &str) -> String {
    let mut regex = String::new();
    let chars: Vec<char> = glob_part.chars().collect();
    let mut i = 0;
    
    while i < chars.len() {
        match chars[i] {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '[' => {
                regex.push('[');
                i += 1;
                if i < chars.len() && (chars[i] == '!' || chars[i] == '^') {
                    regex.push('^');
                    i += 1;
                }
                while i < chars.len() && chars[i] != ']' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        regex.push('\\');
                        i += 1;
                        regex.push(chars[i]);
                    } else {
                        let c = chars[i];
                        if c == '-' || c == '^' || c == ']' || c == '\\' {
                            regex.push('\\');
                        }
                        regex.push(c);
                    }
                    i += 1;
                }
                regex.push(']');
            }
            '.' | '+' | '^' | '$' | '(' | ')' | '{' | '}' | '|' | '\\' => {
                regex.push('\\');
                regex.push(chars[i]);
            }
            _ => regex.push(chars[i]),
        }
        i += 1;
    }
    
    regex
}

fn find_matching_paren(s: &str, start: usize) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut depth = 0;
    let mut i = start;
    
    while i < chars.len() {
        match chars[i] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            '\\' if i + 1 < chars.len() => i += 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// Converts extglob pattern to regex.
/// For `!(foo).txt` produces `^(?!foo\.txt$).*\.txt$` (match anything ending in .txt except foo.txt)
fn extglob_to_regex(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    
    if chars.len() >= 2 && chars[0] == '!' && chars[1] == '(' {
        if let Some(paren_end) = find_matching_paren(pattern, 1) {
            let inner_pattern = &pattern[2..paren_end];
            let suffix = &pattern[paren_end + 1..];
            
            let alternatives: Vec<&str> = split_pattern_list(inner_pattern);
            
            let excluded: Vec<String> = alternatives
                .iter()
                .map(|alt| {
                    let full_excluded = format!("{}{}", alt, suffix);
                    extglob_to_regex_inner(&full_excluded)
                })
                .collect();
            
            let suffix_regex = glob_to_regex_part(suffix);
            
            let mut regex = String::from("^");
            for excl in &excluded {
                regex.push_str(&format!("(?!{}$)", excl));
            }
            
            if suffix.is_empty() {
                regex.push_str(".+");
            } else {
                regex.push_str(".*");
                regex.push_str(&suffix_regex);
            }
            regex.push('$');
            
            return regex;
        }
    }
    
    let mut regex = String::from("^");
    let mut i = 0;
    
    while i < chars.len() {
        let c = chars[i];
        
        if i + 1 < chars.len() && chars[i + 1] == '(' {
            match c {
                '?' | '*' | '+' | '@' | '!' => {
                    let paren_start = i + 1;
                    if let Some(paren_end) = find_matching_paren(pattern, paren_start) {
                        let inner: String = chars[paren_start + 1..paren_end].iter().collect();
                        let alternatives: Vec<&str> = split_pattern_list(&inner);
                        
                        let alt_regexes: Vec<String> = alternatives
                            .iter()
                            .map(|alt| extglob_to_regex_inner(alt))
                            .collect();
                        let alt_group = format!("(?:{})", alt_regexes.join("|"));
                        
                        match c {
                            '?' => regex.push_str(&format!("{}?", alt_group)),
                            '*' => regex.push_str(&format!("{}*", alt_group)),
                            '+' => regex.push_str(&format!("{}+", alt_group)),
                            '@' => regex.push_str(&alt_group),
                            '!' => {
                                regex.push_str(&format!("(?!{})", alt_group));
                                regex.push_str(".*");
                            }
                            _ => {}
                        }
                        
                        i = paren_end + 1;
                        continue;
                    }
                }
                _ => {}
            }
        }
        
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '[' => {
                regex.push('[');
                i += 1;
                if i < chars.len() && (chars[i] == '!' || chars[i] == '^') {
                    regex.push('^');
                    i += 1;
                }
                while i < chars.len() && chars[i] != ']' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        regex.push('\\');
                        i += 1;
                        regex.push(chars[i]);
                    } else {
                        let c = chars[i];
                        if c == '-' && i > 0 && i + 1 < chars.len() && chars[i + 1] != ']' {
                            regex.push(c);
                        } else if c == '^' || c == '\\' {
                            regex.push('\\');
                            regex.push(c);
                        } else {
                            regex.push(c);
                        }
                    }
                    i += 1;
                }
                regex.push(']');
            }
            '.' | '+' | '^' | '$' | '(' | ')' | '{' | '}' | '|' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
        i += 1;
    }
    
    regex.push('$');
    regex
}

fn extglob_to_regex_inner(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut regex = String::new();
    let mut i = 0;
    
    while i < chars.len() {
        let c = chars[i];
        
        if i + 1 < chars.len() && chars[i + 1] == '(' {
            match c {
                '?' | '*' | '+' | '@' | '!' => {
                    let paren_start = i + 1;
                    if let Some(paren_end) = find_matching_paren(pattern, paren_start) {
                        let inner: String = chars[paren_start + 1..paren_end].iter().collect();
                        let alternatives: Vec<&str> = split_pattern_list(&inner);
                        
                        let alt_regexes: Vec<String> = alternatives
                            .iter()
                            .map(|alt| extglob_to_regex_inner(alt))
                            .collect();
                        let alt_group = format!("(?:{})", alt_regexes.join("|"));
                        
                        match c {
                            '?' => regex.push_str(&format!("{}?", alt_group)),
                            '*' => regex.push_str(&format!("{}*", alt_group)),
                            '+' => regex.push_str(&format!("{}+", alt_group)),
                            '@' => regex.push_str(&alt_group),
                            '!' => {
                                regex.push_str(&format!("(?!{})", alt_group));
                            }
                            _ => {}
                        }
                        
                        i = paren_end + 1;
                        continue;
                    }
                }
                _ => {}
            }
        }
        
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '[' => {
                regex.push('[');
                i += 1;
                if i < chars.len() && (chars[i] == '!' || chars[i] == '^') {
                    regex.push('^');
                    i += 1;
                }
                while i < chars.len() && chars[i] != ']' {
                    regex.push(chars[i]);
                    i += 1;
                }
                regex.push(']');
            }
            '.' | '+' | '^' | '$' | '(' | ')' | '{' | '}' | '|' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
        i += 1;
    }
    
    regex
}

fn split_pattern_list(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let chars: Vec<char> = s.chars().collect();
    
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '|' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

pub fn expand_pathname(pattern: &str) -> Result<Vec<String>> {
    if !contains_glob_chars(pattern) {
        return Ok(vec![pattern.to_string()]);
    }

    if contains_extglob(pattern) {
        return expand_extglob_pathname(pattern);
    }

    let normalized = normalize_pattern(pattern);

    match glob::glob(&normalized) {
        Ok(paths) => {
            let mut results: Vec<String> = paths
                .filter_map(|entry| entry.ok())
                .filter(|path| {
                    let filename = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");

                    !filename.starts_with('.') || pattern.contains("/.")
                })
                .map(|path| path.display().to_string())
                .collect();

            if results.is_empty() {
                Ok(vec![pattern.to_string()])
            } else {
                results.sort();
                Ok(results)
            }
        }
        Err(_) => {
            Ok(vec![pattern.to_string()])
        }
    }
}

fn expand_extglob_pathname(pattern: &str) -> Result<Vec<String>> {
    let (dir_part, file_pattern) = if let Some(slash_pos) = pattern.rfind('/') {
        (&pattern[..=slash_pos], &pattern[slash_pos + 1..])
    } else {
        ("", pattern)
    };
    
    let dir = if dir_part.is_empty() { "." } else { dir_part.trim_end_matches('/') };
    
    if contains_extglob(dir_part) {
        let dir_expanded = expand_extglob_dir(dir_part)?;
        let mut all_results = Vec::new();
        
        for expanded_dir in dir_expanded {
            let full_pattern = format!("{}/{}", expanded_dir.trim_end_matches('/'), file_pattern);
            let results = expand_extglob_pathname(&full_pattern)?;
            all_results.extend(results);
        }
        
        if all_results.is_empty() {
            return Ok(vec![pattern.to_string()]);
        }
        all_results.sort();
        all_results.dedup();
        return Ok(all_results);
    }
    
    let regex_pattern = extglob_to_regex(file_pattern);
    let has_negation = file_pattern.starts_with("!(");
    
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(vec![pattern.to_string()]),
    };
    
    let results: Vec<String> = if has_negation {
        let regex = match FancyRegex::new(&regex_pattern) {
            Ok(r) => r,
            Err(_) => return Ok(vec![pattern.to_string()]),
        };
        
        read_dir
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let file_name = entry.file_name();
                let name = file_name.to_str()?;
                
                if name.starts_with('.') && !file_pattern.starts_with('.') {
                    return None;
                }
                
                if regex.is_match(name).unwrap_or(false) {
                    let path = if dir_part.is_empty() {
                        name.to_string()
                    } else {
                        format!("{}{}", dir_part, name)
                    };
                    Some(path)
                } else {
                    None
                }
            })
            .collect()
    } else {
        let regex = match Regex::new(&regex_pattern) {
            Ok(r) => r,
            Err(_) => return Ok(vec![pattern.to_string()]),
        };
        
        read_dir
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let file_name = entry.file_name();
                let name = file_name.to_str()?;
                
                if name.starts_with('.') && !file_pattern.starts_with('.') {
                    return None;
                }
                
                if regex.is_match(name) {
                    let path = if dir_part.is_empty() {
                        name.to_string()
                    } else {
                        format!("{}{}", dir_part, name)
                    };
                    Some(path)
                } else {
                    None
                }
            })
            .collect()
    };
    
    if results.is_empty() {
        Ok(vec![pattern.to_string()])
    } else {
        let mut sorted = results;
        sorted.sort();
        Ok(sorted)
    }
}

fn expand_extglob_dir(dir_pattern: &str) -> Result<Vec<String>> {
    let parts: Vec<&str> = dir_pattern.split('/').collect();
    let mut current_paths = vec![String::new()];
    
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            if i == 0 {
                current_paths = vec!["/".to_string()];
            }
            continue;
        }
        
        let mut new_paths = Vec::new();
        
        for base in &current_paths {
            let search_dir = if base.is_empty() { "." } else { base.as_str() };
            
            if !contains_glob_chars(part) {
                let new_path = if base.is_empty() || base == "/" {
                    format!("{}{}", base, part)
                } else {
                    format!("{}/{}", base, part)
                };
                if Path::new(&new_path).exists() {
                    new_paths.push(new_path);
                }
            } else {
                let regex_pattern = extglob_to_regex(part);
                if let Ok(regex) = Regex::new(&regex_pattern) {
                    if let Ok(read_dir) = std::fs::read_dir(search_dir) {
                        for entry in read_dir.filter_map(|e| e.ok()) {
                            if let Some(name) = entry.file_name().to_str() {
                                if !name.starts_with('.') || part.starts_with('.') {
                                    if regex.is_match(name) && entry.path().is_dir() {
                                        let new_path = if base.is_empty() {
                                            name.to_string()
                                        } else if base == "/" {
                                            format!("/{}", name)
                                        } else {
                                            format!("{}/{}", base, name)
                                        };
                                        new_paths.push(new_path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if new_paths.is_empty() {
            return Ok(vec![dir_pattern.to_string()]);
        }
        
        current_paths = new_paths;
    }
    
    Ok(current_paths)
}

pub fn match_extglob_pattern(pattern: &str, text: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    
    if chars.len() >= 2 && chars[0] == '!' && chars[1] == '(' {
        if let Some(paren_end) = find_matching_paren(pattern, 1) {
            if paren_end == pattern.len() - 1 {
                let inner_pattern = &pattern[2..paren_end];
                let alternatives: Vec<&str> = split_pattern_list(inner_pattern);
                for alt in alternatives {
                    let positive_pattern = if alt.contains('(') && contains_extglob(alt) {
                        alt.to_string()
                    } else {
                        format!("@({})", alt)
                    };
                    
                    if match_extglob_pattern(&positive_pattern, text) {
                        return false;
                    }
                    
                    let simple_regex = format!("^{}$", glob_to_regex_part(alt));
                    if let Ok(re) = Regex::new(&simple_regex) {
                        if re.is_match(text) {
                            return false;
                        }
                    }
                }
                return true;
            }
        }
    }
    
    let regex_pattern = extglob_to_regex(pattern);
    match Regex::new(&regex_pattern) {
        Ok(regex) => regex.is_match(text),
        Err(_) => pattern == text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_glob_returns_as_is() {
        let result = expand_pathname("hello.txt").unwrap();
        assert_eq!(result, vec!["hello.txt"]);
    }

    #[test]
    fn test_detects_glob_chars() {
        assert!(contains_glob_chars("*.txt"));
        assert!(contains_glob_chars("file?.txt"));
        assert!(contains_glob_chars("[abc].txt"));
        assert!(!contains_glob_chars("plain.txt"));
    }

    #[test]
    fn test_glob_no_match_returns_pattern() {
        let result = expand_pathname("*.nonexistent_xyz").unwrap();
        assert_eq!(result, vec!["*.nonexistent_xyz"]);
    }
    
    #[test]
    fn test_normalize_caret_negation() {
        assert_eq!(normalize_pattern("file[^abc].txt"), "file[!abc].txt");
        assert_eq!(normalize_pattern("[^0-9]"), "[!0-9]");
        assert_eq!(normalize_pattern("[abc]"), "[abc]");
        assert_eq!(normalize_pattern("plain.txt"), "plain.txt");
    }
    
    #[test]
    fn test_detects_extglob() {
        assert!(contains_extglob("@(foo|bar)"));
        assert!(contains_extglob("?(abc)"));
        assert!(contains_extglob("*(xyz)"));
        assert!(contains_extglob("+(def)"));
        assert!(contains_extglob("!(ghi)"));
        assert!(!contains_extglob("foo"));
        assert!(!contains_extglob("foo(bar)"));
    }
    
    #[test]
    fn test_extglob_to_regex_at() {
        let regex = extglob_to_regex("@(foo|bar)");
        let re = Regex::new(&regex).unwrap();
        assert!(re.is_match("foo"));
        assert!(re.is_match("bar"));
        assert!(!re.is_match("baz"));
        assert!(!re.is_match("foobar"));
    }
    
    #[test]
    fn test_extglob_to_regex_question() {
        let regex = extglob_to_regex("foo?(bar)");
        let re = Regex::new(&regex).unwrap();
        assert!(re.is_match("foo"));
        assert!(re.is_match("foobar"));
        assert!(!re.is_match("foobarbar"));
    }
    
    #[test]
    fn test_extglob_to_regex_star() {
        let regex = extglob_to_regex("foo*(bar)");
        let re = Regex::new(&regex).unwrap();
        assert!(re.is_match("foo"));
        assert!(re.is_match("foobar"));
        assert!(re.is_match("foobarbar"));
    }
    
    #[test]
    fn test_extglob_to_regex_plus() {
        let regex = extglob_to_regex("foo+(bar)");
        let re = Regex::new(&regex).unwrap();
        assert!(!re.is_match("foo"));
        assert!(re.is_match("foobar"));
        assert!(re.is_match("foobarbar"));
    }
    
    #[test]
    fn test_match_extglob_pattern() {
        assert!(match_extglob_pattern("@(foo|bar)", "foo"));
        assert!(match_extglob_pattern("@(foo|bar)", "bar"));
        assert!(!match_extglob_pattern("@(foo|bar)", "baz"));
        
        assert!(match_extglob_pattern("foo?(bar)", "foo"));
        assert!(match_extglob_pattern("foo?(bar)", "foobar"));
        
        assert!(match_extglob_pattern("!(foo)", "bar"));
        assert!(!match_extglob_pattern("!(foo)", "foo"));
    }
    
    #[test]
    fn test_contains_extglob_negation() {
        assert!(contains_extglob("!(foo).txt"));
        assert!(contains_extglob("!(foo|bar).txt"));
        assert!(contains_extglob("*.!(txt|log)"));
    }
    
    #[test]
    fn test_extglob_negation_with_suffix() {
        let regex = extglob_to_regex("!(foo).txt");
        println!("Generated regex for !(foo).txt: {}", regex);
        let re = FancyRegex::new(&regex).unwrap();
        
        assert!(re.is_match("bar.txt").unwrap(), "bar.txt should match !(foo).txt");
        assert!(re.is_match("baz.txt").unwrap(), "baz.txt should match !(foo).txt");
        assert!(!re.is_match("foo.txt").unwrap(), "foo.txt should NOT match !(foo).txt");
        assert!(!re.is_match("bar.md").unwrap(), "bar.md should NOT match !(foo).txt");
    }
    
    #[test]
    fn test_extglob_negation_multiple() {
        let regex = extglob_to_regex("!(foo|bar).txt");
        println!("Generated regex for !(foo|bar).txt: {}", regex);
        let re = FancyRegex::new(&regex).unwrap();
        
        assert!(re.is_match("baz.txt").unwrap(), "baz.txt should match !(foo|bar).txt");
        assert!(!re.is_match("foo.txt").unwrap(), "foo.txt should NOT match !(foo|bar).txt");
        assert!(!re.is_match("bar.txt").unwrap(), "bar.txt should NOT match !(foo|bar).txt");
    }
}
