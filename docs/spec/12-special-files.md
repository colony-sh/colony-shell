# Part 12: Special Files

## 12.1 Overview

Colony recognizes special files in the project directory that configure behavior:

| File | Purpose |
|------|---------|
| `AGENT.md` | Define project-specific agents |
| `CONTEXT.md` | Seed session context when entering directory |
| `TOOLS.md` | Define and configure tools |
| `PROMPT.md` | Reusable prompt templates |
| `.colony/` | Project configuration directory |

```
project/
├── AGENT.md           # Agent definitions
├── CONTEXT.md         # Context seeding
├── TOOLS.md           # Tool configuration
├── PROMPT.md          # Prompt templates
└── .colony/
    ├── config.toml    # Project settings
    ├── sessions/      # Saved sessions
    └── cache/         # Cache data
```

## 12.2 AGENT.md Format

```markdown
# AGENT.md

Define agents in Markdown format that is both human-readable documentation
and machine-parseable configuration.

---

## code-reviewer

Reviews code for bugs, style issues, and improvements.

### Description

A thorough code reviewer that examines code for potential issues,
suggests improvements, and ensures best practices are followed.

### Parameters

| Name | Type | Required | Default | Description |
|------|------|----------|---------|-------------|
| file | Str | yes | | File path to review |
| focus | Str | no | null | Specific areas to focus on |
| style_guide | Str | no | "default" | Style guide to follow |

### Output

Returns a structured review with:
- `issues`: List of found issues
- `suggestions`: Improvement suggestions  
- `summary`: Brief summary

### Tools

- read_file
- search
- git:diff

### Capabilities

- model: claude-sonnet-4-20250514
- max_tokens: 4096
- temperature: 0.3
- context: read

### Prompt

```
You are an expert code reviewer. Analyze the following code for:

1. **Bugs and Issues**
   - Logic errors
   - Edge cases
   - Null pointer issues
   - Resource leaks

2. **Code Quality**
   - Readability
   - Maintainability
   - DRY violations
   - Naming conventions

3. **Performance**
   - Inefficient algorithms
   - Unnecessary allocations
   - N+1 queries

File: {file}
Focus: {focus}
Style Guide: {style_guide}

Session Context:
{context}

Provide your review in the following JSON format:
{
  "issues": [...],
  "suggestions": [...],
  "summary": "..."
}
```

### Hooks

```colony
on_start {
    context "Starting code review for {file}"
}

on_complete(result) {
    context --type decision "Review complete: {result.summary}"
    if $[len(result.issues) > 0] {
        context --type action "Found {len(result.issues)} issues"
    }
}
```

---

## test-generator

Generates unit tests for code.

### Description

Analyzes code and generates comprehensive unit tests with good coverage.

### Parameters

| Name | Type | Required | Default | Description |
|------|------|----------|---------|-------------|
| file | Str | yes | | File to generate tests for |
| framework | Str | no | "auto" | Test framework to use |
| coverage | Float | no | 0.8 | Target coverage |

### Tools

- read_file
- write_file
- shell

### Prompt

```
Generate comprehensive unit tests for the following code.

File: {file}
Framework: {framework}
Target Coverage: {coverage}

Requirements:
- Cover all public functions
- Include edge cases
- Test error conditions
- Use descriptive test names
```
```

## 12.3 AGENT.md Parser

