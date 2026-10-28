//! Pathname expansion (globbing)
//!
//! POSIX glob patterns:
//! - `*` matches any string
//! - `?` matches any single character
//! - `[...]` matches any character in brackets
//! - `[!...]` or `[^...]` matches any character NOT in brackets
//!
//! Extended glob patterns (extglob):
//! - ?(pattern-list) - matches zero or one occurrence
//! - *(pattern-list) - matches zero or more occurrences
//! - +(pattern-list) - matches one or more occurrences
//! - @(pattern-list) - matches exactly one of the patterns
//! - !(pattern-list) - matches anything except the patterns

use crate::Result;
use fancy_regex::Regex as FancyRegex;
use regex::Regex;
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

fn posix_class_to_regex(class_name: &str) -> Option<&'static str> {
    match class_name {
        "alnum" => Some("a-zA-Z0-9"),
        "alpha" => Some("a-zA-Z"),
        "blank" => Some(" \\t"),
        "cntrl" => Some("\\x00-\\x1f\\x7f"),
        "digit" => Some("0-9"),
        "graph" => Some("!-~"),
        "lower" => Some("a-z"),
        "print" => Some(" -~"),
        "punct" => Some("!-/:-@\\[-`{-~"),
        "space" => Some(" \\t\\n\\r\\x0b\\x0c"),
        "upper" => Some("A-Z"),
        "xdigit" => Some("0-9A-Fa-f"),
        "word" => Some("a-zA-Z0-9_"),
        "ascii" => Some("\\x00-\\x7f"),
        _ => None,
    }
}

fn posix_class_to_glob(class_name: &str) -> Option<&'static str> {
    match class_name {
        "alnum" => Some("a-zA-Z0-9"),
        "alpha" => Some("a-zA-Z"),
        "digit" => Some("0-9"),
        "lower" => Some("a-z"),
        "upper" => Some("A-Z"),
        "xdigit" => Some("0-9A-Fa-f"),
        _ => None,
    }
}

fn expand_posix_classes(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len());
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '[' {
            let bracket_start = i;
            let mut bracket_content = String::new();
            i += 1;

            let negated = i < chars.len() && (chars[i] == '!' || chars[i] == '^');
            if negated {
                i += 1;
            }

            while i < chars.len() && chars[i] != ']' {
                if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == ':' {
                    let class_start = i + 2;
                    i += 2;
                    while i + 1 < chars.len() && !(chars[i] == ':' && chars[i + 1] == ']') {
                        i += 1;
                    }
                    if i + 1 < chars.len() && chars[i] == ':' && chars[i + 1] == ']' {
                        let class_name: String = chars[class_start..i].iter().collect();
                        if let Some(expanded) = posix_class_to_glob(&class_name) {
                            bracket_content.push_str(expanded);
                        } else {
                            bracket_content.push_str(&format!("[:{}:]", class_name));
                        }
                        i += 2;
                    } else {
                        bracket_content
                            .push_str(&chars[class_start - 2..=i].iter().collect::<String>());
                        i += 1;
                    }
                } else {
                    bracket_content.push(chars[i]);
                    i += 1;
                }
            }

            if i < chars.len() && chars[i] == ']' {
                result.push('[');
                if negated {
                    result.push('!');
                }
                result.push_str(&bracket_content);
                result.push(']');
                i += 1;
            } else {
                for c in &chars[bracket_start..=i.min(chars.len() - 1)] {
                    result.push(*c);
                }
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
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
                    } else if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == ':' {
                        // POSIX character class [:digit:], [:alpha:], etc.
                        let start = i;
                        i += 2;
                        let class_start = i;
                        while i + 1 < chars.len() && !(chars[i] == ':' && chars[i + 1] == ']') {
                            i += 1;
                        }
                        if i + 1 < chars.len() && chars[i] == ':' && chars[i + 1] == ']' {
                            let class_name: String = chars[class_start..i].iter().collect();
                            if let Some(equivalent) = posix_class_to_regex(&class_name) {
                                regex.push_str(equivalent);
                            } else {
                                for c in chars.iter().take(i + 2).skip(start) {
                                    regex.push(*c);
                                }
                            }
                            i += 1;
                        } else {
                            for c in chars.iter().take(i + 1).skip(start) {
                                regex.push(*c);
                            }
                        }
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
                    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

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
        Err(_) => Ok(vec![pattern.to_string()]),
    }
}

