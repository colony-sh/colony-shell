//! Client-side rate limiting (P5-29)
//!
//! Parses X-RateLimit-Remaining and X-RateLimit-Reset headers.
//! Implements a token bucket with pre-flight checks before sending requests.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Rate limit state extracted from response headers
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    /// Remaining requests in current window
    pub remaining: u32,
    /// When the rate limit resets (absolute instant)
    pub reset_at: Instant,
    /// Total limit for the window (if provided)
    pub limit: Option<u32>,
}

/// Configuration for per-endpoint rate limits
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Default requests per second (fallback when no headers)
    pub default_rps: f64,
    /// Per-endpoint overrides
    pub endpoint_limits: HashMap<String, f64>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        let mut endpoint_limits = HashMap::new();
        endpoint_limits.insert("/v1/chat/completions".to_string(), 10.0);
        endpoint_limits.insert("/v1/models".to_string(), 60.0);
        endpoint_limits.insert("/v1/recipes".to_string(), 30.0);

        Self {
            default_rps: 10.0,
            endpoint_limits,
        }
    }
}

/// Token bucket rate limiter
#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(rps: f64) -> Self {
        Self {
            tokens: rps,
            max_tokens: rps,
            refill_rate: rps,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }

    fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn time_until_available(&mut self) -> Duration {
        self.refill();
        if self.tokens >= 1.0 {
            return Duration::ZERO;
        }
        let needed = 1.0 - self.tokens;
        Duration::from_secs_f64(needed / self.refill_rate)
    }
}

/// Client-side rate limiter with per-endpoint tracking
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    server_limits: Arc<Mutex<HashMap<String, RateLimitInfo>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(Mutex::new(HashMap::new())),
            server_limits: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Pre-flight check: wait until we can send a request
    ///
    /// Returns immediately if tokens are available; waits if rate limited.
    pub async fn acquire(&self, endpoint: &str) {
        let wait = {
            let mut buckets = self.buckets.lock().await;
            let bucket = buckets.entry(endpoint.to_string()).or_insert_with(|| {
                let rps = self
                    .config
                    .endpoint_limits
                    .get(endpoint)
                    .copied()
                    .unwrap_or(self.config.default_rps);
                TokenBucket::new(rps)
            });

            // Check server-side rate limit first
            if let Some(info) = self.check_server_limit(endpoint).await {
                if info.remaining == 0 {
                    let wait = info.reset_at.saturating_duration_since(Instant::now());
                    if !wait.is_zero() {
                        warn!(
                            endpoint = endpoint,
                            wait_ms = wait.as_millis(),
                            "Server rate limit hit, waiting for reset"
                        );
                        return tokio::time::sleep(wait).await;
                    }
                }
            }

            if bucket.try_acquire() {
                return;
            }

            bucket.time_until_available()
        };

        if !wait.is_zero() {
            debug!(
                endpoint = endpoint,
                wait_ms = wait.as_millis(),
                "Rate limit: waiting for token"
            );
            tokio::time::sleep(wait).await;

            let mut buckets = self.buckets.lock().await;
            if let Some(bucket) = buckets.get_mut(endpoint) {
                bucket.try_acquire();
            }
        }
    }

    /// Update rate limit state from response headers
    pub async fn update_from_headers(&self, endpoint: &str, headers: &reqwest::header::HeaderMap) {
        let remaining = headers
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u32>().ok());

        let reset = headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        let limit = headers
            .get("x-ratelimit-limit")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u32>().ok());

        if let (Some(remaining), Some(reset_secs)) = (remaining, reset) {
            let reset_at = Instant::now() + Duration::from_secs(reset_secs);
            let info = RateLimitInfo {
                remaining,
                reset_at,
                limit,
            };

            debug!(
                endpoint = endpoint,
                remaining = remaining,
                reset_secs = reset_secs,
                "Updated rate limit info from headers"
            );

            let mut limits = self.server_limits.lock().await;
            limits.insert(endpoint.to_string(), info);
        }
    }

    /// Check if we have server-side rate limit info for this endpoint
    async fn check_server_limit(&self, endpoint: &str) -> Option<RateLimitInfo> {
        let limits = self.server_limits.lock().await;
        limits.get(endpoint).cloned()
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(RateLimitConfig::default())
    }
}

impl Clone for RateLimiter {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            buckets: Arc::clone(&self.buckets),
            server_limits: Arc::clone(&self.server_limits),
        }
    }
}

/// Parse rate limit headers from a reqwest response
pub fn parse_rate_limit_headers(headers: &reqwest::header::HeaderMap) -> Option<RateLimitInfo> {
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok())?;

    let reset_secs = headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())?;

    let limit = headers
        .get("x-ratelimit-limit")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok());

    Some(RateLimitInfo {
        remaining,
        reset_at: Instant::now() + Duration::from_secs(reset_secs),
        limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(10.0);
        assert!(bucket.try_acquire());
        assert!(bucket.try_acquire());
    }

    #[test]
    fn test_token_bucket_exhaustion() {
        let mut bucket = TokenBucket::new(2.0);
        assert!(bucket.try_acquire());
        assert!(bucket.try_acquire());
        assert!(!bucket.try_acquire());
    }

    #[test]
    fn test_token_bucket_time_until_available() {
        let mut bucket = TokenBucket::new(1.0);
        bucket.try_acquire();
        let wait = bucket.time_until_available();
        assert!(wait > Duration::ZERO);
        assert!(wait <= Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire() {
        let limiter = RateLimiter::new(RateLimitConfig {
            default_rps: 100.0,
            endpoint_limits: HashMap::new(),
        });

        limiter.acquire("/test").await;
    }

    #[tokio::test]
    async fn test_rate_limiter_update_from_headers() {
        let limiter = RateLimiter::default();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "5".parse().unwrap());
        headers.insert("x-ratelimit-reset", "60".parse().unwrap());
        headers.insert("x-ratelimit-limit", "100".parse().unwrap());

        limiter
            .update_from_headers("/v1/chat/completions", &headers)
            .await;

        let info = limiter
            .check_server_limit("/v1/chat/completions")
            .await
            .unwrap();
        assert_eq!(info.remaining, 5);
        assert_eq!(info.limit, Some(100));
    }

    #[test]
    fn test_parse_rate_limit_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "10".parse().unwrap());
        headers.insert("x-ratelimit-reset", "30".parse().unwrap());
        headers.insert("x-ratelimit-limit", "60".parse().unwrap());

        let info = parse_rate_limit_headers(&headers).unwrap();
        assert_eq!(info.remaining, 10);
        assert_eq!(info.limit, Some(60));
    }

    #[test]
    fn test_parse_rate_limit_headers_missing() {
        let headers = reqwest::header::HeaderMap::new();
        assert!(parse_rate_limit_headers(&headers).is_none());
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert!(config.endpoint_limits.contains_key("/v1/chat/completions"));
        assert!(config.endpoint_limits.contains_key("/v1/models"));
    }
}
