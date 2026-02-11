//! Parser for AGENT.md files
//!
//! Parses markdown agent definitions into `AgentDefinition` structs.
//! Uses a line-based state machine to handle the structured markdown format.

use std::path::Path;

use super::types::{AgentCapabilities, AgentDefinition, AgentParameter};

/// State machine states for the AGENT.md parser
#[derive(Debug, PartialEq, Eq)]
enum ParserState {
    Preamble,
    Description,
    Capabilities,
    Tools,
    Parameters,
    Prompt,
    Hooks,
}

/// Builder for constructing an AgentDefinition during parsing
struct AgentBuilder {
    name: String,
    description_lines: Vec<String>,
    capabilities: AgentCapabilities,
    tools: Vec<String>,
    parameters: Vec<AgentParameter>,
    prompt_lines: Vec<String>,
    hooks_lines: Vec<String>,
    in_code_block: bool,
}

impl AgentBuilder {
    fn new(name: String) -> Self {
        Self {
            name,
            description_lines: Vec::new(),
            capabilities: AgentCapabilities::default(),
            tools: Vec::new(),
            parameters: Vec::new(),
            prompt_lines: Vec::new(),
            hooks_lines: Vec::new(),
            in_code_block: false,
        }
    }

    fn build(self, source_file: &Path) -> AgentDefinition {
        let description = self.description_lines.join("\n").trim().to_string();

        let prompt_template = self.prompt_lines.join("\n").trim().to_string();

        let hooks = if self.hooks_lines.is_empty() {
            None
        } else {
            Some(self.hooks_lines.join("\n").trim().to_string())
        };

        AgentDefinition {
            name: self.name,
            description,
            capabilities: self.capabilities,
            tools: self.tools,
            parameters: self.parameters,
            prompt_template,
            hooks,
            source_file: source_file.to_path_buf(),
        }
    }
}

pub struct AgentMdParser;