```rust
// crates/colony-agents/src/parser.rs

/// Parses AGENT.md format
pub struct AgentMdParser;

impl AgentMdParser {
    pub fn parse(content: &str) -> Result<Vec<AgentDef>, ParseError> {
        let mut agents = Vec::new();
        let mut lines = content.lines().peekable();
        
        while let Some(line) = lines.next() {
            // Look for agent header (## agent-name)
            if line.starts_with("## ") {
                let name = line[3..].trim().to_string();
                let agent = Self::parse_agent(&name, &mut lines)?;
                agents.push(agent);
            }
        }
        
        Ok(agents)
    }
    
    fn parse_agent(
        name: &str,
        lines: &mut std::iter::Peekable<std::str::Lines>,
    ) -> Result<AgentDef, ParseError> {
        let mut builder = AgentBuilder::new(name);
        let mut current_section = Section::None;
        let mut in_code_block = false;
        let mut code_content = String::new();
        let mut code_lang = String::new();
        let mut table_rows: Vec<Vec<String>> = Vec::new();
        let mut in_table = false;
        
        while let Some(line) = lines.peek() {
            // Stop at next agent
            if line.starts_with("## ") {
                break;
            }
            
            let line = lines.next().unwrap();
            
            // Code block handling
            if line.starts_with("```") {
                if in_code_block {
                    // End of code block
                    in_code_block = false;
                    match current_section {
                        Section::Prompt => builder.prompt = code_content.clone(),
                        Section::Hooks => builder.hooks_code = Some(code_content.clone()),
                        _ => {}
                    }
                    code_content.clear();
                } else {
                    // Start of code block
                    in_code_block = true;
                    code_lang = line[3..].trim().to_string();
                }
                continue;
            }
            
            if in_code_block {
                code_content.push_str(line);
                code_content.push('\n');
                continue;
            }
            
            // Table handling
            if line.starts_with("|") && line.ends_with("|") {
                if !in_table {
                    in_table = true;
                    table_rows.clear();
                }
                
                // Skip separator line
                if line.contains("---") {
                    continue;
                }
                
                let cells: Vec<String> = line[1..line.len()-1]
                    .split('|')
                    .map(|s| s.trim().to_string())
                    .collect();
                table_rows.push(cells);
                continue;
            } else if in_table {
                // End of table
                in_table = false;
                match current_section {
                    Section::Parameters => {
                        builder.params = Self::parse_param_table(&table_rows)?;
                    }
                    Section::Tools => {
                        // Tools might be in a table or list
                    }
                    _ => {}
                }
                table_rows.clear();
            }
            
            // Section headers (### Section)
            if line.starts_with("### ") {
                let section_name = line[4..].trim().to_lowercase();
                current_section = match section_name.as_str() {
                    "description" => Section::Description,
                    "parameters" | "params" | "inputs" => Section::Parameters,
                    "output" | "returns" => Section::Output,
                    "tools" => Section::Tools,
                    "capabilities" => Section::Capabilities,
                    "prompt" => Section::Prompt,
                    "hooks" => Section::Hooks,
                    _ => Section::None,
                };
                continue;
            }
            
            // Content parsing based on section
            match current_section {
                Section::Description => {
                    if !line.is_empty() {
                        if builder.description.is_some() {
                            builder.description.as_mut().unwrap().push_str(" ");
                            builder.description.as_mut().unwrap().push_str(line.trim());
                        } else {
                            builder.description = Some(line.trim().to_string());
                        }
                    }
                }
                Section::Tools => {
                    // Parse tool list (- tool_name format)
                    if line.trim().starts_with("- ") {
                        let tool = line.trim()[2..].trim().to_string();
                        builder.tools.push(tool);
                    }
                }
                Section::Capabilities => {
                    // Parse key: value format
                    if let Some((key, value)) = line.split_once(':') {
                        let key = key.trim().to_lowercase();
                        let value = value.trim();
                        
                        match key.as_str() {
                            "model" => builder.model = Some(value.to_string()),
                            "max_tokens" => builder.max_tokens = value.parse().ok(),
                            "temperature" => builder.temperature = value.parse().ok(),
                            "context" => builder.context_read = value.contains("read"),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        
        builder.build()
    }
    
    fn parse_param_table(rows: &[Vec<String>]) -> Result<Vec<AgentParam>, ParseError> {
        // First row is header
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        
        let header = &rows[0];
        let name_idx = header.iter().position(|h| h.to_lowercase() == "name").unwrap_or(0);
        let type_idx = header.iter().position(|h| h.to_lowercase() == "type").unwrap_or(1);
        let req_idx = header.iter().position(|h| h.to_lowercase() == "required").unwrap_or(2);
        let def_idx = header.iter().position(|h| h.to_lowercase() == "default").unwrap_or(3);
        let desc_idx = header.iter().position(|h| h.to_lowercase() == "description").unwrap_or(4);
        
        let mut params = Vec::new();
        
        for row in rows.iter().skip(1) {  // Skip header
            if row.len() <= name_idx {
                continue;
            }
            
            let name = row.get(name_idx).cloned().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            
            let type_str = row.get(type_idx).cloned().unwrap_or_default();
            let required = row.get(req_idx)
                .map(|s| s.to_lowercase() == "yes" || s.to_lowercase() == "true")
                .unwrap_or(false);
            let default = row.get(def_idx).cloned()
                .filter(|s| !s.is_empty() && s != "null");
            let description = row.get(desc_idx).cloned();
            
            params.push(AgentParam {
                name,
                type_annotation: TypeAnnotation::parse(&type_str).ok(),
                default: default.map(|s| Value::Str(s)),
                description,
                required,
            });
        }
        
        Ok(params)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    None,
    Description,
    Parameters,
    Output,
    Tools,
    Capabilities,
    Prompt,
    Hooks,
}

struct AgentBuilder {
    name: String,
    description: Option<String>,
    params: Vec<AgentParam>,
    tools: Vec<String>,
    model: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    context_read: bool,
    prompt: String,
    hooks_code: Option<String>,
}

impl AgentBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            params: Vec::new(),
            tools: Vec::new(),
            model: None,
            max_tokens: None,
            temperature: None,
            context_read: false,
            prompt: String::new(),
            hooks_code: None,
        }
    }
    
    fn build(self) -> Result<AgentDef, ParseError> {
        Ok(AgentDef {
            name: self.name,
            description: self.description,
            version: None,
            capabilities: AgentCapabilities {
                tools: self.tools.into_iter()
                    .map(|t| ToolSpec {
                        pattern: t,
                        description: None,
                        requires_confirmation: false,
                    })
                    .collect(),
                mcp_servers: Vec::new(),
                max_tokens: self.max_tokens,
                temperature: self.temperature,
                model: self.model,
                context: ContextCapabilities {
                    read: self.context_read,
                    write: false,
                    types: Vec::new(),
                },
                max_iterations: None,
                timeout_secs: None,
            },
            input_params: self.params,
            output_type: None,
            prompt: PromptTemplate::Literal(self.prompt),
            hooks: AgentHooks::default(),
            source: None,
        })
    }
}
```

## 12.4 CONTEXT.md Format

```markdown
# CONTEXT.md

