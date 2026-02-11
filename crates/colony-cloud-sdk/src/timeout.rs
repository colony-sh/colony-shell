//! Per-operation timeouts and structured logging (P5-31)
//!
//! Configurable timeout per operation type (chat, stream, upload).
//! Structured log entries with operation, duration, status.
//! Request ID tracking across retries.

use crate::error::CloudError;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Operation types for per-operation timeout configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    /// Non-streaming chat completion
    Chat,
    /// Streaming chat completion
    Stream,
    /// Model listing
    Models,
    /// Recipe submission
    RecipeSubmit,
    /// Health check
    Health,
}

impl OperationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Stream => "stream",
            Self::Models => "models",
            Self::RecipeSubmit => "recipe_submit",
            Self::Health => "health",
        }
    }
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Timeout configuration per operation type
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub chat: Duration,
    pub stream: Duration,
    pub models: Duration,
    pub recipe_submit: Duration,
    pub health: Duration,
}

impl TimeoutConfig {
    /// Get timeout for a specific operation type
    pub fn for_operation(&self, op: OperationType) -> Duration {
        match op {
            OperationType::Chat => self.chat,
            OperationType::Stream => self.stream,
            OperationType::Models => self.models,
            OperationType::RecipeSubmit => self.recipe_submit,
            OperationType::Health => self.health,
        }
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            chat: Duration::from_secs(30),
            stream: Duration::from_secs(120),
            models: Duration::from_secs(10),
            recipe_submit: Duration::from_secs(15),
            health: Duration::from_secs(5),
        }
    }
}

/// Request tracking ID for correlating retries and logs
#[derive(Debug, Clone)]
pub struct RequestId(String);

impl RequestId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Status of a completed operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStatus {
    Success,
    Error,
    Timeout,
    Cancelled,
}

impl OperationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for OperationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured log entry for an operation
#[derive(Debug, Clone)]
pub struct OperationLog {
    pub request_id: String,
    pub operation: OperationType,
    pub status: OperationStatus,
    pub duration: Duration,
    pub attempt: u32,
    pub http_status: Option<u16>,
    pub error_message: Option<String>,
}

impl OperationLog {
    /// Emit this log entry via tracing
    pub fn emit(&self) {
        match self.status {
            OperationStatus::Success => {
                info!(
                    request_id = %self.request_id,
                    operation = %self.operation,
                    status = %self.status,
                    duration_ms = self.duration.as_millis(),
                    attempt = self.attempt,
                    http_status = ?self.http_status,
                    "Operation completed"
                );
            }
            OperationStatus::Error | OperationStatus::Timeout => {
                warn!(
                    request_id = %self.request_id,
                    operation = %self.operation,
                    status = %self.status,
                    duration_ms = self.duration.as_millis(),
                    attempt = self.attempt,
                    http_status = ?self.http_status,
                    error = ?self.error_message,
                    "Operation failed"
                );
            }
            OperationStatus::Cancelled => {
                debug!(
                    request_id = %self.request_id,
                    operation = %self.operation,
                    status = %self.status,
                    duration_ms = self.duration.as_millis(),
                    "Operation cancelled"
                );
            }
        }
    }
}

/// Guard that tracks an operation's lifecycle
///
/// Records start time and emits a structured log on drop or explicit finish.
pub struct OperationTracker {
    request_id: RequestId,
    operation: OperationType,
    start: Instant,
    attempt: u32,
    finished: bool,
}

impl OperationTracker {
    pub fn start(operation: OperationType) -> Self {
        let request_id = RequestId::new();
        debug!(
            request_id = %request_id,
            operation = %operation,
            "Starting operation"
        );

        Self {
            request_id,
            operation,
            start: Instant::now(),
            attempt: 1,
            finished: false,
        }
    }

    pub fn with_request_id(mut self, id: RequestId) -> Self {
        self.request_id = id;
        self
    }

    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = attempt;
        self
    }

    pub fn request_id(&self) -> &str {
        self.request_id.as_str()
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Record success and emit log
    pub fn finish_success(&mut self, http_status: Option<u16>) -> OperationLog {
        self.finished = true;
        let log = OperationLog {
            request_id: self.request_id.to_string(),
            operation: self.operation,
            status: OperationStatus::Success,
            duration: self.start.elapsed(),
            attempt: self.attempt,
            http_status,
            error_message: None,
        };
        log.emit();
        log
    }

    /// Record failure and emit log
    pub fn finish_error(&mut self, err: &CloudError) -> OperationLog {
        self.finished = true;
        let (status, http_status) = match err {
            CloudError::Http(e) => {
                let code = e.status().map(|s| s.as_u16());
                if e.is_timeout() {
                    (OperationStatus::Timeout, code)
                } else {
                    (OperationStatus::Error, code)
                }
            }
            CloudError::Api { status, .. } => (OperationStatus::Error, Some(*status)),
            _ => (OperationStatus::Error, None),
        };

        let log = OperationLog {
            request_id: self.request_id.to_string(),
            operation: self.operation,
            status,
            duration: self.start.elapsed(),
            attempt: self.attempt,
            http_status,
            error_message: Some(err.to_string()),
        };
        log.emit();
        log
    }

    /// Record timeout and emit log
    pub fn finish_timeout(&mut self) -> OperationLog {
        self.finished = true;
        let log = OperationLog {
            request_id: self.request_id.to_string(),
            operation: self.operation,
            status: OperationStatus::Timeout,
            duration: self.start.elapsed(),
            attempt: self.attempt,
            http_status: None,
            error_message: Some("operation timed out".to_string()),
        };
        log.emit();
        log
    }
}

