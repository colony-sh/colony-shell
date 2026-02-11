//! Streaming interruption/cancellation support
//!
//! Provides CancellationToken support for streaming responses, allowing
//! the user to cancel mid-stream via Ctrl+C. Partial responses are
//! preserved (everything streamed so far is kept).

use crate::provider::StreamEvent;
use crate::Result;
use futures::stream::Stream;
use futures::StreamExt;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken as TokioCancellationToken;

/// Cancellation token for streaming responses
#[derive(Clone)]
pub struct CancellationToken {
    inner: TokioCancellationToken,
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            inner: TokioCancellationToken::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Cancel the streaming operation
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.inner.cancel();
    }

    /// Check if cancellation has been requested
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Get the inner tokio CancellationToken for use with tokio::select!
    pub fn token(&self) -> &TokioCancellationToken {
        &self.inner
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a cancellable streaming operation
#[derive(Debug, Clone)]
pub struct CancellableStreamResult {
    pub text: String,
    pub was_cancelled: bool,
    pub events_received: usize,
}

/// Wrap a completion stream with cancellation support
///
/// Returns the partial text accumulated before cancellation (if cancelled),
/// or the full response text (if completed normally).
pub async fn cancellable_stream(
    stream: Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>,
    cancel: &CancellationToken,
) -> Result<CancellableStreamResult> {
    let mut stream = stream;
    let mut text = String::new();
    let mut events_received = 0usize;

    loop {
        tokio::select! {
            _ = cancel.token().cancelled() => {
                return Ok(CancellableStreamResult {
                    text,
                    was_cancelled: true,
                    events_received,
                });
            }
            event = stream.next() => {
                let Some(event) = event else {
                    break;
                };

                events_received += 1;

                match event? {
                    StreamEvent::TextDelta { text: delta, .. } => {
                        text.push_str(&delta);
                    }
                    StreamEvent::Done => {
                        break;
                    }
                    StreamEvent::Error { message } => {
                        return Err(crate::AgentError::Protocol(format!("Stream error: {}", message)));
                    }
                    _ => {}
                }

                if cancel.is_cancelled() {
                    return Ok(CancellableStreamResult {
                        text,
                        was_cancelled: true,
                        events_received,
                    });
                }
            }
        }
    }

    Ok(CancellableStreamResult {
        text,
        was_cancelled: false,
        events_received,
    })
}

/// Install a Ctrl+C handler that triggers cancellation
pub fn install_ctrlc_handler(cancel: &CancellationToken) -> Result<()> {
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel_clone.cancel();
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[test]
    fn test_cancellation_token_new() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_cancel() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_clone() {
        let token = CancellationToken::new();
        let cloned = token.clone();
        token.cancel();
        assert!(cloned.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancellable_stream_completes() {
        let events = vec![
            Ok(StreamEvent::TextDelta {
                text: "Hello".to_string(),
                index: 0,
            }),
            Ok(StreamEvent::TextDelta {
                text: " world".to_string(),
                index: 0,
            }),
            Ok(StreamEvent::Done),
        ];

        let stream = Box::pin(stream::iter(events));
        let cancel = CancellationToken::new();

        let result = cancellable_stream(stream, &cancel).await.unwrap();
        assert_eq!(result.text, "Hello world");
        assert!(!result.was_cancelled);
        assert_eq!(result.events_received, 3);
    }

    #[tokio::test]
    async fn test_cancellable_stream_cancelled() {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let events = vec![
            Ok(StreamEvent::TextDelta {
                text: "Hello".to_string(),
                index: 0,
            }),
            Ok(StreamEvent::TextDelta {
                text: " world".to_string(),
                index: 0,
            }),
            Ok(StreamEvent::Done),
        ];

        // Cancel before starting
        cancel_clone.cancel();

        let stream = Box::pin(stream::iter(events));
        let result = cancellable_stream(stream, &cancel).await.unwrap();
        assert!(result.was_cancelled);
    }

    #[tokio::test]
    async fn test_cancellable_stream_empty() {
        let events: Vec<Result<StreamEvent>> = vec![Ok(StreamEvent::Done)];
        let stream = Box::pin(stream::iter(events));
        let cancel = CancellationToken::new();

        let result = cancellable_stream(stream, &cancel).await.unwrap();
        assert_eq!(result.text, "");
        assert!(!result.was_cancelled);
    }

    #[tokio::test]
    async fn test_cancellable_stream_error() {
        let events: Vec<Result<StreamEvent>> = vec![Ok(StreamEvent::Error {
            message: "test error".to_string(),
        })];
        let stream = Box::pin(stream::iter(events));
        let cancel = CancellationToken::new();

        let result = cancellable_stream(stream, &cancel).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_cancellable_stream_result_fields() {
        let result = CancellableStreamResult {
            text: "partial".to_string(),
            was_cancelled: true,
            events_received: 5,
        };
        assert_eq!(result.text, "partial");
        assert!(result.was_cancelled);
        assert_eq!(result.events_received, 5);
    }
}
