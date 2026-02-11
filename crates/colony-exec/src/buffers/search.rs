//! Buffer search
//!
//! Find text matches within a `StyledBuffer` and return their positions.

use super::buffer::StyledBuffer;
use serde::{Deserialize, Serialize};

/// A search result within a buffer
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub line: usize,
    pub column: usize,
    pub length: usize,
}

/// Find all occurrences of a pattern in the buffer
pub fn search_buffer(buffer: &StyledBuffer, pattern: &str) -> Vec<SearchResult> {
    if pattern.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    for (line_idx, line) in buffer.lines().iter().enumerate() {
        let text = line.text();
        let mut start = 0;
        while let Some(pos) = text[start..].find(pattern) {
            results.push(SearchResult {
                line: line_idx,
                column: start + pos,
                length: pattern.len(),
            });
            start += pos + 1;
        }
    }

    results
}

/// Find all occurrences of a pattern (case-insensitive) in the buffer
pub fn search_buffer_case_insensitive(buffer: &StyledBuffer, pattern: &str) -> Vec<SearchResult> {
    if pattern.is_empty() {
        return Vec::new();
    }

    let pattern_lower = pattern.to_lowercase();
    let mut results = Vec::new();

    for (line_idx, line) in buffer.lines().iter().enumerate() {
        let text = line.text();
        let text_lower = text.to_lowercase();
        let mut start = 0;
        while let Some(pos) = text_lower[start..].find(&pattern_lower) {
            results.push(SearchResult {
                line: line_idx,
                column: start + pos,
                length: pattern.len(),
            });
            start += pos + 1;
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buffer(lines: &[&str]) -> StyledBuffer {
        let mut buf = StyledBuffer::new();
        for line in lines {
            buf.push_text(line);
        }
        buf
    }

    #[test]
    fn test_search_single_match() {
        let buf = make_buffer(&["hello world"]);
        let results = search_buffer(&buf, "world");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line, 0);
        assert_eq!(results[0].column, 6);
        assert_eq!(results[0].length, 5);
    }

    #[test]
    fn test_search_multiple_matches_same_line() {
        let buf = make_buffer(&["abcabc"]);
        let results = search_buffer(&buf, "abc");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].column, 0);
        assert_eq!(results[1].column, 3);
    }

    #[test]
    fn test_search_multiple_lines() {
        let buf = make_buffer(&["foo bar", "baz foo", "qux"]);
        let results = search_buffer(&buf, "foo");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].line, 0);
        assert_eq!(results[1].line, 1);
    }

    #[test]
    fn test_search_no_match() {
        let buf = make_buffer(&["hello world"]);
        let results = search_buffer(&buf, "xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_empty_pattern() {
        let buf = make_buffer(&["hello"]);
        let results = search_buffer(&buf, "");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_empty_buffer() {
        let buf = StyledBuffer::new();
        let results = search_buffer(&buf, "test");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_case_insensitive() {
        let buf = make_buffer(&["Hello WORLD hello"]);
        let results = search_buffer_case_insensitive(&buf, "hello");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].column, 0);
        assert_eq!(results[1].column, 12);
    }

    #[test]
    fn test_search_case_insensitive_no_match() {
        let buf = make_buffer(&["hello world"]);
        let results = search_buffer_case_insensitive(&buf, "xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            line: 5,
            column: 10,
            length: 3,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"line\":5"));
        assert!(json.contains("\"column\":10"));
        assert!(json.contains("\"length\":3"));
    }
}
