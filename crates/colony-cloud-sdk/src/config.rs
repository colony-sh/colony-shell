//! Configuration loading for Colony Cloud SDK
//!
//! Auth resolution order:
//! 1. Explicit API key passed to `ColonyClient::new()`
//! 2. `COLONY_API_KEY` environment variable
//! 3. `~/.colony/config.toml` file (under `[cloud]` section)

use crate::error::{CloudError, Result};
use serde::Deserialize;
use std::path::PathBuf;

const ENV_API_KEY: &str = "COLONY_API_KEY";
const ENV_BASE_URL: &str = "COLONY_CLOUD_URL";
const DEFAULT_BASE_URL: &str = "https://api.colony.sh";

/// Resolved configuration for the Colony Cloud client
#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub api_key: String,
    pub base_url: String,
}

impl CloudConfig {
    /// Resolve configuration from environment and config file.
    ///
    /// Tries `COLONY_API_KEY` env var first, then `~/.colony/config.toml`.
    /// Base URL comes from `COLONY_CLOUD_URL` env var, falling back to the
    /// production default.
    pub fn from_env() -> Result<Self> {
        let api_key = Self::resolve_api_key()?;
        let base_url = Self::resolve_base_url();

        Ok(Self { api_key, base_url })
    }

    /// Create config with an explicit API key.
    ///
    /// The base URL is still resolved from `COLONY_CLOUD_URL` (falling back
    /// to the production default). Use [`with_key_and_url`](Self::with_key_and_url)
    /// to supply both explicitly.
    pub fn with_key(api_key: impl Into<String>) -> Self {
        let base_url = Self::resolve_base_url();
        Self {
            api_key: api_key.into(),
            base_url,
        }
    }

    /// Create config with an explicit API key and base URL.
    pub fn with_key_and_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }

    fn resolve_base_url() -> String {
        std::env::var(ENV_BASE_URL).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
    }

    fn resolve_api_key() -> Result<String> {
        if let Ok(key) = std::env::var(ENV_API_KEY) {
            if !key.is_empty() {
                return Ok(key);
            }
        }

        if let Some(key) = Self::load_key_from_config_file()? {
            return Ok(key);
        }

        Err(CloudError::Auth(format!(
            "No Colony API key found. Set {} or add [cloud] api_key to ~/.colony/config.toml",
            ENV_API_KEY
        )))
    }

    fn config_file_path() -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join(".colony").join("config.toml"))
    }

    fn load_key_from_config_file() -> Result<Option<String>> {
        let Some(path) = Self::config_file_path() else {
            return Ok(None);
        };

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(CloudError::Config(format!(
                    "failed to read {}: {}",
                    path.display(),
                    e
                )));
            }
        };

        let config: ConfigFile = toml::from_str(&content).map_err(|e| {
            CloudError::Config(format!("invalid TOML in {}: {}", path.display(), e))
        })?;

        Ok(config.cloud.and_then(|c| c.api_key))
    }
}

/// Top-level structure of ~/.colony/config.toml (partial)
#[derive(Debug, Deserialize)]
struct ConfigFile {
    cloud: Option<CloudSection>,
}

/// [cloud] section of config.toml
#[derive(Debug, Deserialize)]
struct CloudSection {
    api_key: Option<String>,
    #[allow(dead_code)]
    base_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_key() {
        let config = CloudConfig::with_key("test-key-123");
        assert_eq!(config.api_key, "test-key-123");
        assert!(!config.base_url.is_empty());
    }

    #[test]
    fn test_default_base_url() {
        let config = CloudConfig::with_key("k");
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
    }
}