fn expand_extglob_pathname(pattern: &str) -> Result<Vec<String>> {
    let (dir_part, file_pattern) = if let Some(slash_pos) = pattern.rfind('/') {
        (&pattern[..=slash_pos], &pattern[slash_pos + 1..])
    } else {
        ("", pattern)
    };

    let dir = if dir_part.is_empty() {
        "."
    } else {
        dir_part.trim_end_matches('/')
    };

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
    let has_negation = file_pattern.contains("!(");

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
                                if (!name.starts_with('.') || part.starts_with('.'))
                                    && regex.is_match(name)
                                    && entry.path().is_dir()
                                {
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

use crate::ShoptOptions;
use walkdir::WalkDir;

fn contains_globstar(pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('/').collect();
    for part in parts {
        if part == "**" {
            return true;
        }
    }
    false
}

pub fn expand_pathname_with_options(pattern: &str, options: &ShoptOptions) -> Result<Vec<String>> {
    if !contains_glob_chars(pattern) {
        return Ok(vec![pattern.to_string()]);
    }

    if options.globstar && contains_globstar(pattern) {
        return expand_globstar_pathname(pattern, options);
    }

    if contains_extglob(pattern) {
        return expand_extglob_pathname(pattern);
    }

    let mut normalized = normalize_pattern(pattern);
    normalized = expand_posix_classes(&normalized);

    if !options.globstar && contains_globstar(&normalized) {
        normalized = normalized.replace("**", "*");
    }

    match glob::glob(&normalized) {
        Ok(paths) => {
            let mut results: Vec<String> = paths
                .filter_map(|entry| entry.ok())
                .filter(|path| {
                    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                    options.dotglob || !filename.starts_with('.') || pattern.contains("/.")
                })
                .map(|path| path.display().to_string())
                .collect();

            if results.is_empty() {
                if options.nullglob {
                    Ok(vec![])
                } else {
                    Ok(vec![pattern.to_string()])
                }
            } else {
                results.sort();
                Ok(results)
            }
        }
        Err(_) => {
            if options.nullglob {
                Ok(vec![])
            } else {
                Ok(vec![pattern.to_string()])
            }
        }
    }
}

fn expand_globstar_pathname(pattern: &str, options: &ShoptOptions) -> Result<Vec<String>> {
    let parts: Vec<&str> = pattern.split('/').collect();

    let globstar_idx = parts.iter().position(|&p| p == "**");
    let globstar_idx = match globstar_idx {
        Some(idx) => idx,
        None => return expand_pathname(pattern),
    };

    let prefix_parts = &parts[..globstar_idx];
    let suffix_parts = &parts[globstar_idx + 1..];

    let base_dir = if prefix_parts.is_empty() {
        ".".to_string()
    } else {
        prefix_parts.join("/")
    };

    let suffix_pattern = if suffix_parts.is_empty() {
        None
    } else {
        Some(suffix_parts.join("/"))
    };

    let dirs_only = pattern.ends_with("**/");

    let mut results = Vec::new();

    if !std::path::Path::new(&base_dir).exists() {
        if options.nullglob {
            return Ok(vec![]);
        } else {
            return Ok(vec![pattern.to_string()]);
        }
    }

    for entry in WalkDir::new(&base_dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            options.dotglob || !name.starts_with('.') || e.depth() == 0
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        let path_str = path.display().to_string();

        let relative_path = if base_dir == "." {
            path_str.clone()
        } else if path_str.starts_with(&base_dir) {
            let stripped = path_str.strip_prefix(&base_dir).unwrap_or(&path_str);
            let stripped = stripped.strip_prefix('/').unwrap_or(stripped);
            if stripped.is_empty() {
                ".".to_string()
            } else {
                stripped.to_string()
            }
        } else {
            path_str.clone()
        };

        if dirs_only && !path.is_dir() {
            continue;
        }

        match &suffix_pattern {
            None => {
                let result = if prefix_parts.is_empty() {
                    if relative_path == "." {
                        continue;
                    }
                    relative_path
                } else {
                    path_str
                };

                if dirs_only {
                    results.push(format!("{}/", result.trim_end_matches('/')));
                } else {
                    results.push(result);
                }
            }
            Some(suffix) => {
                if path.is_dir() {
                    let full_pattern = format!("{}/{}", path_str, suffix);
                    let sub_results = expand_pathname_with_options(&full_pattern, options)?;
                    for r in sub_results {
                        if r != full_pattern || std::path::Path::new(&r).exists() {
                            results.push(r);
                        }
                    }
                }
            }
        }
    }

    if results.is_empty() {
        if options.nullglob {
            Ok(vec![])
        } else {
            Ok(vec![pattern.to_string()])
        }
    } else {
        results.sort();
        results.dedup();
        Ok(results)
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

        assert!(
            re.is_match("bar.txt").unwrap(),
            "bar.txt should match !(foo).txt"
        );
        assert!(
            re.is_match("baz.txt").unwrap(),
            "baz.txt should match !(foo).txt"
        );
        assert!(
            !re.is_match("foo.txt").unwrap(),
            "foo.txt should NOT match !(foo).txt"
        );
        assert!(
            !re.is_match("bar.md").unwrap(),
            "bar.md should NOT match !(foo).txt"
        );
    }

    #[test]
    fn test_extglob_negation_multiple() {
        let regex = extglob_to_regex("!(foo|bar).txt");
        println!("Generated regex for !(foo|bar).txt: {}", regex);
        let re = FancyRegex::new(&regex).unwrap();

        assert!(
            re.is_match("baz.txt").unwrap(),
            "baz.txt should match !(foo|bar).txt"
        );
        assert!(
            !re.is_match("foo.txt").unwrap(),
            "foo.txt should NOT match !(foo|bar).txt"
        );
        assert!(
            !re.is_match("bar.txt").unwrap(),
            "bar.txt should NOT match !(foo|bar).txt"
        );
    }
}
