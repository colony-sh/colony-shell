//! Focus system errors

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FocusError {
    #[error("goal not found: {0}")]
    GoalNotFound(String),

    #[error("invalid transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("tree integrity error: {0}")]
    TreeIntegrity(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<rusqlite::Error> for FocusError {
    fn from(e: rusqlite::Error) -> Self {
        FocusError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for FocusError {
    fn from(e: serde_json::Error) -> Self {
        FocusError::Serialization(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, FocusError>;
