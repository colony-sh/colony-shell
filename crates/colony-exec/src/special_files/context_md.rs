//! Parser for CONTEXT.md files
//!
//! Parses markdown context sections into `ContextEntry` structs.
//! Each `## heading` becomes a separate context entry.

use std::path::Path;

use super::types::ContextEntry;

pub struct ContextMdParser;

impl ContextMdParser {
    /// Parse CONTEXT.md content into context entries.
    ///
    /// Skips preamble (everything before first `---`).
    /// Each `## heading` starts a new section.
    pub fn parse(content: &str, source_file: &Path) -> Vec<ContextEntry> {
        let mut entries = Vec::new();
        let mut current_section: Option<String> = None;
        let mut current_lines: Vec<String> = Vec::new();
        let mut past_preamble = false;

        for line in content.lines() {
            // Skip everything before first ---
            if !past_preamble {
                if line.trim() == "---" {
                    past_preamble = true;
                }
                continue;
            }

            // New section heading
            if let Some(heading) = line.strip_prefix("## ") {
                let heading = heading.trim();
                // Finalize previous section
                if let Some(section) = current_section.take() {
                    let content_text = current_lines.join("\n").trim().to_string();
                    if !content_text.is_empty() {
                        entries.push(ContextEntry {
                            section,
                            content: content_text,
                            source_file: source_file.to_path_buf(),
                        });
                    }
                }
                current_section = Some(heading.to_string());
                current_lines.clear();
                continue;
            }

            if current_section.is_some() {
                current_lines.push(line.to_string());
            }
        }

        // Finalize last section
        if let Some(section) = current_section.take() {
            let content_text = current_lines.join("\n").trim().to_string();
            if !content_text.is_empty() {
                entries.push(ContextEntry {
                    section,
                    content: content_text,
                    source_file: source_file.to_path_buf(),
                });
            }
        }

        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_path() -> PathBuf {
        PathBuf::from("test/CONTEXT.md")
    }

    #[test]
    fn test_parse_context_sections() {
        let content = r#"# CONTEXT.md

Context seed for this project.

---

## Project Overview

Colony is an agent-first shell written in Rust.

## Architecture

- `crates/colony-parser/` - PEG parser
- `crates/colony-exec/` - Execution engine

## Current Focus

Working on special files system (F030).
"#;
        let entries = ContextMdParser::parse(content, &test_path());
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].section, "Project Overview");
        assert!(entries[0]
            .content
            .contains("Colony is an agent-first shell"));
        assert_eq!(entries[1].section, "Architecture");
        assert!(entries[1].content.contains("colony-parser"));
        assert_eq!(entries[2].section, "Current Focus");
        assert!(entries[2].content.contains("F030"));
    }

    #[test]
    fn test_parse_empty_context() {
        let content = "# CONTEXT.md\n\nNothing here.\n";
        let entries = ContextMdParser::parse(content, &test_path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_preamble_ignored() {
        let content = r#"# CONTEXT.md

This preamble text should be ignored.
It has multiple lines.

---

## Actual Content

This is the real content.
"#;
        let entries = ContextMdParser::parse(content, &test_path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].section, "Actual Content");
        assert!(!entries[0].content.contains("preamble"));
    }

    #[test]
    fn test_empty_sections_skipped() {
        let content = r#"# CONTEXT.md

---

## Empty Section

## Non-empty Section

Has content.
"#;
        let entries = ContextMdParser::parse(content, &test_path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].section, "Non-empty Section");
    }
}
