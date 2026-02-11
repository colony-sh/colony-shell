//! Parser for TOOLS.md files
//!
//! Parses markdown tool definitions into `ToolConfig` structs.
//! Each `## heading` starts a new tool definition with optional
//! `### Schema` and `### Configuration` subsections.

use std::collections::HashMap;
use std::path::Path;

use super::types::ToolConfig;

#[derive(Debug, PartialEq, Eq)]
enum ParserState {
    Preamble,
    Description,
    Schema,
    Configuration,
}

struct ToolBuilder {
    name: String,
    description_lines: Vec<String>,
    schema_lines: Vec<String>,
    configuration: HashMap<String, String>,
    in_code_block: bool,
}

impl ToolBuilder {
    fn new(name: String) -> Self {
        Self {
            name,
            description_lines: Vec::new(),
            schema_lines: Vec::new(),
            configuration: HashMap::new(),
            in_code_block: false,
        }
    }

    fn build(self) -> ToolConfig {
        let description = self.description_lines.join("\n").trim().to_string();

        let schema = if self.schema_lines.is_empty() {
            None
        } else {
            let json_str = self.schema_lines.join("\n");
            serde_json::from_str(&json_str).ok()
        };

        ToolConfig {
            name: self.name,
            description,
            schema,
            configuration: self.configuration,
        }
    }
}

pub struct ToolsMdParser;

impl ToolsMdParser {
    /// Parse TOOLS.md content into tool configurations.
    pub fn parse(content: &str, _source_file: &Path) -> Vec<ToolConfig> {
        let mut tools = Vec::new();
        let mut current: Option<ToolBuilder> = None;
        let mut state = ParserState::Preamble;

        for line in content.lines() {
            // New tool definition
            if let Some(name) = line.strip_prefix("## ") {
                let name = name.trim();
                if !name.is_empty() {
                    if let Some(builder) = current.take() {
                        tools.push(builder.build());
                    }
                    current = Some(ToolBuilder::new(name.to_string()));
                    state = ParserState::Description;
                    continue;
                }
            }

            // Separator ends current tool
            if line.trim() == "---" {
                if let Some(builder) = current.take() {
                    tools.push(builder.build());
                }
                state = ParserState::Preamble;
                continue;
            }

            let Some(builder) = current.as_mut() else {
                continue;
            };

            // Subsection headings
            if let Some(section) = line.strip_prefix("### ") {
                builder.in_code_block = false;
                match section.trim() {
                    "Schema" => state = ParserState::Schema,
                    "Configuration" => state = ParserState::Configuration,
                    _ => state = ParserState::Description,
                }
                continue;
            }

            match state {
                ParserState::Preamble => {}
                ParserState::Description => {
                    builder.description_lines.push(line.to_string());
                }
                ParserState::Schema => {
                    let trimmed = line.trim();
                    if trimmed.starts_with("```") {
                        builder.in_code_block = !builder.in_code_block;
                        continue;
                    }
                    if builder.in_code_block {
                        builder.schema_lines.push(line.to_string());
                    }
                }
                ParserState::Configuration => {
                    if let Some(item) = line.strip_prefix("- ") {
                        let item = item.trim();
                        if let Some((key, value)) = item.split_once(':') {
                            builder
                                .configuration
                                .insert(key.trim().to_string(), value.trim().to_string());
                        }
                    }
                }
            }
        }

        // Finalize last tool
        if let Some(builder) = current.take() {
            tools.push(builder.build());
        }

        tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_path() -> PathBuf {
        PathBuf::from("test/TOOLS.md")
    }

    #[test]
    fn test_parse_tool_with_schema() {
        let content = r#"# TOOLS.md

Tool definitions.

---

## shell

Execute shell commands.

### Schema

```json
{
  "type": "object",
  "properties": {
    "command": { "type": "string" }
  }
}
```

### Configuration

- timeout: 30s
- allowed_commands: [ls, cat, grep, git]
"#;
        let tools = ToolsMdParser::parse(content, &test_path());
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "shell");
        assert_eq!(tools[0].description, "Execute shell commands.");
        assert!(tools[0].schema.is_some());
        let schema = tools[0].schema.as_ref().unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(tools[0].configuration["timeout"], "30s");
    }

    #[test]
    fn test_parse_multiple_tools() {
        let content = r#"# TOOLS.md

---

## read_file

Read a file.

### Configuration

- max_size: 1MB

---

## write_file

Write a file.

### Configuration

- require_confirmation: true
"#;
        let tools = ToolsMdParser::parse(content, &test_path());
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[1].name, "write_file");
        assert_eq!(tools[1].configuration["require_confirmation"], "true");
    }

    #[test]
    fn test_parse_empty_tools() {
        let content = "# TOOLS.md\n\nNo tools.\n";
        let tools = ToolsMdParser::parse(content, &test_path());
        assert!(tools.is_empty());
    }
}
