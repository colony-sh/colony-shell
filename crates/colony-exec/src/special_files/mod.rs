//! Special files system (F030)
//!
//! Loads and parses AGENT.md, CONTEXT.md, TOOLS.md, PROMPT.md, HEARTBEAT.md,
//! and .colony/config.toml from a directory. Triggered on directory changes
//! (cd, pushd, popd) via the tuple space.

mod agent_md;
mod context_md;
pub mod heartbeat;
mod prompt_md;
mod tools_md;
pub mod types;

use std::path::Path;

pub use agent_md::AgentMdParser;
pub use context_md::ContextMdParser;
pub use heartbeat::{HeartbeatParser, HeartbeatWatcher};
pub use prompt_md::PromptMdParser;
pub use tools_md::ToolsMdParser;
pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum SpecialFilesError {
    #[error("I/O error reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },

    #[error("parse error in {file}: {message}")]
    Parse { file: String, message: String },

    #[error("TOML parse error in {path}: {source}")]
    Toml {
        path: String,
        source: toml::de::Error,
    },
}

/// Central loader for all special files in a directory.
pub struct SpecialFilesLoader;

impl SpecialFilesLoader {
    /// Load all special files from a directory.
    ///
    /// Called on directory change (cd, pushd, popd). Individual file failures
    /// are logged but do not prevent loading other files.
    pub fn load(dir: &Path) -> Result<SpecialFilesState, SpecialFilesError> {
        let mut state = SpecialFilesState {
            directory: dir.to_path_buf(),
            agents: Vec::new(),
            context_entries: Vec::new(),
            tool_configs: Vec::new(),
            prompt_templates: Vec::new(),
            project_config: None,
            heartbeat_entries: Vec::new(),
        };

        if let Ok(agents) = Self::try_load_agent_md(dir) {
            state.agents = agents;
        }

        if let Ok(context) = Self::try_load_context_md(dir) {
            state.context_entries = context;
        }

        if let Ok(tools) = Self::try_load_tools_md(dir) {
            state.tool_configs = tools;
        }

        if let Ok(prompts) = Self::try_load_prompt_md(dir) {
            state.prompt_templates = prompts;
        }

        if let Ok(config) = Self::try_load_colony_config(dir) {
            state.project_config = Some(config);
        }

        if let Ok(heartbeat) = Self::try_load_heartbeat_md(dir) {
            state.heartbeat_entries = heartbeat;
        }

        Ok(state)
    }

    /// Check whether a directory contains any special files.
    pub fn has_special_files(dir: &Path) -> bool {
        dir.join("AGENT.md").exists()
            || dir.join("CONTEXT.md").exists()
            || dir.join("TOOLS.md").exists()
            || dir.join("PROMPT.md").exists()
            || dir.join(".colony").exists()
            || dir.join("HEARTBEAT.md").exists()
    }

    fn try_load_agent_md(dir: &Path) -> Result<Vec<types::AgentDefinition>, SpecialFilesError> {
        let path = dir.join("AGENT.md");
        let content = read_file_if_exists(&path)?;
        let Some(content) = content else {
            return Ok(Vec::new());
        };
        AgentMdParser::parse(&content, &path).map_err(|e| SpecialFilesError::Parse {
            file: path.display().to_string(),
            message: e.to_string(),
        })
    }

    fn try_load_context_md(dir: &Path) -> Result<Vec<types::ContextEntry>, SpecialFilesError> {
        let path = dir.join("CONTEXT.md");
        let content = read_file_if_exists(&path)?;
        let Some(content) = content else {
            return Ok(Vec::new());
        };
        Ok(ContextMdParser::parse(&content, &path))
    }

    fn try_load_tools_md(dir: &Path) -> Result<Vec<types::ToolConfig>, SpecialFilesError> {
        let path = dir.join("TOOLS.md");
        let content = read_file_if_exists(&path)?;
        let Some(content) = content else {
            return Ok(Vec::new());
        };
        Ok(ToolsMdParser::parse(&content, &path))
    }

    fn try_load_prompt_md(dir: &Path) -> Result<Vec<types::PromptTemplate>, SpecialFilesError> {
        let path = dir.join("PROMPT.md");
        let content = read_file_if_exists(&path)?;
        let Some(content) = content else {
            return Ok(Vec::new());
        };
        Ok(PromptMdParser::parse(&content, &path))
    }

