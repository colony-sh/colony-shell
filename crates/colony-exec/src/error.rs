//! Execution errors

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Command not found: {0}")]
    CommandNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Expansion error: {0}")]
    ExpansionError(#[from] colony_expand::ExpansionError),

    #[error("Fork failed: {0}")]
    ForkError(nix::Error),

    #[error("Exec failed: {0}")]
    ExecError(nix::Error),

    #[error("Wait failed: {0}")]
    WaitError(nix::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid UTF-8 in argument")]
    InvalidUtf8,

    #[error("Redirect error: {0}")]
    RedirectError(String),

    #[error("Ambiguous redirect")]
    AmbiguousRedirect,

    #[error("Invalid redirect: {0}")]
    InvalidRedirect(String),

    #[error("Return with exit code {0}")]
    Return(i32),

    #[error("Break")]
    Break,

    #[error("Continue")]
    Continue,

    #[error("Exit with code {0}")]
    Exit(i32),

    #[error("errexit triggered with code {0}")]
    Errexit(i32),
}

pub type Result<T> = std::result::Result<T, ExecutionError>;
