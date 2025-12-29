use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("LanceDB error: {0}")]
    LanceDb(#[from] lancedb::Error),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid scope: {0}")]
    InvalidScope(String),

    #[error("Memory not found: {0}")]
    NotFound(String),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),
}

pub type Result<T> = std::result::Result<T, MemoryError>;
