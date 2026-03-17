//! Special files system (F030)
//!
//! Loads and parses AGENT.md, CONTEXT.md, TOOLS.md, PROMPT.md, HEARTBEAT.md,
//! and .colony/config.toml from a directory. Triggered on directory changes
//! (cd, pushd, popd) via the tuple space.
//!
//! ## Directory Hierarchy
//!
//! Files are searched in order (first found wins, unless the type merges):
//! 1. Current directory
//! 2. Parent directory (recurse up to filesystem root)
//! 3. Home directory (`~/.colony/`)
//!
//! CONTEXT.md and HEARTBEAT.md merge from all levels.

mod agent_md;
mod context_md;
pub mod heartbeat;
mod prompt_md;
mod tools_md;
pub mod types;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use agent_md::AgentMdParser;
pub use context_md::ContextMdParser;
pub use heartbeat::{body_is_empty, parse_heartbeat, ChecklistItem, HeartbeatConfig};
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
    /// Load all special files from a single directory (no hierarchy walk).
    pub fn load(dir: &Path) -> Result<SpecialFilesState, SpecialFilesError> {
        let mut state = SpecialFilesState {
            directory: dir.to_path_buf(),
            agents: Vec::new(),
            context_entries: Vec::new(),
            tool_configs: Vec::new(),
            prompt_templates: Vec::new(),
            project_config: None,
            heartbeat_config: None,
            heartbeat_items: Vec::new(),
            heartbeat_path: None,
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
        if let Ok((config, items, path)) = Self::try_load_heartbeat_md(dir) {
            state.heartbeat_config = Some(config);
            state.heartbeat_items = items;
            state.heartbeat_path = Some(path);
        }

        Ok(state)
    }

    /// Load special files with directory hierarchy walking.
    ///
    /// For non-merging types (Agent, Tools, Prompt, Config, Heartbeat),
    /// the first directory that contains the file wins. For merging types
    /// (Context), entries from all ancestor directories are collected with
    /// the deepest directory's entries appearing first.
    pub fn load_with_hierarchy(dir: &Path) -> Result<SpecialFilesState, SpecialFilesError> {
        let mut state = SpecialFilesState {
            directory: dir.to_path_buf(),
            agents: Vec::new(),
            context_entries: Vec::new(),
            tool_configs: Vec::new(),
            prompt_templates: Vec::new(),
            project_config: None,
            heartbeat_config: None,
            heartbeat_items: Vec::new(),
            heartbeat_path: None,
        };

        let search_dirs = Self::build_search_path(dir);
        let mut found_agent = false;
        let mut found_tools = false;
        let mut found_prompt = false;
        let mut found_config = false;

        for search_dir in &search_dirs {
            if !found_agent {
                if let Ok(agents) = Self::try_load_agent_md(search_dir) {
                    if !agents.is_empty() {
                        state.agents = agents;
                        found_agent = true;
                    }
                }
            }
            if let Ok(mut context) = Self::try_load_context_md(search_dir) {
                if !context.is_empty() {
                    state.context_entries.append(&mut context);
                }
            }
            if !found_tools {
                if let Ok(tools) = Self::try_load_tools_md(search_dir) {
                    if !tools.is_empty() {
                        state.tool_configs = tools;
                        found_tools = true;
                    }
                }
            }
            if !found_prompt {
                if let Ok(prompts) = Self::try_load_prompt_md(search_dir) {
                    if !prompts.is_empty() {
                        state.prompt_templates = prompts;
                        found_prompt = true;
                    }
                }
            }
            if !found_config {
                if let Ok(config) = Self::try_load_colony_config(search_dir) {
                    state.project_config = Some(config);
                    found_config = true;
                }
            }
            // Heartbeat is first-found (not merging) since F080 operates on a single HEARTBEAT.md
            if state.heartbeat_path.is_none() {
                if let Ok((config, items, path)) = Self::try_load_heartbeat_md(search_dir) {
                    state.heartbeat_config = Some(config);
                    state.heartbeat_items = items;
                    state.heartbeat_path = Some(path);
                }
            }
        }

        Ok(state)
    }

    /// Collect metadata about loaded special files across search directories.
    pub fn collect_loaded_files(dir: &Path) -> HashMap<PathBuf, LoadedSpecialFile> {
        let mut loaded = HashMap::new();
        let search_dirs = Self::build_search_path(dir);
        let candidates: &[(SpecialFileType, &str)] = &[
            (SpecialFileType::Agent, "AGENT.md"),
            (SpecialFileType::Context, "CONTEXT.md"),
            (SpecialFileType::Tools, "TOOLS.md"),
            (SpecialFileType::Prompt, "PROMPT.md"),
            (SpecialFileType::Config, ".colony/config.toml"),
            (SpecialFileType::Heartbeat, "HEARTBEAT.md"),
        ];
        for search_dir in &search_dirs {
            for &(file_type, filename) in candidates {
                let path = search_dir.join(filename);
                if path.is_file() {
                    let content = std::fs::read_to_string(&path).unwrap_or_default();
                    let last_modified = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .unwrap_or(SystemTime::UNIX_EPOCH);
                    loaded.insert(
                        path.clone(),
                        LoadedSpecialFile {
                            file_type,
                            path,
                            content,
                            last_modified,
                        },
                    );
                }
            }
        }
        loaded
    }

    /// Build the ordered list of directories to search.
    fn build_search_path(dir: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        let mut current = dir.to_path_buf();
        loop {
            dirs.push(current.clone());
            let Some(parent) = current.parent() else {
                break;
            };
            if parent == current {
                break;
            }
            current = parent.to_path_buf();
        }
        if let Some(home) = dirs::home_dir() {
            let home_colony = home.join(".colony");
            if home_colony.is_dir() && !dirs.iter().any(|d| d == &home_colony) {
                dirs.push(home_colony);
            }
        }
        dirs
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

    /// Check whether a directory or any of its ancestors contain special files.
    pub fn has_special_files_in_hierarchy(dir: &Path) -> bool {
        Self::build_search_path(dir)
            .iter()
            .any(|d| Self::has_special_files(d))
    }

    fn try_load_agent_md(dir: &Path) -> Result<Vec<types::AgentDefinition>, SpecialFilesError> {
        let path = dir.join("AGENT.md");
        let Some(content) = read_file_if_exists(&path)? else {
            return Ok(Vec::new());
        };
        AgentMdParser::parse(&content, &path).map_err(|e| SpecialFilesError::Parse {
            file: path.display().to_string(),
            message: e.to_string(),
        })
    }

    fn try_load_context_md(dir: &Path) -> Result<Vec<types::ContextEntry>, SpecialFilesError> {
        let path = dir.join("CONTEXT.md");
        let Some(content) = read_file_if_exists(&path)? else {
            return Ok(Vec::new());
        };
        Ok(ContextMdParser::parse(&content, &path))
    }

    fn try_load_tools_md(dir: &Path) -> Result<Vec<types::ToolConfig>, SpecialFilesError> {
        let path = dir.join("TOOLS.md");
        let Some(content) = read_file_if_exists(&path)? else {
            return Ok(Vec::new());
        };
        Ok(ToolsMdParser::parse(&content, &path))
    }

    fn try_load_prompt_md(dir: &Path) -> Result<Vec<types::PromptTemplate>, SpecialFilesError> {
        let path = dir.join("PROMPT.md");
        let Some(content) = read_file_if_exists(&path)? else {
            return Ok(Vec::new());
        };
        Ok(PromptMdParser::parse(&content, &path))
    }

    fn try_load_colony_config(dir: &Path) -> Result<types::ProjectConfig, SpecialFilesError> {
        let path = dir.join(".colony/config.toml");
        let Some(content) = read_file_if_exists(&path)? else {
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

    fn try_load_heartbeat_md(
        dir: &Path,
    ) -> Result<(types::HeartbeatConfig, Vec<types::ChecklistItem>, PathBuf), SpecialFilesError>
    {
        let path = dir.join("HEARTBEAT.md");
        let Some(content) = read_file_if_exists(&path)? else {
            return Err(SpecialFilesError::Io {
                path: path.display().to_string(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
            });
        };
        let (config, items) = parse_heartbeat(&content);
        Ok((config, items, path))
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
    fn test_load_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let state = SpecialFilesLoader::load(dir.path()).unwrap();
        assert!(state.agents.is_empty());
        assert!(state.context_entries.is_empty());
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
            "---\ninterval: 10m\n---\n\n- [ ] Check tests\n",
        )
        .unwrap();
        let state = SpecialFilesLoader::load(dir.path()).unwrap();
        assert_eq!(state.agents.len(), 1);
        assert_eq!(state.context_entries.len(), 1);
        assert_eq!(state.tool_configs.len(), 1);
        assert_eq!(state.prompt_templates.len(), 1);
        assert!(state.heartbeat_config.is_some());
        assert_eq!(state.heartbeat_items.len(), 1);
    }

    #[test]
    fn test_special_file_type_filename() {
        assert_eq!(SpecialFileType::Agent.filename(), "AGENT.md");
        assert_eq!(SpecialFileType::Heartbeat.filename(), "HEARTBEAT.md");
    }

    #[test]
    fn test_special_file_type_merges() {
        assert!(!SpecialFileType::Agent.merges_up_tree());
        assert!(SpecialFileType::Context.merges_up_tree());
        // Heartbeat changed to first-found in F080
        assert!(!SpecialFileType::Heartbeat.merges_up_tree());
    }

    #[test]
    fn test_hierarchy_agent_first_wins() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("child");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(
            root.path().join("AGENT.md"),
            "# AGENT.md\n---\n## parent-agent\nP.\n### Capabilities\n- model: p\n",
        )
        .unwrap();
        std::fs::write(
            child.join("AGENT.md"),
            "# AGENT.md\n---\n## child-agent\nC.\n### Capabilities\n- model: c\n",
        )
        .unwrap();
        let state = SpecialFilesLoader::load_with_hierarchy(&child).unwrap();
        assert_eq!(state.agents.len(), 1);
        assert_eq!(state.agents[0].name, "child-agent");
    }

    #[test]
    fn test_hierarchy_agent_inherits_from_parent() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("child");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(
            root.path().join("AGENT.md"),
            "# AGENT.md\n---\n## inherited\nI.\n### Capabilities\n- model: p\n",
        )
        .unwrap();
        let state = SpecialFilesLoader::load_with_hierarchy(&child).unwrap();
        assert_eq!(state.agents.len(), 1);
        assert_eq!(state.agents[0].name, "inherited");
    }

    #[test]
    fn test_hierarchy_context_merges() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("child");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(
            root.path().join("CONTEXT.md"),
            "# CONTEXT.md\n---\n## Org\nAcme.\n",
        )
        .unwrap();
        std::fs::write(
            child.join("CONTEXT.md"),
            "# CONTEXT.md\n---\n## Project\nX.\n",
        )
        .unwrap();
        let state = SpecialFilesLoader::load_with_hierarchy(&child).unwrap();
        assert_eq!(state.context_entries.len(), 2);
        assert_eq!(state.context_entries[0].section, "Project");
        assert_eq!(state.context_entries[1].section, "Org");
    }

    #[test]
    fn test_hierarchy_heartbeat_first_found() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("child");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(
            root.path().join("HEARTBEAT.md"),
            "---\ninterval: 1h\n---\n\n- [ ] Parent check\n",
        )
        .unwrap();
        std::fs::write(
            child.join("HEARTBEAT.md"),
            "---\ninterval: 5m\n---\n\n- [ ] Child check\n",
        )
        .unwrap();
        let state = SpecialFilesLoader::load_with_hierarchy(&child).unwrap();
        // First-found: child wins
        assert!(state.heartbeat_config.is_some());
        assert_eq!(state.heartbeat_config.as_ref().unwrap().interval_secs, 300); // 5m
        assert_eq!(state.heartbeat_items.len(), 1);
        assert_eq!(state.heartbeat_items[0].text, "Child check");
    }

    #[test]
    fn test_hierarchy_empty_graceful() {
        let dir = tempfile::tempdir().unwrap();
        let state = SpecialFilesLoader::load_with_hierarchy(dir.path()).unwrap();
        assert!(state.agents.is_empty());
        assert!(state.context_entries.is_empty());
    }

    #[test]
    fn test_has_special_files_in_hierarchy() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("deep");
        std::fs::create_dir(&child).unwrap();
        assert!(!SpecialFilesLoader::has_special_files_in_hierarchy(&child));
        std::fs::write(root.path().join("AGENT.md"), "# AGENT.md\n").unwrap();
        assert!(SpecialFilesLoader::has_special_files_in_hierarchy(&child));
    }

    #[test]
    fn test_collect_loaded_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("AGENT.md"),
            "# AGENT.md\n---\n## bot\nBot.\n### Capabilities\n- model: x\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("CONTEXT.md"),
            "# CONTEXT.md\n---\n## Info\nCtx.\n",
        )
        .unwrap();
        let loaded = SpecialFilesLoader::collect_loaded_files(dir.path());
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn test_build_search_path() {
        let dir = PathBuf::from("/a/b/c");
        let path = SpecialFilesLoader::build_search_path(&dir);
        assert!(path.len() >= 3);
        assert_eq!(path[0], PathBuf::from("/a/b/c"));
        assert_eq!(path[1], PathBuf::from("/a/b"));
        assert_eq!(path[2], PathBuf::from("/a"));
    }

    #[test]
    fn test_hierarchy_deep_nesting() {
        let root = tempfile::tempdir().unwrap();
        let c = root.path().join("a/b/c");
        std::fs::create_dir_all(&c).unwrap();
        std::fs::write(
            root.path().join("CONTEXT.md"),
            "# CONTEXT.md\n---\n## Root\nRoot.\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("a/AGENT.md"),
            "# AGENT.md\n---\n## deep\nD.\n### Capabilities\n- model: d\n",
        )
        .unwrap();
        let state = SpecialFilesLoader::load_with_hierarchy(&c).unwrap();
        assert_eq!(state.agents.len(), 1);
        assert_eq!(state.agents[0].name, "deep");
        assert_eq!(state.context_entries.len(), 1);
    }
}
