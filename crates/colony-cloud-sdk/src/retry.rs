//! Retry with exponential backoff (P5-28)
//!
//! Retries on 429 (rate limit) and 5xx (server error) responses.
//! Does not retry on 4xx client errors (except 429).

use crate::error::{CloudError, Result};
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay between retries (before exponential increase)
    pub base_delay: Duration,
    /// Maximum delay between retries (cap on exponential growth)
    pub max_delay: Duration,
    /// Jitter factor (0.0 = no jitter, 1.0 = full random jitter)
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            jitter_factor: 0.5,
        }
    }
}

/// Outcome of a request attempt for the retry loop
#[derive(Debug)]
pub enum RequestOutcome<T> {
    /// Request succeeded
    Success(T),
    /// Request failed with a retryable error
    Retryable(CloudError),
    /// Request failed with a non-retryable error
    Fatal(CloudError),
}

/// Determine if an HTTP status code is retryable
pub fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

/// Determine if a CloudError is retryable
pub fn is_retryable_error(err: &CloudError) -> bool {
    match err {
        CloudError::Api { status, .. } => is_retryable_status(*status),
        CloudError::Http(e) => {
            if let Some(status) = e.status() {
                is_retryable_status(status.as_u16())
            } else {
                e.is_connect() || e.is_timeout()
            }
        }
        CloudError::Stream(_) => true,
        _ => false,
    }
}

/// Calculate delay for a retry attempt with exponential backoff and jitter
pub fn calculate_delay(config: &RetryConfig, attempt: u32) -> Duration {
    let exp_delay = config.base_delay.as_millis() as f64 * 2_f64.powi(attempt as i32);
    let capped = exp_delay.min(config.max_delay.as_millis() as f64);

    let jitter_range = capped * config.jitter_factor;
    let jitter = jitter_range * pseudo_random_fraction(attempt);
    let final_ms = (capped - jitter_range / 2.0 + jitter).max(0.0);

    Duration::from_millis(final_ms as u64)
}

/// Simple pseudo-random fraction based on attempt number
/// (avoids pulling in rand crate for a simple use case)
fn pseudo_random_fraction(seed: u32) -> f64 {
    let x = seed.wrapping_mul(2654435761);
    (x as f64) / (u32::MAX as f64)
}

/// Execute an async operation with retry logic
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, operation: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_error: Option<CloudError> = None;

    for attempt in 0..=config.max_retries {
        if attempt > 0 {
            let delay = calculate_delay(config, attempt - 1);
            debug!(
                attempt = attempt,
                delay_ms = delay.as_millis(),
                "Retrying request"
            );
            tokio::time::sleep(delay).await;
        }

        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(attempt = attempt, "Request succeeded after retry");
                }
                return Ok(result);
            }
            Err(err) => {
                if is_retryable_error(&err) && attempt < config.max_retries {
                    warn!(
                        attempt = attempt,
                        max_retries = config.max_retries,
                        error = %err,
                        "Retryable error, will retry"
                    );
                    last_error = Some(err);
                } else {
                    return Err(err);
                }
            }
        }
    }

    Err(last_error.expect("retry loop must have set last_error"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_is_retryable_status() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(502));
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(599));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(403));
        assert!(!is_retryable_status(404));
        assert!(!is_retryable_status(200));
    }

    #[test]
    fn test_is_retryable_error() {
        let retryable = CloudError::Api {
            status: 429,
            message: "rate limited".to_string(),
        };
        assert!(is_retryable_error(&retryable));

        let retryable_500 = CloudError::Api {
            status: 500,
            message: "server error".to_string(),
        };
        assert!(is_retryable_error(&retryable_500));

        let non_retryable = CloudError::Api {
            status: 400,
            message: "bad request".to_string(),
        };
        assert!(!is_retryable_error(&non_retryable));

        let auth_err = CloudError::Auth("bad key".to_string());
        assert!(!is_retryable_error(&auth_err));
    }

    #[test]
    fn test_calculate_delay_exponential() {
        let config = RetryConfig {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            jitter_factor: 0.0,
            ..Default::default()
        };

        let d0 = calculate_delay(&config, 0);
        let d1 = calculate_delay(&config, 1);
        let d2 = calculate_delay(&config, 2);

        assert_eq!(d0.as_millis(), 100);
        assert_eq!(d1.as_millis(), 200);
        assert_eq!(d2.as_millis(), 400);
    }

    #[test]
    fn test_calculate_delay_max_cap() {
        let config = RetryConfig {
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(5),
            jitter_factor: 0.0,
            ..Default::default()
        };

        let d10 = calculate_delay(&config, 10);
        assert!(d10 <= Duration::from_secs(5));
    }

    #[test]
    fn test_calculate_delay_with_jitter() {
        let config = RetryConfig {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            jitter_factor: 1.0,
            ..Default::default()
        };

        let d0 = calculate_delay(&config, 0);
        assert!(d0.as_millis() <= 100);
    }

    #[tokio::test]
    async fn test_with_retry_success_first_try() {
        let config = RetryConfig::default();
        let result = with_retry(&config, || async { Ok::<_, CloudError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_retry_success_after_retries() {
        let config = RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_millis(1),
            ..Default::default()
        };

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = with_retry(&config, || {
            let counter = counter_clone.clone();
            async move {
                let attempt = counter.fetch_add(1, Ordering::SeqCst);
                if attempt < 2 {
                    Err(CloudError::Api {
                        status: 500,
                        message: "server error".to_string(),
                    })
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_non_retryable_fails_immediately() {
        let config = RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_millis(1),
            ..Default::default()
        };

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32> = with_retry(&config, || {
            let counter = counter_clone.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err(CloudError::Api {
                    status: 400,
                    message: "bad request".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_exhausts_retries() {
        let config = RetryConfig {
            max_retries: 2,
            base_delay: Duration::from_millis(1),
            ..Default::default()
        };

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32> = with_retry(&config, || {
            let counter = counter_clone.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err(CloudError::Api {
                    status: 503,
                    message: "unavailable".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