This file provides initial context when entering this project directory.
The content is automatically loaded into the context channel.

---

## Project Overview

This is the Colony shell project - an agent-first shell with POSIX compatibility.

## Architecture

- `crates/colony-core`: Core types and configuration
- `crates/colony-parser`: Lexer and parser
- `crates/colony-runtime`: Execution engine
- `crates/colony-agents`: Agent framework

## Current Focus

- Implementing the tuple space for coordination
- Building out the MCP integration
- Refining the channel system

## Conventions

- Use Rust 2021 edition
- Follow standard Rust naming conventions
- All public APIs should have documentation
- Tests go in `tests/` directory

## Important Files

- `docs/spec/`: Implementation specification
- `AGENT.md`: Project agents
- `.colony/config.toml`: Project configuration

## Team Context

- Lead: Jesse
- Focus: Agent-native development workflows
```

## 12.5 Context Loader

```rust
// crates/colony-runtime/src/context_loader.rs

use std::path::Path;

pub struct ContextLoader;

impl ContextLoader {
    /// Load CONTEXT.md from directory
    pub fn load_from_dir(dir: &Path) -> Option<Vec<ContextEntry>> {
        let context_file = dir.join("CONTEXT.md");
        
        if !context_file.exists() {
            return None;
        }
        
        let content = std::fs::read_to_string(&context_file).ok()?;
        Some(Self::parse_context_md(&content, dir))
    }
    
