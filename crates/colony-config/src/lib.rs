//! Colony Shell project configuration
//!
//! Provides .colonyrc loading with direnv-style trust model:
//! - Per-project agent personality and conventions
//! - MCP server configuration
//! - Tool classifications
//! - Security: Explicit trust required before loading

mod trust;
mod discovery;

pub use trust::{TrustManager, TrustStatus};
pub use discovery::{discover_config, ConfigLocation};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Colony Shell configuration loaded from .colonyrc
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ColonyConfig {
    /// Configuration format version
    pub version: u32,

    /// Agent configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentConfig>,

    /// Project metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectConfig>,

    /// MCP server configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpConfig>,

    /// Tool configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsConfig>,

    /// Completion configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<CompletionConfig>,
}

/// Agent-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Custom system prompt / agent personality for this project
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,

    /// Preferred LLM provider for this project
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_provider: Option<String>,

    /// Preferred model name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_model: Option<String>,

    /// Temperature override (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Max tokens override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            personality: None,
            preferred_provider: None,
            preferred_model: None,
            temperature: None,
            max_tokens: None,
        }
    }
}

/// Project metadata and conventions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectConfig {
    /// Project name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Project description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Primary programming language
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Project goals (shown to agent for context)
    #[serde(default)]
    pub goals: Vec<String>,

    /// Coding conventions (shown to agent)
    #[serde(default)]
    pub conventions: Vec<String>,

    /// Documentation links
    #[serde(default)]
    pub docs: Vec<String>,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct McpConfig {
    /// MCP servers to auto-connect on shell start
    #[serde(default)]
    pub servers: Vec<McpServer>,
}

/// MCP server definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    /// Server name (for reference)
    pub name: String,

    /// Command to execute
    pub command: String,

    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,

    /// Auto-start on shell initialization
    #[serde(default = "default_true")]
    pub auto_start: bool,
}

fn default_true() -> bool {
    true
}

/// Tool configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ToolsConfig {
    /// Tool classification overrides
    /// Maps tool name → "programmatic" or "user_facing"
    #[serde(default)]
    pub classifications: std::collections::HashMap<String, String>,
}

/// Completion configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompletionConfig {
    /// Suggest commands from PATH
    pub suggest_commands: bool,

    /// Suggest environment variables
    pub suggest_variables: bool,

    /// Suggest file paths
    pub suggest_paths: bool,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            suggest_commands: true,
            suggest_variables: true,
            suggest_paths: true,
        }
    }
}

impl ColonyConfig {
    /// Load configuration from TOML string
    pub fn from_toml(content: &str) -> Result<Self> {
        toml::from_str(content).map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    /// Serialize to TOML string
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::SerializeError(e.to_string()))
    }

    /// Load from file path
    pub fn from_file(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let content = std::fs::read_to_string(&path).map_err(|e| {
            ConfigError::IoError(format!("failed to read {}: {}", path.display(), e))
        })?;

        Self::from_toml(&content)
    }

    /// Merge another config into this one
    ///
    /// Used for layering: system config + project config
    /// Project config values override system config
    pub fn merge(&mut self, other: ColonyConfig) {
        // Merge agent config
        if let Some(other_agent) = other.agent {
            let agent = self.agent.get_or_insert(AgentConfig::default());

            if other_agent.personality.is_some() {
                agent.personality = other_agent.personality;
            }
            if other_agent.preferred_provider.is_some() {
                agent.preferred_provider = other_agent.preferred_provider;
            }
            if other_agent.preferred_model.is_some() {
                agent.preferred_model = other_agent.preferred_model;
            }
            if other_agent.temperature.is_some() {
                agent.temperature = other_agent.temperature;
            }
            if other_agent.max_tokens.is_some() {
                agent.max_tokens = other_agent.max_tokens;
            }
        }

        // Merge project config
        if other.project.is_some() {
            self.project = other.project;
        }

        // Merge MCP config
        if let Some(other_mcp) = other.mcp {
            let mcp = self.mcp.get_or_insert(McpConfig::default());
            mcp.servers.extend(other_mcp.servers);
        }

        // Merge tools config
        if let Some(other_tools) = other.tools {
            let tools = self.tools.get_or_insert(ToolsConfig::default());
            tools.classifications.extend(other_tools.classifications);
        }

        // Merge completion config
        if other.completion.is_some() {
            self.completion = other.completion;
        }
    }
}

/// Configuration error types
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to parse config: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("serialize error: {0}")]
    SerializeError(String),

    #[error("untrusted config: {0}")]
    Untrusted(String),

    #[error("trust error: {0}")]
    TrustError(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
version = 1
"#;
        let config = ColonyConfig::from_toml(toml).unwrap();
        assert_eq!(config.version, 1);
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
version = 1

[agent]
personality = "You are a Rust expert."
preferred_provider = "anthropic"
preferred_model = "claude-3-5-sonnet-20241022"
temperature = 0.7

[project]
name = "my-project"
description = "A cool project"
language = "rust"
goals = ["Build fast", "Stay safe"]
conventions = ["Use parking_lot", "No unwrap()"]
"#;

        let config = ColonyConfig::from_toml(toml).unwrap();
        assert_eq!(config.version, 1);
        assert!(config.agent.is_some());

        let agent = config.agent.unwrap();
        assert_eq!(agent.personality, Some("You are a Rust expert.".to_string()));
        assert_eq!(agent.preferred_provider, Some("anthropic".to_string()));
        assert_eq!(agent.temperature, Some(0.7));

        assert!(config.project.is_some());
        let project = config.project.unwrap();
        assert_eq!(project.name, Some("my-project".to_string()));
        assert_eq!(project.goals.len(), 2);
    }

    #[test]
    fn test_config_merge() {
        let mut system = ColonyConfig {
            version: 1,
            agent: Some(AgentConfig {
                personality: Some("System default".to_string()),
                preferred_provider: Some("anthropic".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let project = ColonyConfig {
            version: 1,
            agent: Some(AgentConfig {
                personality: Some("Project override".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        system.merge(project);

        let agent = system.agent.unwrap();
        assert_eq!(agent.personality, Some("Project override".to_string()));
        assert_eq!(agent.preferred_provider, Some("anthropic".to_string()));
    }
}
