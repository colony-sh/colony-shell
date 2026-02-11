//! Recipe system errors

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RecipeError {
    #[error("recipe not found: {0}")]
    NotFound(String),

    #[error("invalid recipe: {0}")]
    Invalid(String),

    #[error("missing required file: {0}")]
    MissingFile(String),

    #[error("TOML parse error in {path}: {source}")]
    TomlParse {
        path: String,
        source: toml::de::Error,
    },

    #[error("I/O error: {source} ({path})")]
    Io {
        path: String,
        source: std::io::Error,
    },

    #[error("recipe already installed: {0}")]
    AlreadyInstalled(String),

    #[error("invalid recipe URL: {0}")]
    InvalidUrl(String),

    #[error("registry error: {0}")]
    Registry(String),
}

pub type Result<T> = std::result::Result<T, RecipeError>;
