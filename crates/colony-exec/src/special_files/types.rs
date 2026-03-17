//! Type definitions for special files system (F030)
//!
//! These types represent the parsed contents of AGENT.md, CONTEXT.md,
//! TOOLS.md, PROMPT.md, HEARTBEAT.md, and .colony/config.toml.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

/// Identifies the type of a special file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpecialFileType {
    Agent,
    Context,
    Tools,
    Prompt,
    Config,
    Heartbeat,
}

impl SpecialFileType {
    pub fn filename(&self) -> &'static str {
        match self {
            Self::Agent => "AGENT.md",
            Self::Context => "CONTEXT.md",
            Self::Tools => "TOOLS.md",
            Self::Prompt => "PROMPT.md",
            Self::Config => ".colony/config.toml",
            Self::Heartbeat => "HEARTBEAT.md",
        }
    }
    pub fn merges_up_tree(&self) -> bool {
        matches!(self, Self::Context)
    }
}

impl std::fmt::Display for SpecialFileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.filename())
    }
}

/// A single loaded special file with metadata for cache invalidation.
#[derive(Debug, Clone)]
pub struct LoadedSpecialFile {
    pub file_type: SpecialFileType,
    pub path: PathBuf,
    pub content: String,
    pub last_modified: SystemTime,
}

/// Parsed agent definition from AGENT.md
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub capabilities: AgentCapabilities,
    pub tools: Vec<String>,
    pub parameters: Vec<AgentParameter>,
    pub prompt_template: String,
    pub hooks: Option<String>,
    pub source_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentCapabilities {
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentParameter {
    pub name: String,
    pub type_name: String,
    pub required: bool,
    pub default: Option<String>,
    pub description: String,
}

/// Parsed context entry from CONTEXT.md
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub section: String,
    pub content: String,
    pub source_file: PathBuf,
}

/// Parsed tool config from TOOLS.md
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    pub description: String,
    pub schema: Option<serde_json::Value>,
    pub configuration: HashMap<String, String>,
}

/// Parsed prompt template from PROMPT.md
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub name: String,
    pub template: String,
    pub variables: Vec<String>,
}

// Re-export heartbeat types from the F080 scheduler module.
// The heartbeat types are canonical in heartbeat::types and re-exported
// here for backwards compatibility with the F030 special files system.
pub use crate::heartbeat::types::{ChecklistItem, HeartbeatConfig};

/// Project config from .colony/config.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectConfig {
    pub project: ProjectInfo,
    pub channels: ChannelsConfig,
    pub agents: AgentsConfig,
    pub tools: ToolsProjectConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ChannelsConfig {
    pub auto_load_context: Option<bool>,
    pub context_max_entries: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AgentsConfig {
    pub default_model: Option<String>,
    pub max_parallel_agents: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ToolsProjectConfig {
    pub allowed: Vec<String>,
    pub denied: Vec<String>,
    pub require_confirmation: Vec<String>,
}

/// Result of loading all special files from a directory
#[derive(Debug, Clone)]
pub struct SpecialFilesState {
    pub directory: PathBuf,
    pub agents: Vec<AgentDefinition>,
    pub context_entries: Vec<ContextEntry>,
    pub tool_configs: Vec<ToolConfig>,
    pub prompt_templates: Vec<PromptTemplate>,
    pub project_config: Option<ProjectConfig>,
    pub heartbeat_config: Option<HeartbeatConfig>,
    pub heartbeat_items: Vec<ChecklistItem>,
    pub heartbeat_path: Option<PathBuf>,
}
