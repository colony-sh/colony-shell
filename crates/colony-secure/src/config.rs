use crate::{default_secure_vars_config_path, Result, SecureError};
use glob::Pattern;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecureConfig {
    #[serde(default)]
    pub patterns: PatternConfig,
    #[serde(default)]
    pub variables: VariableConfig,
    #[serde(default)]
    pub audit: AuditConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternConfig {
    #[serde(default = "default_secure_patterns")]
    pub secure: Vec<String>,
    #[serde(default)]
    pub not_secure: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VariableConfig {
    #[serde(default)]
    pub secure: Vec<String>,
}

/// Configuration for audit logging of secure variable access events.
///
/// These fields are parsed from the user's config file and stored, but the audit
/// logger itself is not yet implemented. When wired up, `enabled` should gate
/// whether access events are written, and `file` should specify the log path
/// (defaulting to a standard location under the colony config directory when
/// `None`).
///
/// TODO: Implement an `AuditLogger` that opens `file` (or a default path) on
/// `SecureRegistry` construction when `enabled` is `true`, and writes a
/// structured log entry for every `check_access` call and every grant/revoke.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditConfig {
    /// Whether audit logging is active. Parsed from config but not yet used.
    // TODO: Wire up to an AuditLogger in SecureRegistry when audit logging is implemented.
    #[allow(dead_code)]
    #[serde(default)]
    pub enabled: bool,
    /// Path to the audit log file. `None` means use the default colony log directory.
    // TODO: Wire up to an AuditLogger in SecureRegistry when audit logging is implemented.
    #[allow(dead_code)]
    #[serde(default)]
    pub file: Option<PathBuf>,
}

impl Default for PatternConfig {
    fn default() -> Self {
        Self {
            secure: default_secure_patterns(),
            not_secure: vec![
                "PUBLIC_KEY".to_string(),
                "SSH_AUTH_SOCK".to_string(),
                "GPG_AGENT_INFO".to_string(),
            ],
        }
    }
}

fn default_secure_patterns() -> Vec<String> {
    vec![
        "*_SECRET".to_string(),
        "*_SECRET_*".to_string(),
        "*_KEY".to_string(),
        "*_TOKEN".to_string(),
        "*_PASSWORD".to_string(),
        "*_CREDENTIAL".to_string(),
        "*_API_KEY".to_string(),
        "AWS_*_ACCESS_KEY*".to_string(),
    ]
}

impl SecureConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| SecureError::Config(e.to_string()))
    }

    pub fn load_or_default() -> Result<Self> {
        let path = default_secure_vars_config_path();
        if path.exists() {
            Self::load(&path)
        } else {
            Ok(Self::default())
        }
    }

    pub fn load_with_project_override(project_path: Option<&Path>) -> Result<Self> {
        let mut config = Self::load_or_default()?;

        if let Some(project) = project_path {
            let project_config_path = project.join(".colony").join("secure-vars.toml");
            if project_config_path.exists() {
                let project_config = Self::load(&project_config_path)?;
                config.merge(project_config);
            }
        }

        Ok(config)
    }

    pub fn merge(&mut self, other: SecureConfig) {
        self.patterns.secure.extend(other.patterns.secure);
        self.patterns.not_secure.extend(other.patterns.not_secure);
        self.variables.secure.extend(other.variables.secure);

        if other.audit.enabled {
            self.audit = other.audit;
        }
    }

    pub fn is_secure(&self, name: &str) -> bool {
        if self.patterns.not_secure.iter().any(|p| {
            Pattern::new(p)
                .map(|pat| pat.matches(name))
                .unwrap_or(false)
        }) {
            return false;
        }

        if self.variables.secure.contains(&name.to_string()) {
            return true;
        }

        self.patterns.secure.iter().any(|p| {
            Pattern::new(p)
                .map(|pat| pat.matches(name))
                .unwrap_or(false)
        })
    }

    pub fn matching_pattern(&self, name: &str) -> Option<String> {
        for pattern in &self.patterns.secure {
            if let Ok(pat) = Pattern::new(pattern) {
                if pat.matches(name) {
                    return Some(pattern.clone());
                }
            }
        }
        None
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content =
            toml::to_string_pretty(self).map_err(|e| SecureError::Config(e.to_string()))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_patterns() {
        let config = SecureConfig::default();

        assert!(config.is_secure("AWS_SECRET_KEY"));
        assert!(config.is_secure("GITHUB_TOKEN"));
        assert!(config.is_secure("DB_PASSWORD"));
        assert!(config.is_secure("API_KEY"));
        assert!(config.is_secure("MY_SECRET"));

        assert!(!config.is_secure("PUBLIC_KEY"));
        assert!(!config.is_secure("SSH_AUTH_SOCK"));
        assert!(!config.is_secure("PATH"));
        assert!(!config.is_secure("HOME"));
    }

    #[test]
    fn test_matching_pattern() {
        let config = SecureConfig::default();

        let pattern = config.matching_pattern("AWS_SECRET_KEY");
        assert!(pattern.is_some());

        let pattern = config.matching_pattern("PATH");
        assert!(pattern.is_none());
    }

    #[test]
    fn test_explicit_variables() {
        let mut config = SecureConfig::default();
        config.variables.secure.push("MY_CUSTOM_VAR".to_string());

        assert!(config.is_secure("MY_CUSTOM_VAR"));
    }

    #[test]
    fn test_not_secure_override() {
        let mut config = SecureConfig::default();
        config.patterns.not_secure.push("TEST_SECRET".to_string());

        assert!(!config.is_secure("TEST_SECRET"));
    }

    #[test]
    fn test_config_merge() {
        let mut base = SecureConfig::default();
        let mut project = SecureConfig::default();
        project.variables.secure.push("PROJECT_SECRET".to_string());
        project
            .patterns
            .not_secure
            .push("IGNORE_ME_KEY".to_string());

        base.merge(project);

        assert!(base
            .variables
            .secure
            .contains(&"PROJECT_SECRET".to_string()));
        assert!(base
            .patterns
            .not_secure
            .contains(&"IGNORE_ME_KEY".to_string()));
    }
}