    fn try_load_colony_config(dir: &Path) -> Result<types::ProjectConfig, SpecialFilesError> {
        let path = dir.join(".colony/config.toml");
        let content = read_file_if_exists(&path)?;
        let Some(content) = content else {
            return Err(SpecialFilesError::Io {
                path: path.display().to_string(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
            });
        };
        toml::from_str(&content).map_err(|e| SpecialFilesError::Toml {
            path: path.display().to_string(),
            source: e,
        })
    }

    fn try_load_heartbeat_md(dir: &Path) -> Result<Vec<types::HeartbeatEntry>, SpecialFilesError> {
        let path = dir.join("HEARTBEAT.md");
        let content = read_file_if_exists(&path)?;
        let Some(content) = content else {
            return Ok(Vec::new());
        };
        Ok(HeartbeatParser::parse(&content))
    }
}

fn read_file_if_exists(path: &Path) -> Result<Option<String>, SpecialFilesError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(SpecialFilesError::Io {
            path: path.display().to_string(),
            source: e,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_special_files_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!SpecialFilesLoader::has_special_files(dir.path()));
    }

    #[test]
    fn test_has_special_files_with_agent_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AGENT.md"), "# AGENT.md\n").unwrap();
        assert!(SpecialFilesLoader::has_special_files(dir.path()));
    }

    #[test]
    fn test_has_special_files_with_colony_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".colony")).unwrap();
        assert!(SpecialFilesLoader::has_special_files(dir.path()));
    }

    #[test]
    fn test_load_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let state = SpecialFilesLoader::load(dir.path()).unwrap();
        assert!(state.agents.is_empty());
        assert!(state.context_entries.is_empty());
        assert!(state.tool_configs.is_empty());
        assert!(state.prompt_templates.is_empty());
        assert!(state.project_config.is_none());
        assert!(state.heartbeat_entries.is_empty());
    }

    #[test]
    fn test_load_with_agent_and_context() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(
            dir.path().join("AGENT.md"),
            "# AGENT.md\n---\n## test-agent\nTest.\n### Capabilities\n- model: test\n",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("CONTEXT.md"),
            "# CONTEXT.md\n---\n## Overview\nTest project.\n",
        )
        .unwrap();

        let state = SpecialFilesLoader::load(dir.path()).unwrap();
        assert_eq!(state.agents.len(), 1);
        assert_eq!(state.agents[0].name, "test-agent");
        assert_eq!(state.context_entries.len(), 1);
        assert_eq!(state.context_entries[0].section, "Overview");
    }

    #[test]
    fn test_load_with_colony_config() {
        let dir = tempfile::tempdir().unwrap();
        let colony_dir = dir.path().join(".colony");
        std::fs::create_dir(&colony_dir).unwrap();
        std::fs::write(
            colony_dir.join("config.toml"),
            r#"
[project]
name = "test-project"

[agents]
default_model = "claude-sonnet-4-20250514"
max_parallel_agents = 2
"#,
        )
        .unwrap();

        let state = SpecialFilesLoader::load(dir.path()).unwrap();
        let config = state.project_config.unwrap();
        assert_eq!(config.project.name, Some("test-project".to_string()));
        assert_eq!(
            config.agents.default_model,
            Some("claude-sonnet-4-20250514".to_string())
        );
        assert_eq!(config.agents.max_parallel_agents, Some(2));
    }

    #[test]
    fn test_load_all_files() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(
            dir.path().join("AGENT.md"),
            "# AGENT.md\n---\n## agent-a\nA.\n### Capabilities\n- model: m\n",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("CONTEXT.md"),
            "# CONTEXT.md\n---\n## Sec1\nContent.\n",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("TOOLS.md"),
            "# TOOLS.md\n---\n## shell\nRun commands.\n### Configuration\n- timeout: 30s\n",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("PROMPT.md"),
            "# PROMPT.md\n---\n## tpl\n```\nHello {name}\n```\n",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("HEARTBEAT.md"),
            "# HEARTBEAT.md\n---\n## agent: worker\n- status: idle\n- updated: 2025-01-15T00:00:00Z\n",
        )
        .unwrap();

        let state = SpecialFilesLoader::load(dir.path()).unwrap();
        assert_eq!(state.agents.len(), 1);
        assert_eq!(state.context_entries.len(), 1);
        assert_eq!(state.tool_configs.len(), 1);
        assert_eq!(state.prompt_templates.len(), 1);
        assert_eq!(state.heartbeat_entries.len(), 1);
        assert_eq!(state.heartbeat_entries[0].agent_name, "worker");
    }
}
