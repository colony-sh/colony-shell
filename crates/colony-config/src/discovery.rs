//! Config file discovery
//!
//! Walks up directory tree to find .colonyrc files, supporting:
//! - Project-specific: ./project/.colonyrc
//! - User-wide: ~/.colonyrc
//! - Layering: User config + project config (merged)

use crate::{ColonyConfig, ConfigError, Result, TrustManager, TrustStatus};
use std::env;
use std::path::{Path, PathBuf};

/// Location where config was found
#[derive(Debug, Clone)]
pub struct ConfigLocation {
    /// Path to the .colonyrc file
    pub path: PathBuf,

    /// Parsed configuration
    pub config: ColonyConfig,

    /// Trust status
    pub trust_status: TrustStatus,

    /// Configuration scope
    pub scope: ConfigScope,
}

/// Configuration scope
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    /// System-wide config (~/.colonyrc)
    System,

    /// Project-specific config (found in current directory or ancestor)
    Project,
}

/// Discover .colonyrc configuration
///
/// Searches for configs in order:
/// 1. ~/.colonyrc (system-wide)
/// 2. Walk up from current directory to find project .colonyrc
///
/// Returns layered config: system + project merged
pub fn discover_config() -> Result<Option<ConfigLocation>> {
    let trust_manager = TrustManager::new()?;

    // Try system config first
    let system_config = find_system_config(&trust_manager)?;

    // Try project config
    let project_config = find_project_config(&trust_manager)?;

    // Merge configs if both exist
    match (system_config, project_config) {
        (Some(mut system), Some(project)) => {
            // Merge project into system
            system.config.merge(project.config.clone());

            // Return combined, preferring project trust status
            Ok(Some(ConfigLocation {
                path: project.path,
                config: system.config,
                trust_status: project.trust_status,
                scope: ConfigScope::Project,
            }))
        }
        (None, Some(project)) => Ok(Some(project)),
        (Some(system), None) => Ok(Some(system)),
        (None, None) => Ok(None),
    }
}

/// Find system-wide config at ~/.colonyrc
fn find_system_config(trust_manager: &TrustManager) -> Result<Option<ConfigLocation>> {
    let home = dirs::home_dir()
        .ok_or_else(|| ConfigError::IoError("could not determine home directory".to_string()))?;

    let config_path = home.join(".colonyrc");

    if !config_path.exists() {
        return Ok(None);
    }

    load_config_at(&config_path, ConfigScope::System, trust_manager)
}

/// Find project config by walking up directory tree
fn find_project_config(trust_manager: &TrustManager) -> Result<Option<ConfigLocation>> {
    let current_dir = env::current_dir()
        .map_err(|e| ConfigError::IoError(format!("failed to get current directory: {}", e)))?;

    // Walk up directory tree
    let mut dir = current_dir.as_path();

    loop {
        let config_path = dir.join(".colonyrc");

        if config_path.exists() {
            return load_config_at(&config_path, ConfigScope::Project, trust_manager);
        }

        // Move to parent directory
        dir = match dir.parent() {
            Some(parent) => parent,
            None => break, // Reached filesystem root
        };
    }

    Ok(None)
}

/// Load config from specific path
fn load_config_at(
    path: &Path,
    scope: ConfigScope,
    trust_manager: &TrustManager,
) -> Result<Option<ConfigLocation>> {
    // Read config file
    let content = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::IoError(format!("failed to read {}: {}", path.display(), e)))?;

    // Check trust
    let trust_status = trust_manager.check_trust(path, &content);

    // Parse config
    let config = ColonyConfig::from_toml(&content)?;

    Ok(Some(ConfigLocation {
        path: path.to_path_buf(),
        config,
        trust_status,
        scope,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_find_system_config_not_found() {
        let trust_manager = TrustManager::new().unwrap();
        let result = find_system_config(&trust_manager).unwrap();
        // May or may not have ~/.colonyrc - just check it doesn't error
        let _ = result;
    }

    #[test]
    fn test_walk_up_directory_tree() {
        let temp = TempDir::new().unwrap();
        let nested = temp.path().join("a/b/c");
        fs::create_dir_all(&nested).unwrap();

        // Create .colonyrc at top level
        let config_path = temp.path().join(".colonyrc");
        fs::write(&config_path, "version = 1").unwrap();

        // Change to nested directory
        let original = env::current_dir().unwrap();
        env::set_current_dir(&nested).ok();

        // Should find config at ancestor
        let trust_manager = TrustManager::new().unwrap();
        let result = find_project_config(&trust_manager).unwrap();

        assert!(result.is_some());
        let location = result.unwrap();
        assert_eq!(location.scope, ConfigScope::Project);

        // Restore
        env::set_current_dir(original).ok();
    }
}