    fn parse_context_md(content: &str, dir: &Path) -> Vec<ContextEntry> {
        let mut entries = Vec::new();
        let mut current_section = String::new();
        let mut section_content = String::new();
        
        for line in content.lines() {
            if line.starts_with("## ") {
                // Save previous section
                if !current_section.is_empty() && !section_content.trim().is_empty() {
                    entries.push(ContextEntry {
                        id: generate_id("ctx"),
                        timestamp: Utc::now(),
                        entry_type: ContextType::Seed,
                        source: ContextSource {
                            file: Some(dir.join("CONTEXT.md").to_string_lossy().to_string()),
                            ..Default::default()
                        },
                        content: format!("{}: {}", current_section, section_content.trim()),
                        metadata: IndexMap::new(),
                        session_id: String::new(),
                    });
                }
                
                current_section = line[3..].trim().to_string();
                section_content.clear();
            } else if !line.starts_with('#') && !line.trim().is_empty() && line != "---" {
                section_content.push_str(line);
                section_content.push('\n');
            }
        }
        
        // Save last section
        if !current_section.is_empty() && !section_content.trim().is_empty() {
            entries.push(ContextEntry {
                id: generate_id("ctx"),
                timestamp: Utc::now(),
                entry_type: ContextType::Seed,
                source: ContextSource {
                    file: Some(dir.join("CONTEXT.md").to_string_lossy().to_string()),
                    ..Default::default()
                },
                content: format!("{}: {}", current_section, section_content.trim()),
                metadata: IndexMap::new(),
                session_id: String::new(),
            });
        }
        
        entries
    }
}
```

## 12.6 TOOLS.md Format

```markdown
# TOOLS.md

Define and configure tools available in this project.

---

## shell

Execute shell commands.

### Schema

```json
{
  "type": "object",
  "properties": {
    "command": {
      "type": "string",
      "description": "Command to execute"
    },
    "cwd": {
      "type": "string",
      "description": "Working directory"
    }
  },
  "required": ["command"]
}
```

### Configuration

- timeout: 30s
- allowed_commands: [ls, cat, grep, find, git]
- denied_commands: [rm, sudo, chmod]

---

## database-query

Query the project database.

### Schema

```json
{
  "type": "object",
  "properties": {
    "query": {
      "type": "string",
      "description": "SQL query"
    },
    "params": {
      "type": "array",
      "description": "Query parameters"
    }
  },
  "required": ["query"]
}
```

### Configuration

- connection: postgres://localhost/mydb
- read_only: true
- max_rows: 1000

---

## External Tools

### MCP Servers

| Server | Command | Purpose |
|--------|---------|---------|
| filesystem | npx @anthropic/mcp-filesystem | File operations |
| git | mcp-git-server | Git operations |

### HTTP APIs

| Name | URL | Auth |
|------|-----|------|
| jira | https://company.atlassian.net/rest/api | Bearer token |
| github | https://api.github.com | PAT |
```

## 12.7 PROMPT.md Format

```markdown
# PROMPT.md

Reusable prompt templates for this project.

---

## code-review-template

```
You are reviewing code in the {project} project.

## Guidelines

1. Follow the style guide at {style_guide_url}
2. Check for common issues:
   - Memory leaks
   - SQL injection
   - XSS vulnerabilities

## Context

{context}

## Code to Review

{code}
```

---

## commit-message-template

```
Generate a commit message for these changes:

{diff}

Follow conventional commits format:
- feat: new feature
- fix: bug fix
- docs: documentation
- style: formatting
- refactor: code restructure
- test: adding tests
- chore: maintenance
```

---

## documentation-template

