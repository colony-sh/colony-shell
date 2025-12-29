//! Trust management for .colonyrc files
//!
//! Implements direnv-style trust model:
//! - Configs must be explicitly trusted before loading
//! - Trust is tied to config content hash (changes require re-trust)
//! - Trusted configs stored in ~/.config/colony/trusted.toml

use crate::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Manages trusted .colonyrc files
pub struct TrustManager {
    trust_file: PathBuf,
    trusted: TrustedConfigs,
}

/// Trust status for a config file
#[derive(Debug, Clone, PartialEq)]
pub enum TrustStatus {
    /// Config is trusted with this hash
    Trusted { hash: String },

    /// Config exists but is not trusted
    Untrusted { path: PathBuf, hash: String },

    /// Config changed since last trust
    Changed {
        path: PathBuf,
        current_hash: String,
        trusted_hash: String,
    },

    /// No config found
    NotFound,
}

/// Stored trust data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TrustedConfigs {
    /// Map: absolute directory path → config content hash
    #[serde(default)]
    directories: HashMap<String, TrustedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustedEntry {
    /// SHA256 hash of .colonyrc content
    hash: String,

    /// When this directory was trusted
    #[serde(skip_serializing_if = "Option::is_none")]
    trusted_at: Option<String>,
}

impl TrustManager {
    /// Create trust manager
    ///
    /// Loads trust data from ~/.config/colony/trusted.toml
    pub fn new() -> Result<Self> {
        let trust_file = Self::trust_file_path()?;

        // Load existing trust data
        let trusted = if trust_file.exists() {
            let content = fs::read_to_string(&trust_file)
                .map_err(|e| ConfigError::IoError(format!("failed to read trust file: {}", e)))?;

            toml::from_str(&content).unwrap_or_default()
        } else {
            TrustedConfigs::default()
        };

        Ok(Self {
            trust_file,
            trusted,
        })
    }

    /// Get trust file path: ~/.config/colony/trusted.toml
    fn trust_file_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().ok_or_else(|| {
            ConfigError::IoError("could not determine config directory".to_string())
        })?;

        let colony_dir = config_dir.join("colony");
        fs::create_dir_all(&colony_dir).map_err(|e| {
            ConfigError::IoError(format!("failed to create colony config dir: {}", e))
        })?;

        Ok(colony_dir.join("trusted.toml"))
    }

    /// Check trust status of a config file
    pub fn check_trust(&self, config_path: &Path, content: &str) -> TrustStatus {
        let dir = config_path.parent().unwrap_or(Path::new("."));
        let dir_str = dir
            .canonicalize()
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_else(|| dir.display().to_string());

        let current_hash = Self::hash_content(content);

        match self.trusted.directories.get(&dir_str) {
            Some(entry) if entry.hash == current_hash => {
                TrustStatus::Trusted { hash: current_hash }
            }
            Some(entry) => TrustStatus::Changed {
                path: config_path.to_path_buf(),
                current_hash,
                trusted_hash: entry.hash.clone(),
            },
            None => TrustStatus::Untrusted {
                path: config_path.to_path_buf(),
                hash: current_hash,
            },
        }
    }

    /// Trust a config file
    ///
    /// Adds directory and content hash to trusted list
    pub fn trust(&mut self, config_path: &Path, content: &str) -> Result<()> {
        let dir = config_path.parent().unwrap_or(Path::new("."));
        let dir_str = dir
            .canonicalize()
            .map_err(|e| ConfigError::IoError(format!("failed to canonicalize path: {}", e)))?
            .to_str()
            .ok_or_else(|| ConfigError::IoError("path is not valid UTF-8".to_string()))?
            .to_string();

        let hash = Self::hash_content(content);
        let now = chrono::Local::now().to_rfc3339();

        self.trusted.directories.insert(
            dir_str,
            TrustedEntry {
                hash,
                trusted_at: Some(now),
            },
        );

        self.save()?;

        Ok(())
    }

    /// Untrust a directory
    pub fn untrust(&mut self, dir: &Path) -> Result<()> {
        let dir_str = dir
            .canonicalize()
            .map_err(|e| ConfigError::IoError(format!("failed to canonicalize path: {}", e)))?
            .to_str()
            .ok_or_else(|| ConfigError::IoError("path is not valid UTF-8".to_string()))?
            .to_string();

        self.trusted.directories.remove(&dir_str);
        self.save()?;

        Ok(())
    }

    /// List all trusted directories
    pub fn list_trusted(&self) -> Vec<(String, String)> {
        self.trusted
            .directories
            .iter()
            .map(|(dir, entry)| (dir.clone(), entry.hash.clone()))
            .collect()
    }

    /// Save trust data to disk
    fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(&self.trusted).map_err(|e| {
            ConfigError::SerializeError(format!("failed to serialize trust data: {}", e))
        })?;

        fs::write(&self.trust_file, content)
            .map_err(|e| ConfigError::IoError(format!("failed to write trust file: {}", e)))?;

        Ok(())
    }

    /// Compute SHA256 hash of config content
    fn hash_content(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_content() {
        let content = "version = 1\n[agent]\npersonality = \"test\"";
        let hash = TrustManager::hash_content(content);
        assert_eq!(hash.len(), 64); // SHA256 is 64 hex chars
    }

    #[test]
    fn test_trust_status_untrusted() {
        let manager = TrustManager {
            trust_file: PathBuf::from("/tmp/trust.toml"),
            trusted: TrustedConfigs::default(),
        };

        let status = manager.check_trust(Path::new("/tmp/.colonyrc"), "version = 1");

        match status {
            TrustStatus::Untrusted { .. } => {} // Expected
            _ => panic!("Expected Untrusted status"),
        }
    }
}
