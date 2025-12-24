mod approvals;
mod config;
mod encrypt;
mod keychain;
mod lineage;
mod registry;

pub use approvals::{ApprovalDatabase, ApprovalScope, SecureApproval};
pub use config::SecureConfig;
pub use encrypt::{decrypt, encrypt, generate_key, is_encrypted};
pub use keychain::Keychain;
pub use lineage::ProcessLineage;
pub use registry::{AccessResult, SecureRegistry, SecureVarSource, SecureVariable};

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SecureError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Keychain error: {0}")]
    Keychain(String),

    #[error("Access denied to secure variable: {0}")]
    AccessDenied(String),

    #[error("Variable not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, SecureError>;

pub fn default_config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("colony"))
        .unwrap_or_else(|| PathBuf::from("~/.config/colony"))
}

pub fn default_approvals_db_path() -> PathBuf {
    default_config_dir().join("secure-approvals.db")
}

pub fn default_secure_vars_config_path() -> PathBuf {
    default_config_dir().join("secure-vars.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_paths() {
        let config_dir = default_config_dir();
        assert!(config_dir.to_string_lossy().contains("colony"));

        let db_path = default_approvals_db_path();
        assert!(db_path.to_string_lossy().contains("secure-approvals.db"));
    }
}