```
Generate documentation for the following code.

Language: {language}
Style: {doc_style}

Code:
{code}

Include:
- Brief description
- Parameters/arguments
- Return value
- Examples
- Edge cases
```
```

## 12.8 Prompt Template Loader

```rust
// crates/colony-agents/src/prompts.rs

use std::path::Path;

/// Manages prompt templates
pub struct PromptManager {
    templates: IndexMap<String, PromptTemplate>,
}

impl PromptManager {
    pub fn new() -> Self {
        Self {
            templates: IndexMap::new(),
        }
    }
    
    /// Load templates from PROMPT.md
    pub fn load_from_file(&mut self, path: &Path) -> Result<(), io::Error> {
        let content = std::fs::read_to_string(path)?;
        self.parse_prompt_md(&content);
        Ok(())
    }
    
    fn parse_prompt_md(&mut self, content: &str) {
        let mut current_name = String::new();
        let mut in_code_block = false;
        let mut template_content = String::new();
        
        for line in content.lines() {
            if line.starts_with("## ") && !in_code_block {
                // Save previous template
                if !current_name.is_empty() && !template_content.is_empty() {
                    self.templates.insert(
                        current_name.clone(),
                        PromptTemplate::Template {
                            template: template_content.trim().to_string(),
                            variables: Self::extract_variables(&template_content),
                        },
                    );
                }
                
                current_name = line[3..].trim().to_string();
                template_content.clear();
            } else if line.starts_with("```") {
                in_code_block = !in_code_block;
            } else if in_code_block {
                template_content.push_str(line);
                template_content.push('\n');
            }
        }
        
        // Save last template
        if !current_name.is_empty() && !template_content.is_empty() {
            self.templates.insert(
                current_name,
                PromptTemplate::Template {
                    template: template_content.trim().to_string(),
                    variables: Self::extract_variables(&template_content),
                },
            );
        }
    }
    
    fn extract_variables(template: &str) -> Vec<String> {
        let mut vars = Vec::new();
        let mut chars = template.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '{' {
                let mut var = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '}' {
                        chars.next();
                        break;
                    }
                    var.push(chars.next().unwrap());
                }
                if !var.is_empty() && !vars.contains(&var) {
                    vars.push(var);
                }
            }
        }
        
        vars
    }
    
    /// Get a template by name
    pub fn get(&self, name: &str) -> Option<&PromptTemplate> {
        self.templates.get(name)
    }
    
    /// Render a template with values
    pub fn render(&self, name: &str, values: &IndexMap<String, Value>) -> Result<String, PromptError> {
        let template = self.get(name)
            .ok_or_else(|| PromptError::TemplateNotFound(name.to_string()))?;
        
        template.render(values)
    }
    
    /// List available templates
    pub fn list(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }
}
```

## 12.9 .colony/ Directory

```
.colony/
├── config.toml          # Project configuration
├── sessions/            # Saved session data
│   ├── session-abc.json
│   └── session-def.json
├── cache/               # Cache data
│   ├── tools.json       # Tool cache
│   └── mcp.json         # MCP server cache
└── history/             # Project-specific history
    └── .colony_history
```

### config.toml

```toml
# .colony/config.toml

[project]
name = "colony"
version = "0.1.0"

[shell]
# Override shell options for this project
options = ["colony:all", "strict:all"]

[channels]
# Context settings
context_max_entries = 500
# Auto-load CONTEXT.md
auto_load_context = true

[agents]
# Default agent settings
default_model = "claude-sonnet-4-20250514"
max_parallel_agents = 2

[mcp]
# Auto-connect these servers
auto_connect = ["filesystem", "git"]

[tools]
# Tool restrictions
allowed = ["*"]
denied = ["rm -rf", "sudo"]
require_confirmation = ["write_file", "shell"]

[history]
# Use project-specific history
project_history = true
max_entries = 5000
```

## 12.10 Special Files Loader

```rust
// crates/colony-runtime/src/special_files.rs

use std::path::Path;

