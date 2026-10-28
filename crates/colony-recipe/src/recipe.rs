//! Recipe type definitions and parsing
//!
//! A Recipe is a declarative TOML configuration that defines an LLM "mode",
//! combining system prompts, tools, skills, and behaviors into a switchable profile.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::{RecipeError, Result};

fn default_version() -> String {
    "0.1.0".to_string()
}

/// Complete recipe configuration parsed from recipe.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    #[serde(rename = "recipe")]
    pub meta: RecipeMeta,

    #[serde(default)]
    pub prompt: PromptConfig,

    #[serde(default)]
    pub tools: ToolsConfig,

    #[serde(default)]
    pub skills: HashMap<String, SkillDef>,

    #[serde(default)]
    pub commands: Vec<CommandDef>,

    #[serde(default)]
    pub model: ModelConfig,

    #[serde(default)]
    pub behavior: BehaviorConfig,
}

impl Recipe {
    /// Convenience accessor for imports from the recipe meta section.
    pub fn imports(&self) -> &[String] {
        &self.meta.imports
    }
}

/// Recipe metadata (the \[recipe\] section in TOML)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeMeta {
    pub name: String,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default = "default_version")]
    pub version: String,

    #[serde(default)]
    pub author: Option<String>,

    #[serde(default)]
    pub imports: Vec<String>,
}

/// Prompt configuration section
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptConfig {
    #[serde(default)]
    pub system: Option<String>,

    #[serde(default)]
    pub context_files: Vec<String>,
}

/// Tools and MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolsConfig {
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,

    #[serde(default)]
    pub approval: ToolApprovalConfig,
}

/// MCP server definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Tool approval policies
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolApprovalConfig {
    #[serde(default)]
    pub auto_approve: Vec<String>,
    #[serde(default)]
    pub require_approval: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Inline skill definition within a recipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
}

/// Command definition (slash commands)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub action: CommandAction,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
}

/// Command action type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandAction {
    Shell,
    Agent,
    Skill,
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelConfig {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
}

/// Behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BehaviorConfig {
    #[serde(default)]
    pub streaming: Option<bool>,
    #[serde(default)]
    pub auto_continue: Option<bool>,
    #[serde(default)]
    pub max_turns: Option<u32>,
}

impl Recipe {
    /// Parse a recipe from a TOML string.
    pub fn from_toml(content: &str, path: &Path) -> Result<Self> {
        toml::from_str(content).map_err(|e| RecipeError::TomlParse {
            path: path.display().to_string(),
            source: e,
        })
    }

    /// Load a recipe from a recipe.toml file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| RecipeError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::from_toml(&content, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_minimal_recipe() {
        let toml = r#"
[recipe]
name = "test-recipe"

[prompt]
system = "You are a test assistant."
"#;
        let recipe = Recipe::from_toml(toml, &PathBuf::from("test.toml")).unwrap();
        assert_eq!(recipe.meta.name, "test-recipe");
        assert_eq!(recipe.meta.version, "0.1.0");
        assert_eq!(
            recipe.prompt.system.as_deref(),
            Some("You are a test assistant.")
        );
    }

    #[test]
    fn test_parse_full_recipe() {
        let toml = r#"
[recipe]
name = "coding-agent"
description = "Full-featured coding assistant"
version = "1.0.0"
author = "colony"
imports = ["official/bundles/file-ops"]

[prompt]
system = "You are an expert software engineer."
context_files = ["CLAUDE.md", "AGENTS.md"]

[tools]
mcp_servers = [
    { name = "filesystem", command = "npx", args = ["-y", "@anthropic/mcp-filesystem", "."] },
]

[tools.approval]
auto_approve = ["read_file"]
deny = ["rm_rf"]

[skills.code-review]
trigger = "/review"
description = "Review code for issues"
prompt = "Review the following code."

[[commands]]
name = "test"
description = "Run project tests"
action = "shell"
command = "cargo test"

[model]
provider = "colony"
name = "claude-3.5-sonnet"
temperature = 0.7

[behavior]
streaming = true
auto_continue = true
max_turns = 50
"#;
        let recipe = Recipe::from_toml(toml, &PathBuf::from("test.toml")).unwrap();
        assert_eq!(recipe.meta.name, "coding-agent");
        assert_eq!(recipe.meta.version, "1.0.0");
        assert_eq!(recipe.imports().len(), 1);
        assert_eq!(recipe.tools.mcp_servers.len(), 1);
        assert_eq!(recipe.tools.approval.deny, vec!["rm_rf"]);
        assert!(recipe.skills.contains_key("code-review"));
        assert_eq!(recipe.commands.len(), 1);
        assert_eq!(recipe.model.temperature, Some(0.7));
        assert_eq!(recipe.behavior.max_turns, Some(50));
    }

    #[test]
    fn test_parse_invalid_toml() {
        let toml = "this is not valid toml {{{";
        let result = Recipe::from_toml(toml, &PathBuf::from("bad.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("TOML parse error"));
    }
}
