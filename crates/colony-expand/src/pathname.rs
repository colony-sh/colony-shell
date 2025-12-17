//! Pathname expansion (globbing)
//!
//! POSIX glob patterns:
//! - * matches any string
//! - ? matches any single character
//! - [...] matches any character in brackets
//! - [!...] matches any character NOT in brackets

use crate::Result;

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

    // Glob the pattern
    match glob::glob(pattern) {
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
    fn test_glob_not_yet_implemented() {
        // For now, glob patterns return unchanged
        let result = expand_pathname("*.txt").unwrap();
        assert_eq!(result, vec!["*.txt"]);
    }
}
