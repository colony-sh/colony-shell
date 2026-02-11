//! Error types for Colony Cloud SDK

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("streaming error: {0}")]
    Stream(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CloudError>;
