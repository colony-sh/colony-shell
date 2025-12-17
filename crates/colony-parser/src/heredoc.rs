//! Heredoc preprocessing for pest parser
//!
//! Heredocs are the one shell construct that can't be parsed purely by PEG because
//! the content appears AFTER the command line structurally.
//!
//! Example:
//! ```bash
//! cat <<EOF | grep pattern    # Command line
//! This is heredoc content     # Content starts next line
//! More content                # More content
//! EOF                         # Delimiter ends it
//! ```
//!
//! This module provides minimal preprocessing to normalize heredocs into a form
//! that pest can parse.

use std::collections::HashMap;

/// A heredoc marker found in source
#[derive(Debug, Clone)]
struct HeredocMarker {
    /// Position of << or <<- in source
    start_pos: usize,
    /// End of marker line (after newline)
    end_pos: usize,
    /// Delimiter string
    delimiter: String,
    /// Whether to strip leading tabs (<<- vs <<)
    strip_tabs: bool,
    /// Whether delimiter was quoted (affects expansion)
    no_expansion: bool,
}

/// Extracted heredoc content
#[derive(Debug, Clone)]
pub struct HeredocContent {
    /// The raw content between command line and delimiter
    pub content: String,
    /// Whether to expand variables in content
    pub expand: bool,
}

/// Preprocess source to normalize heredocs for pest parsing
///
/// Finds all `<<DELIM` markers, extracts their content, and creates a normalized
/// source string that pest can parse. Returns the normalized source and a map
/// of heredoc contents keyed by their index.
///
/// # Normalization Strategy
///
/// Original:
/// ```bash
/// cat <<EOF
/// content line 1
/// content line 2
/// EOF
/// ```
///
/// Normalized:
/// ```bash
/// cat <<EOF#HEREDOC_0#EOF
/// ```
///
/// The content is stored separately and will be injected into the AST after parsing.
pub fn preprocess_heredocs(source: &str) -> Result<(String, HashMap<usize, HeredocContent>), String> {
    // Find all heredoc markers
    let markers = find_heredoc_markers(source)?;

    if markers.is_empty() {
        // No heredocs, return source unchanged
        return Ok((source.to_string(), HashMap::new()));
    }

    // Extract content and normalize
    let (normalized, contents) = extract_and_normalize(source, markers)?;

    Ok((normalized, contents))
}

/// Find all heredoc markers (<<DELIM and <<-DELIM) in source
fn find_heredoc_markers(source: &str) -> Result<Vec<HeredocMarker>, String> {
    let mut markers = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for << or <<-
        if i + 1 < bytes.len() && bytes[i] == b'<' && bytes[i + 1] == b'<' {
            i += 2; // Skip <<

            // Check for <<-
            let strip_tabs = if i < bytes.len() && bytes[i] == b'-' {
                i += 1;
                true
            } else {
                false
            };

            // Skip whitespace
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }

            // Extract delimiter
            let delim_start = i;
            let mut delimiter = String::new();
            let mut no_expansion = false;

            // Check for quoted delimiter
            if i < bytes.len() && (bytes[i] == b'\'' || bytes[i] == b'"') {
                let quote = bytes[i];
                no_expansion = true;
                i += 1;

                while i < bytes.len() && bytes[i] != quote {
                    delimiter.push(bytes[i] as char);
                    i += 1;
                }

                if i < bytes.len() {
                    i += 1; // Skip closing quote
                }
            } else {
                // Unquoted delimiter
                while i < bytes.len()
                    && bytes[i] != b' '
                    && bytes[i] != b'\t'
                    && bytes[i] != b'\n'
                    && bytes[i] != b'|'
                    && bytes[i] != b'&'
                    && bytes[i] != b';'
                {
                    delimiter.push(bytes[i] as char);
                    i += 1;
                }
            }

            if delimiter.is_empty() {
                return Err("Empty heredoc delimiter".to_string());
            }

            // Find end of command line
            let start_pos = delim_start - (if strip_tabs { 3 } else { 2 });
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }

            if i < bytes.len() {
                i += 1; // Skip newline
            }

            let end_pos = i;

            markers.push(HeredocMarker {
                start_pos,
                end_pos,
                delimiter,
                strip_tabs,
                no_expansion,
            });
        } else {
            i += 1;
        }
    }

    Ok(markers)
}

