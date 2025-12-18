//! Pathname expansion (globbing)
//!
//! POSIX glob patterns:
//! - * matches any string
//! - ? matches any single character
//! - [...] matches any character in brackets
//! - [!...] or [^...] matches any character NOT in brackets

use crate::Result;

/// Normalize glob pattern for the glob crate
/// 
/// The Rust glob crate uses [!...] for negation, but POSIX and bash also support [^...]
/// This function converts [^...] to [!...] for compatibility.
fn normalize_pattern(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len());
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    
    while i < chars.len() {
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == '^' {
            // Convert [^ to [!
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

/// Expand a glob pattern to matching file paths
///
/// If the pattern matches files, returns the list of sorted paths.
/// If the pattern doesn't match anything, returns the pattern itself (POSIX behavior).
pub fn expand_pathname(pattern: &str) -> Result<Vec<String>> {
    // Check if pattern contains glob meta-characters
    if !contains_glob_chars(pattern) {
        // No glob chars - return as-is
        return Ok(vec![pattern.to_string()]);
    }

    // Normalize [^...] to [!...] for glob crate compatibility
    let normalized = normalize_pattern(pattern);

    // Glob the pattern
    match glob::glob(&normalized) {
        Ok(paths) => {
            let mut results: Vec<String> = paths
                .filter_map(|entry| entry.ok())
                .filter(|path| {
                    // Filter out hidden files unless pattern explicitly includes them
                    // A pattern explicitly includes hidden if it starts with . or contains /.
                    let filename = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");

                    !filename.starts_with('.') || pattern.contains("/.")
                })
                .map(|path| path.display().to_string())
                .collect();

            if results.is_empty() {
                // No matches - return literal pattern (POSIX behavior)
                Ok(vec![pattern.to_string()])
            } else {
                // Sort results (bash sorts glob results)
                results.sort();
                Ok(results)
            }
        }
        Err(_) => {
            // Invalid pattern - return as-is
            Ok(vec![pattern.to_string()])
        }
    }
}

/// Check if a pattern contains glob metacharacters
fn contains_glob_chars(s: &str) -> bool {
    for c in s.chars() {
        match c {
            '*' | '?' | '[' | ']' => return true,
            _ => {}
        }
    }
    false
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
}