/// Loads and manages special files
pub struct SpecialFilesLoader {
    /// Loaded agents from AGENT.md
    pub agents: Vec<AgentDef>,
    /// Loaded context from CONTEXT.md  
    pub context_entries: Vec<ContextEntry>,
    /// Loaded tools from TOOLS.md
    pub tool_configs: Vec<ToolConfig>,
    /// Loaded prompts from PROMPT.md
    pub prompts: PromptManager,
    /// Project config from .colony/config.toml
    pub config: ProjectConfig,
}

impl SpecialFilesLoader {
    /// Load all special files from a directory
    pub fn load(dir: &Path) -> Result<Self, LoadError> {
        let mut loader = Self {
            agents: Vec::new(),
            context_entries: Vec::new(),
            tool_configs: Vec::new(),
            prompts: PromptManager::new(),
            config: ProjectConfig::default(),
        };
        
        // Load AGENT.md
        let agent_path = dir.join("AGENT.md");
        if agent_path.exists() {
            let content = std::fs::read_to_string(&agent_path)?;
            loader.agents = AgentMdParser::parse(&content)?;
        }
        
        // Load CONTEXT.md
        if let Some(entries) = ContextLoader::load_from_dir(dir) {
            loader.context_entries = entries;
        }
        
        // Load TOOLS.md
        let tools_path = dir.join("TOOLS.md");
        if tools_path.exists() {
            let content = std::fs::read_to_string(&tools_path)?;
            loader.tool_configs = ToolsParser::parse(&content)?;
        }
        
        // Load PROMPT.md
        let prompt_path = dir.join("PROMPT.md");
        if prompt_path.exists() {
            loader.prompts.load_from_file(&prompt_path)?;
        }
        
        // Load .colony/config.toml
        let config_path = dir.join(".colony/config.toml");
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            loader.config = toml::from_str(&content)?;
        }
        
        Ok(loader)
    }
    
    /// Check if directory has special files
    pub fn has_special_files(dir: &Path) -> bool {
        dir.join("AGENT.md").exists()
            || dir.join("CONTEXT.md").exists()
            || dir.join("TOOLS.md").exists()
            || dir.join("PROMPT.md").exists()
            || dir.join(".colony").exists()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProjectConfig {
    pub project: ProjectInfo,
    pub shell: ShellConfig,
    pub channels: ChannelsConfig,
    pub agents: AgentsConfig,
    pub mcp: McpConfig,
    pub tools: ToolsConfig,
    pub history: HistoryConfig,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProjectInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ShellConfig {
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ChannelsConfig {
    pub context_max_entries: Option<usize>,
    pub auto_load_context: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AgentsConfig {
    pub default_model: Option<String>,
    pub max_parallel_agents: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ToolsConfig {
    pub allowed: Vec<String>,
    pub denied: Vec<String>,
    pub require_confirmation: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HistoryConfig {
    pub project_history: Option<bool>,
    pub max_entries: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
}
```

## 12.11 Directory Change Hook

```rust
// Integration with shell - on cd

impl Shell {
    /// Called when changing directories
    pub fn on_directory_change(&mut self, new_dir: &Path) -> Result<(), ShellError> {
        // Check for special files
        if SpecialFilesLoader::has_special_files(new_dir) {
            let loader = SpecialFilesLoader::load(new_dir)?;
            
            // Register agents
            for agent in loader.agents {
                self.agent_registry.register(agent);
            }
            
            // Load context
            if loader.config.channels.auto_load_context.unwrap_or(true) {
                let channels = self.channels.write().unwrap();
                if let Some(ctx) = channels.context_mut() {
                    for entry in loader.context_entries {
                        ctx.add(entry);
                    }
                }
            }
            
            // Apply shell options
            for opt in &loader.config.shell.options {
                self.options.apply_group(opt)?;
            }
            
            // Store project config
            self.project_config = Some(loader.config);
        }
        
        Ok(())
    }
}
```

---

*Continue to Part 13: Interactive Features*