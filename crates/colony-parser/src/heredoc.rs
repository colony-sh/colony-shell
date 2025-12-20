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

pub struct HeredocContent {
    pub content: String,
    pub expand: bool,
    pub strip_tabs: bool,
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
/// Skips heredocs that appear inside $() since they'll be handled when the comsub is parsed
fn find_heredoc_markers(source: &str) -> Result<Vec<HeredocMarker>, String> {
    let mut markers = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut current_line_end = None;
    let mut paren_depth = 0;
    let mut in_comsub = false;

    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'(' {
            i += 2;
            paren_depth += 1;
            in_comsub = true;
            continue;
        }
        
        if in_comsub && bytes[i] == b'(' {
            paren_depth += 1;
            i += 1;
            continue;
        }
        
        if in_comsub && bytes[i] == b')' {
            paren_depth -= 1;
            if paren_depth == 0 {
                in_comsub = false;
            }
            i += 1;
            continue;
        }
        
        if in_comsub {
            if bytes[i] == b'\n' {
                current_line_end = None;
            }
            i += 1;
            continue;
        }
        
        if i + 1 < bytes.len() && bytes[i] == b'<' && bytes[i + 1] == b'<' {
            i += 2;

            let strip_tabs = if i < bytes.len() && bytes[i] == b'-' {
                i += 1;
                true
            } else {
                false
            };

            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }

            let delim_start = i;
            let mut delimiter = String::new();
            let mut no_expansion = false;

            if i < bytes.len() && (bytes[i] == b'\'' || bytes[i] == b'"') {
                let quote = bytes[i];
                no_expansion = true;
                i += 1;

                while i < bytes.len() && bytes[i] != quote {
                    delimiter.push(bytes[i] as char);
                    i += 1;
                }

                if i < bytes.len() {
                    i += 1;
                }
            } else {
                while i < bytes.len()
                    && bytes[i] != b' '
                    && bytes[i] != b'\t'
                    && bytes[i] != b'\n'
                    && bytes[i] != b'|'
                    && bytes[i] != b'&'
                    && bytes[i] != b';'
                    && bytes[i] != b'<'
                {
                    delimiter.push(bytes[i] as char);
                    i += 1;
                }
            }

            if delimiter.is_empty() {
                return Err("Empty heredoc delimiter".to_string());
            }

            let start_pos = delim_start - (if strip_tabs { 3 } else { 2 });
            
            if current_line_end.is_none() {
                let mut end = i;
                while end < bytes.len() && bytes[end] != b'\n' {
                    end += 1;
                }
                if end < bytes.len() {
                    end += 1;
                }
                current_line_end = Some(end);
            }

            markers.push(HeredocMarker {
                start_pos,
                end_pos: current_line_end.unwrap(),
                delimiter,
                strip_tabs,
                no_expansion,
            });
        } else if bytes[i] == b'\n' {
            current_line_end = None;
            i += 1;
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
    let bytes = source.as_bytes();

    // Group markers by their command line (same end_pos means same line)
    // This handles multiple heredocs on a single line correctly
    let mut marker_groups: Vec<Vec<(usize, HeredocMarker)>> = Vec::new();
    let mut current_group: Vec<(usize, HeredocMarker)> = Vec::new();
    let mut current_end_pos = 0;

    for (idx, marker) in markers.into_iter().enumerate() {
        if current_group.is_empty() || marker.end_pos == current_end_pos {
            current_end_pos = marker.end_pos;
            current_group.push((idx, marker));
        } else {
            marker_groups.push(std::mem::take(&mut current_group));
            current_end_pos = marker.end_pos;
            current_group.push((idx, marker));
        }
    }
    if !current_group.is_empty() {
        marker_groups.push(current_group);
    }

    let mut last_pos = 0;

    for group in marker_groups {
        if group.is_empty() {
            continue;
        }

        // All markers in this group share the same end_pos (command line end)
        let group_end_pos = group[0].1.end_pos;
        
        // Copy source up to the command line end (only once per group)
        if group_end_pos > last_pos {
            normalized.push_str(&source[last_pos..group_end_pos]);
        }

        // Process heredocs in sequence - content appears in marker order
        let mut pos = group_end_pos;

        for (idx, marker) in group {
            let mut content = String::new();

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
                    if pos < bytes.len() {
                        pos += 1;
                    }
                    break;
                }

                // POSIX: backslash-newline joins lines (unquoted delimiters only)
                let mut line_content = line.to_string();
                if !marker.no_expansion {
                    while line_content.ends_with('\\') && pos < bytes.len() {
                        line_content.pop();
                        
                        if pos < bytes.len() && bytes[pos] == b'\n' {
                            pos += 1;
                        }
                        
                        // Lookahead: check if next line is delimiter
                        let next_line_start = pos;
                        while pos < bytes.len() && bytes[pos] != b'\n' {
                            pos += 1;
                        }
                        
                        let next_line = &source[next_line_start..pos];
                        let trimmed_next = if marker.strip_tabs {
                            next_line.trim_start_matches('\t')
                        } else {
                            next_line
                        };
                        
                        if trimmed_next.trim() == marker.delimiter {
                            // Backtrack: delimiter found, stop continuation
                            pos = next_line_start;
                            break;
                        }
                        
                        line_content.push_str(next_line);
                    }
                }

                // Not delimiter, add to content
                content.push_str(&line_content);
                if pos < bytes.len() && bytes[pos] == b'\n' {
                    content.push('\n');
                    pos += 1; // Skip newline
                }
            }

            contents.insert(idx, HeredocContent {
                content,
                expand: !marker.no_expansion,
                strip_tabs: marker.strip_tabs,
            });
        }

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

    #[test]
    fn test_heredoc_in_command_substitution() {
        let source = r#"x=$(cat <<EOF
hello
world
EOF
)
echo "$x""#;

        let (normalized, contents) = preprocess_heredocs(source).unwrap();
        
        println!("Normalized source:\n{}", normalized);
        println!("Contents: {} items", contents.len());
        for (idx, content) in contents.iter() {
            println!("  [{}]: {:?}", idx, content.content);
        }
        
        assert_eq!(contents.len(), 0);
        assert_eq!(normalized, source);
    }
}
