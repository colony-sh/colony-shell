//! Parser for PROMPT.md files
//!
//! Parses markdown prompt templates into `PromptTemplate` structs.
//! Each `## heading` defines a named template. The template content
//! is the first fenced code block after the heading.

use std::path::Path;

use super::types::PromptTemplate;

pub struct PromptMdParser;

impl PromptMdParser {
    /// Parse PROMPT.md content into prompt templates.
    ///
    /// Extracts `{variable}` placeholders from template text.
    pub fn parse(content: &str, _source_file: &Path) -> Vec<PromptTemplate> {
        let mut templates = Vec::new();
        let mut current_name: Option<String> = None;
        let mut code_lines: Vec<String> = Vec::new();
        let mut in_code_block = false;
        let mut past_preamble = false;
        let mut captured_template = false;

        for line in content.lines() {
            // Skip preamble
            if !past_preamble {
                if line.trim() == "---" {
                    past_preamble = true;
                }
                continue;
            }

            // New template heading
            if let Some(name) = line.strip_prefix("## ") {
                let name = name.trim();
                if !name.is_empty() {
                    // Finalize previous template
                    if let Some(prev_name) = current_name.take() {
                        let template_text = code_lines.join("\n").trim().to_string();
                        if !template_text.is_empty() {
                            let variables = extract_variables(&template_text);
                            templates.push(PromptTemplate {
                                name: prev_name,
                                template: template_text,
                                variables,
                            });
                        }
                    }
                    current_name = Some(name.to_string());
                    code_lines.clear();
                    in_code_block = false;
                    captured_template = false;
                    continue;
                }
            }

            // Separator ends current template
            if line.trim() == "---" {
                if let Some(prev_name) = current_name.take() {
                    let template_text = code_lines.join("\n").trim().to_string();
                    if !template_text.is_empty() {
                        let variables = extract_variables(&template_text);
                        templates.push(PromptTemplate {
                            name: prev_name,
                            template: template_text,
                            variables,
                        });
                    }
                }
                code_lines.clear();
                in_code_block = false;
                captured_template = false;
                continue;
            }

            if current_name.is_none() || captured_template {
                continue;
            }

            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if in_code_block {
                    // End of code block -- we captured the template
                    in_code_block = false;
                    captured_template = true;
                } else {
                    in_code_block = true;
                }
                continue;
            }

            if in_code_block {
                code_lines.push(line.to_string());
            }
        }

        // Finalize last template
        if let Some(name) = current_name.take() {
            let template_text = code_lines.join("\n").trim().to_string();
            if !template_text.is_empty() {
                let variables = extract_variables(&template_text);
                templates.push(PromptTemplate {
                    name,
                    template: template_text,
                    variables,
                });
            }
        }

        templates
    }
}

/// Extract `{variable}` placeholders from template text.
fn extract_variables(template: &str) -> Vec<String> {
    let mut variables = Vec::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut var_name = String::new();
            let mut valid = true;
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
                if inner.is_alphanumeric() || inner == '_' {
                    var_name.push(inner);
                } else {
                    valid = false;
                    break;
                }
            }
            if valid && !var_name.is_empty() && !variables.contains(&var_name) {
                variables.push(var_name);
            }
        }
    }

    variables
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_path() -> PathBuf {
        PathBuf::from("test/PROMPT.md")
    }

    #[test]
    fn test_parse_prompt_templates() {
        let content = r#"# PROMPT.md

Prompt templates.

---

## code-review-template

```
You are reviewing {project} code.
Focus on: {focus}
Context: {context}
```

---

## commit-message-template

```
Generate a commit message for:
{diff}
```
"#;
        let templates = PromptMdParser::parse(content, &test_path());
        assert_eq!(templates.len(), 2);
        assert_eq!(templates[0].name, "code-review-template");
        assert!(templates[0].template.contains("reviewing {project}"));
        assert_eq!(templates[0].variables, vec!["project", "focus", "context"]);
        assert_eq!(templates[1].name, "commit-message-template");
        assert_eq!(templates[1].variables, vec!["diff"]);
    }

    #[test]
    fn test_parse_empty_prompt() {
        let content = "# PROMPT.md\n\nNo templates.\n";
        let templates = PromptMdParser::parse(content, &test_path());
        assert!(templates.is_empty());
    }

    #[test]
    fn test_extract_variables() {
        let vars = extract_variables("Hello {name}, your {item} is ready.");
        assert_eq!(vars, vec!["name", "item"]);
    }

    #[test]
    fn test_extract_variables_dedup() {
        let vars = extract_variables("{a} and {b} and {a} again");
        assert_eq!(vars, vec!["a", "b"]);
    }

    #[test]
    fn test_extract_variables_ignores_invalid() {
        let vars = extract_variables("{ space } and {valid} and {with-dash}");
        assert_eq!(vars, vec!["valid"]);
    }
}