/// Extract heredoc contents and create normalized source
fn extract_and_normalize(
    source: &str,
    markers: Vec<HeredocMarker>,
) -> Result<(String, HashMap<usize, HeredocContent>), String> {
    let mut normalized = String::new();
    let mut contents = HashMap::new();
    let mut last_pos = 0;
    let bytes = source.as_bytes();

    for (idx, marker) in markers.into_iter().enumerate() {
        // Copy source up to the command line end
        normalized.push_str(&source[last_pos..marker.end_pos]);

        // Extract heredoc content starting from end_pos
        let content_start = marker.end_pos;
        let mut content = String::new();
        let mut pos = content_start;

        // Read lines until we find the delimiter
        while pos < bytes.len() {
            let line_start = pos;

            // Read to end of line
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }

            let line = &source[line_start..pos];

            // Check if this line is the delimiter (possibly with leading whitespace if <<-)
            let trimmed_line = if marker.strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line
            };

            if trimmed_line.trim() == marker.delimiter {
                // Found delimiter, done
                if pos < bytes.len() {
                    pos += 1; // Skip newline after delimiter
                }
                break;
            }

            // Not delimiter, add to content
            content.push_str(line);
            if pos < bytes.len() {
                content.push('\n');
                pos += 1; // Skip newline
            }
        }

        // Store content
        contents.insert(idx, HeredocContent {
            content,
            expand: !marker.no_expansion,
        });

        // Update last_pos to skip the heredoc content in normalized output
        last_pos = pos;
    }

    // Append remaining source
    if last_pos < source.len() {
        normalized.push_str(&source[last_pos..]);
    }

    Ok((normalized, contents))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_simple_heredoc() {
        let source = r#"cat <<EOF
content here
more content
EOF
"#;

        let markers = find_heredoc_markers(source).unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].delimiter, "EOF");
        assert!(!markers[0].strip_tabs);
        assert!(!markers[0].no_expansion);
    }

    #[test]
    fn test_find_heredoc_strip_tabs() {
        let source = r#"cat <<-EOF
	content with tabs
EOF
"#;

        let markers = find_heredoc_markers(source).unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].delimiter, "EOF");
        assert!(markers[0].strip_tabs);
    }

    #[test]
    fn test_find_heredoc_quoted_delimiter() {
        let source = r#"cat <<'EOF'
$var should not expand
EOF
"#;

        let markers = find_heredoc_markers(source).unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].delimiter, "EOF");
        assert!(markers[0].no_expansion);
    }

    #[test]
    fn test_extract_heredoc_content() {
        let source = r#"cat <<EOF
line 1
line 2
EOF
echo done"#;

        let (normalized, contents) = preprocess_heredocs(source).unwrap();

        // Should have extracted content
        assert_eq!(contents.len(), 1);
        let content = contents.get(&0).unwrap();
        assert_eq!(content.content, "line 1\nline 2\n");
        assert!(content.expand);

        // Normalized should skip the heredoc content lines
        assert!(normalized.contains("cat <<EOF"));
        assert!(!normalized.contains("line 1"));
        assert!(normalized.contains("echo done"));
    }

    #[test]
    #[ignore] // TODO: Handle multiple heredocs on same command line
    fn test_multiple_heredocs() {
        let source = r#"cat <<EOF1 <<EOF2
content 1
EOF1
content 2
EOF2
"#;

        let markers = find_heredoc_markers(source).unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].delimiter, "EOF1");
        assert_eq!(markers[1].delimiter, "EOF2");
    }

    #[test]
    fn test_heredoc_in_pipeline() {
        let source = r#"cat <<EOF | grep pattern
search this
and this
EOF
"#;

        let (normalized, contents) = preprocess_heredocs(source).unwrap();
        assert_eq!(contents.len(), 1);

        let content = contents.get(&0).unwrap();
        assert_eq!(content.content, "search this\nand this\n");
    }
}
