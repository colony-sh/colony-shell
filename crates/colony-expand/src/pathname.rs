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
/// If the pattern matches files, returns the list of paths.
/// If the pattern doesn't match anything, returns the pattern itself.
pub fn expand_pathname(pattern: &str) -> Result<Vec<String>> {
    // Check if pattern contains glob meta-characters
    if !contains_glob_chars(pattern) {
        // No glob chars - return as-is
        return Ok(vec![pattern.to_string()]);
    }

    // Try to glob
    // TODO: Add glob crate dependency and implement
    // For now, return pattern unchanged
    Ok(vec![pattern.to_string()])
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