impl Drop for OperationTracker {
    fn drop(&mut self) {
        if !self.finished {
            let log = OperationLog {
                request_id: self.request_id.to_string(),
                operation: self.operation,
                status: OperationStatus::Cancelled,
                duration: self.start.elapsed(),
                attempt: self.attempt,
                http_status: None,
                error_message: None,
            };
            log.emit();
        }
    }
}

/// Execute an async operation with a timeout
pub async fn with_timeout<F, T>(
    timeout: Duration,
    operation: OperationType,
    fut: F,
) -> Result<T, CloudError>
where
    F: std::future::Future<Output = Result<T, CloudError>>,
{
    let mut tracker = OperationTracker::start(operation);

    match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(result)) => {
            tracker.finish_success(None);
            Ok(result)
        }
        Ok(Err(err)) => {
            tracker.finish_error(&err);
            Err(err)
        }
        Err(_elapsed) => {
            tracker.finish_timeout();
            Err(CloudError::Stream(format!(
                "{} operation timed out after {:?}",
                operation, timeout
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_config_defaults() {
        let config = TimeoutConfig::default();
        assert_eq!(config.chat, Duration::from_secs(30));
        assert_eq!(config.stream, Duration::from_secs(120));
        assert_eq!(config.models, Duration::from_secs(10));
    }

    #[test]
    fn test_timeout_config_for_operation() {
        let config = TimeoutConfig::default();
        assert_eq!(
            config.for_operation(OperationType::Chat),
            Duration::from_secs(30)
        );
        assert_eq!(
            config.for_operation(OperationType::Stream),
            Duration::from_secs(120)
        );
    }

    #[test]
    fn test_operation_type_display() {
        assert_eq!(OperationType::Chat.to_string(), "chat");
        assert_eq!(OperationType::Stream.to_string(), "stream");
        assert_eq!(OperationType::Models.to_string(), "models");
    }

    #[test]
    fn test_request_id_uniqueness() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();
        assert_ne!(id1.as_str(), id2.as_str());
    }

    #[test]
    fn test_operation_tracker_lifecycle() {
        let mut tracker = OperationTracker::start(OperationType::Chat);
        assert!(!tracker.request_id().is_empty());
        assert!(tracker.elapsed() < Duration::from_secs(1));

        let log = tracker.finish_success(Some(200));
        assert_eq!(log.status, OperationStatus::Success);
        assert_eq!(log.http_status, Some(200));
    }

    #[test]
    fn test_operation_tracker_error() {
        let mut tracker = OperationTracker::start(OperationType::Models);
        let err = CloudError::Api {
            status: 500,
            message: "internal error".to_string(),
        };

        let log = tracker.finish_error(&err);
        assert_eq!(log.status, OperationStatus::Error);
        assert_eq!(log.http_status, Some(500));
        assert!(log.error_message.is_some());
    }

    #[test]
    fn test_operation_tracker_timeout() {
        let mut tracker = OperationTracker::start(OperationType::Stream);
        let log = tracker.finish_timeout();
        assert_eq!(log.status, OperationStatus::Timeout);
    }

    #[test]
    fn test_operation_tracker_drop_cancelled() {
        let _tracker = OperationTracker::start(OperationType::Health);
        // Drops without finish -> logs as cancelled
    }

    #[tokio::test]
    async fn test_with_timeout_success() {
        let result = with_timeout(
            Duration::from_secs(5),
            OperationType::Chat,
            async { Ok::<_, CloudError>(42) },
        )
        .await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_timeout_expires() {
        let result = with_timeout(
            Duration::from_millis(10),
            OperationType::Chat,
            async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok::<_, CloudError>(42)
            },
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_with_timeout_error_passthrough() {
        let result = with_timeout(
            Duration::from_secs(5),
            OperationType::Chat,
            async {
                Err::<i32, _>(CloudError::Api {
                    status: 400,
                    message: "bad request".to_string(),
                })
            },
        )
        .await;

        assert!(matches!(result, Err(CloudError::Api { status: 400, .. })));
    }

    #[test]
    fn test_operation_log_emit() {
        let log = OperationLog {
            request_id: "test-id".to_string(),
            operation: OperationType::Chat,
            status: OperationStatus::Success,
            duration: Duration::from_millis(100),
            attempt: 1,
            http_status: Some(200),
            error_message: None,
        };
        log.emit();
    }
}