impl AgentMdParser {
    /// Parse AGENT.md content into a list of agent definitions.
    ///
    /// Returns an error if the content is fundamentally malformed.
    /// Missing optional sections are treated as empty.
    pub fn parse(content: &str, source_file: &Path) -> Result<Vec<AgentDefinition>, ParseError> {
        let mut agents: Vec<AgentDefinition> = Vec::new();
        let mut current: Option<AgentBuilder> = None;
        let mut state = ParserState::Preamble;
        let mut param_headers: Vec<String> = Vec::new();

        for line in content.lines() {
            // Check for ## heading (new agent definition)
            if let Some(name) = line.strip_prefix("## ") {
                let name = name.trim();
                if !name.is_empty() {
                    // Finalize previous agent
                    if let Some(builder) = current.take() {
                        agents.push(builder.build(source_file));
                    }
                    current = Some(AgentBuilder::new(name.to_string()));
                    state = ParserState::Description;
                    continue;
                }
            }

            // Check for --- separator (also ends current agent in preamble)
            if line.trim() == "---" {
                if state == ParserState::Preamble {
                    continue;
                }
                // Separator can end current agent if between agents
                if let Some(builder) = current.take() {
                    agents.push(builder.build(source_file));
                }
                state = ParserState::Preamble;
                continue;
            }

            let Some(builder) = current.as_mut() else {
                // In preamble or between agents, skip
                continue;
            };

            // Check for ### subsection headings
            if let Some(section) = line.strip_prefix("### ") {
                let section = section.trim();
                builder.in_code_block = false;
                match section {
                    "Capabilities" => state = ParserState::Capabilities,
                    "Tools" => state = ParserState::Tools,
                    "Parameters" => {
                        state = ParserState::Parameters;
                        param_headers.clear();
                    }
                    "Prompt" => state = ParserState::Prompt,
                    "Hooks" => state = ParserState::Hooks,
                    // Unknown subsection treated as description continuation
                    _ => state = ParserState::Description,
                }
                continue;
            }

            // Process line based on current state
            match state {
                ParserState::Preamble => {}
                ParserState::Description => {
                    builder.description_lines.push(line.to_string());
                }
                ParserState::Capabilities => {
                    if let Some(item) = line.strip_prefix("- ") {
                        let item = item.trim();
                        if let Some((key, value)) = item.split_once(':') {
                            let key = key.trim();
                            let value = value.trim();
                            match key {
                                "model" => {
                                    builder.capabilities.model = Some(value.to_string());
                                }
                                "max_tokens" => {
                                    if let Ok(n) = value.parse::<u32>() {
                                        builder.capabilities.max_tokens = Some(n);
                                    }
                                }
                                "temperature" => {
                                    if let Ok(t) = value.parse::<f32>() {
                                        builder.capabilities.temperature = Some(t);
                                    }
                                }
                                "mcp_server" | "mcp_servers" => {
                                    builder.capabilities.mcp_servers.push(value.to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                }
                ParserState::Tools => {
                    if let Some(item) = line.strip_prefix("- ") {
                        let tool = item.trim();
                        if !tool.is_empty() {
                            builder.tools.push(tool.to_string());
                        }
                    }
                }
                ParserState::Parameters => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    // Parse markdown table
                    if trimmed.starts_with('|') && trimmed.ends_with('|') {
                        let cells: Vec<&str> = trimmed
                            .trim_matches('|')
                            .split('|')
                            .map(str::trim)
                            .collect();

                        // Detect header row
                        if param_headers.is_empty() {
                            param_headers = cells.iter().map(|c| c.to_lowercase()).collect();
                            continue;
                        }

                        // Skip separator row (---|---|---)
                        if cells
                            .iter()
                            .all(|c| c.chars().all(|ch| ch == '-' || ch == ' '))
                        {
                            continue;
                        }

                        // Parse data row using header positions
                        let get_col = |name: &str| -> String {
                            param_headers
                                .iter()
                                .position(|h| h == name)
                                .and_then(|i| cells.get(i))
                                .map(|s| s.to_string())
                                .unwrap_or_default()
                        };

                        let name = get_col("name");
                        if name.is_empty() {
                            continue;
                        }

                        let required_str = get_col("required").to_lowercase();
                        let required = required_str == "yes" || required_str == "true";

                        let default_val = get_col("default");
                        let default = if default_val.is_empty() {
                            None
                        } else {
                            Some(default_val)
                        };

                        builder.parameters.push(AgentParameter {
                            name,
                            type_name: get_col("type"),
                            required,
                            default,
                            description: get_col("description"),
                        });
                    }
                }
                ParserState::Prompt => {
                    if trimmed_is_code_fence(line) {
                        builder.in_code_block = !builder.in_code_block;
                        continue;
                    }
                    if builder.in_code_block {
                        builder.prompt_lines.push(line.to_string());
                    }
                }
                ParserState::Hooks => {
                    if trimmed_is_code_fence(line) {
                        builder.in_code_block = !builder.in_code_block;
                        continue;
                    }
                    if builder.in_code_block {
                        builder.hooks_lines.push(line.to_string());
                    }
                }
            }
        }

        // Finalize last agent
        if let Some(builder) = current.take() {
            agents.push(builder.build(source_file));
        }

        Ok(agents)
    }
}

fn trimmed_is_code_fence(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("```")
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("I/O error reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_path() -> PathBuf {
        PathBuf::from("test/AGENT.md")
    }

    #[test]
    fn test_parse_single_agent() {
        let content = r#"# AGENT.md

---

## code-reviewer

Reviews code quality.

### Capabilities

- model: claude-sonnet-4-20250514
- max_tokens: 8192
- temperature: 0.3

### Tools

- read_file
- search_code

### Prompt

```
Review the following code.
```
"#;
        let agents = AgentMdParser::parse(content, &test_path()).unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "code-reviewer");
        assert_eq!(agents[0].description, "Reviews code quality.");
        assert_eq!(
            agents[0].capabilities.model,
            Some("claude-sonnet-4-20250514".to_string())
        );
        assert_eq!(agents[0].capabilities.max_tokens, Some(8192));
        assert_eq!(agents[0].capabilities.temperature, Some(0.3));
        assert_eq!(agents[0].tools, vec!["read_file", "search_code"]);
        assert_eq!(agents[0].prompt_template, "Review the following code.");
    }

    #[test]
    fn test_parse_multiple_agents() {
        let content = r#"# AGENT.md

Project agents.

---

## code-reviewer

Reviews code.

### Capabilities

- model: claude-sonnet-4-20250514

### Tools

- read_file

---

## test-generator

Generates tests.

### Capabilities

- max_tokens: 4096

### Tools

- read_file
- write_file
"#;
        let agents = AgentMdParser::parse(content, &test_path()).unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "code-reviewer");
        assert_eq!(agents[1].name, "test-generator");
        assert_eq!(agents[1].capabilities.max_tokens, Some(4096));
        assert_eq!(agents[1].tools, vec!["read_file", "write_file"]);
    }

    #[test]
    fn test_parse_empty_agent_md() {
        let content = "# AGENT.md\n\nNo agents here.\n";
        let agents = AgentMdParser::parse(content, &test_path()).unwrap();
        assert!(agents.is_empty());
    }

    #[test]
    fn test_parse_agent_with_parameters() {
        let content = r#"# AGENT.md

---

## reviewer

Review agent.

### Parameters

| Name | Type | Required | Default | Description |
|------|------|----------|---------|-------------|
| files | List[Str] | yes | | Files to review |
| focus | Str | no | all | Focus area |
"#;
        let agents = AgentMdParser::parse(content, &test_path()).unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].parameters.len(), 2);
        assert_eq!(agents[0].parameters[0].name, "files");
        assert_eq!(agents[0].parameters[0].type_name, "List[Str]");
        assert!(agents[0].parameters[0].required);
        assert_eq!(agents[0].parameters[0].default, None);
        assert_eq!(agents[0].parameters[1].name, "focus");
        assert!(!agents[0].parameters[1].required);
        assert_eq!(agents[0].parameters[1].default, Some("all".to_string()));
    }

    #[test]
    fn test_parse_agent_with_hooks() {
        let content = r#"# AGENT.md

---

## deployer

Deploys code.

### Hooks

```
on_start: echo "Starting deployment"
on_complete: echo "Deployment finished"
```
"#;
        let agents = AgentMdParser::parse(content, &test_path()).unwrap();
        assert_eq!(agents.len(), 1);
        assert!(agents[0].hooks.is_some());
        let hooks = agents[0].hooks.as_ref().unwrap();
        assert!(hooks.contains("on_start"));
        assert!(hooks.contains("on_complete"));
    }

    #[test]
    fn test_parse_agents_without_separator() {
        let content = r#"# AGENT.md

## first-agent

First agent description.

### Capabilities

- model: test

## second-agent

Second agent description.

### Capabilities

- model: test2
"#;
        let agents = AgentMdParser::parse(content, &test_path()).unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "first-agent");
        assert_eq!(agents[1].name, "second-agent");
    }
}
