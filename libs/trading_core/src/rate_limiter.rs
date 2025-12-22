//! Generic rate limiter implementations
//!
//! This module provides generic rate limiting algorithms that can be used
//! by exchange-specific implementations.

use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;

/// Rate limit check result
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Time to wait before retrying (if not allowed)
    pub retry_after: Option<Duration>,
    /// Current usage count
    pub current_count: u64,
    /// Limit for this interval
    pub limit: u64,
}

impl RateLimitResult {
    pub fn allowed(count: u64, limit: u64) -> Self {
        Self {
            allowed: true,
            retry_after: None,
            current_count: count,
            limit,
        }
    }

    pub fn blocked(count: u64, limit: u64, retry_after: Duration) -> Self {
        Self {
            allowed: false,
            retry_after: Some(retry_after),
            current_count: count,
            limit,
        }
    }
}

/// Generic rate limiter trait
#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if a request with the given weight is allowed
    /// Returns the result and whether to wait before retrying
    async fn check(&self, weight: u64) -> RateLimitResult;

    /// Wait if necessary before making a request
    /// Returns the result after waiting
    async fn wait_if_needed(&self, weight: u64) -> RateLimitResult {
        loop {
            let result = self.check(weight).await;
            if result.allowed {
                return result;
            }
            if let Some(duration) = result.retry_after {
                sleep(duration).await;
            } else {
                // No retry_after specified, wait a short time
                sleep(Duration::from_millis(100)).await;
            }
        }
    }

    /// Record that a request with the given weight was made
    async fn record(&self, weight: u64);

    /// Update the limit (for dynamic limits from API responses)
    async fn update_limit(&self, limit: u64);

    /// Get current usage count
    async fn current_count(&self) -> u64;

    /// Get current limit
    async fn current_limit(&self) -> u64;
}

/// Fixed window rate limiter
///
/// This algorithm divides time into fixed windows (e.g., 1 minute, 10 seconds).
/// All requests within a window count towards the limit, and the count resets
/// at the start of the next window.
///
/// Example: 100 requests per minute
/// - Window 1: 00:00:00 - 00:00:59 (count resets at 00:01:00)
/// - Window 2: 00:01:00 - 00:01:59 (count resets at 00:02:00)
#[derive(Debug)]
pub struct FixedWindowRateLimiter {
    limit: RwLock<u64>,
    interval: Duration,
    count: RwLock<u64>,
    window_start: RwLock<Instant>,
}

impl FixedWindowRateLimiter {
    /// Create a new fixed window rate limiter
    ///
    /// # Arguments
    /// * `limit` - Maximum number of requests (or weight) per interval
    /// * `interval` - Duration of each window (e.g., Duration::from_secs(60) for 1 minute)
    pub fn new(limit: u64, interval: Duration) -> Self {
        Self {
            limit: RwLock::new(limit),
            interval,
            count: RwLock::new(0),
            window_start: RwLock::new(Instant::now()),
        }
    }
}

#[async_trait::async_trait]
impl RateLimiter for FixedWindowRateLimiter {
    async fn check(&self, weight: u64) -> RateLimitResult {
        let now = Instant::now();
        let mut window_start = self.window_start.write().await;

        // Reset window if needed
        if now.duration_since(*window_start) >= self.interval {
            *window_start = now;
            *self.count.write().await = 0;
        }

        let limit = *self.limit.read().await;
        let count = *self.count.read().await;

        if count + weight <= limit {
            RateLimitResult::allowed(count, limit)
        } else {
            // Calculate time until next window
            let elapsed = now.duration_since(*window_start);
            let retry_after = self.interval.saturating_sub(elapsed);
            RateLimitResult::blocked(count, limit, retry_after)
        }
    }

    async fn record(&self, weight: u64) {
        let now = Instant::now();
        let mut window_start = self.window_start.write().await;

        // Reset window if needed
        if now.duration_since(*window_start) >= self.interval {
            *window_start = now;
            *self.count.write().await = 0;
        }

        *self.count.write().await += weight;
    }

    async fn update_limit(&self, limit: u64) {
        *self.limit.write().await = limit;
    }

    async fn current_count(&self) -> u64 {
        let now = Instant::now();
        let mut window_start = self.window_start.write().await;

        // Reset window if needed
        if now.duration_since(*window_start) >= self.interval {
            *window_start = now;
            *self.count.write().await = 0;
        }

        *self.count.read().await
    }

    async fn current_limit(&self) -> u64 {
        *self.limit.read().await
    }
}

/// Token bucket rate limiter
///
/// This algorithm maintains a bucket of tokens. Each request consumes tokens,
/// and tokens are refilled at a constant rate. This allows for burst traffic
/// up to the bucket capacity, then steady-state traffic at the refill rate.
///
/// Example: 100 requests per minute with bucket size 10
/// - Can make 10 requests immediately (burst)
/// - Then 1 request per 0.6 seconds (steady refill rate)
#[derive(Debug)]
pub struct TokenBucketRateLimiter {
    capacity: RwLock<u64>,
    tokens: RwLock<f64>,
    refill_rate: f64, // tokens per second
    last_refill: RwLock<Instant>,
}

impl TokenBucketRateLimiter {
    /// Create a new token bucket rate limiter
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of tokens (burst capacity)
    /// * `refill_rate` - Number of tokens to add per second
    ///
    /// # Example
    /// ```
    /// // 100 requests per minute = 100/60 = 1.67 requests per second
    /// // With burst capacity of 10
    /// let limiter = TokenBucketRateLimiter::new(10, 100.0 / 60.0);
    /// ```
    pub fn new(capacity: u64, refill_rate: f64) -> Self {
        Self {
            capacity: RwLock::new(capacity),
            tokens: RwLock::new(capacity as f64),
            refill_rate,
            last_refill: RwLock::new(Instant::now()),
        }
    }

    /// Create from limit and interval (convenience constructor)
    ///
    /// # Arguments
    /// * `limit` - Maximum requests per interval
    /// * `interval` - Duration of the interval
    /// * `burst_multiplier` - Multiplier for burst capacity (default: 1.0, meaning no burst)
    ///
    /// # Example
    /// ```
    /// // 100 requests per minute, with 2x burst capacity
    /// let limiter = TokenBucketRateLimiter::from_limit(100, Duration::from_secs(60), 2.0);
    /// ```
    pub fn from_limit(limit: u64, interval: Duration, burst_multiplier: f64) -> Self {
        let capacity = (limit as f64 * burst_multiplier) as u64;
        let refill_rate = limit as f64 / interval.as_secs_f64();
        Self::new(capacity, refill_rate)
    }
}

#[async_trait::async_trait]
impl RateLimiter for TokenBucketRateLimiter {
    async fn check(&self, weight: u64) -> RateLimitResult {
        let now = Instant::now();
        let mut last_refill = self.last_refill.write().await;
        let elapsed = now.duration_since(*last_refill);

        // Refill tokens
        let mut tokens = self.tokens.write().await;
        let capacity = *self.capacity.read().await;
        let tokens_to_add = self.refill_rate * elapsed.as_secs_f64();
        *tokens = (*tokens + tokens_to_add).min(capacity as f64);
        *last_refill = now;

        let current_tokens = *tokens;
        let limit = capacity;

        if current_tokens >= weight as f64 {
            RateLimitResult::allowed((limit as f64 - current_tokens) as u64, limit)
        } else {
            // Calculate time until enough tokens are available
            let tokens_needed = weight as f64 - current_tokens;
            let seconds_to_wait = tokens_needed / self.refill_rate;
            let retry_after = Duration::from_secs_f64(seconds_to_wait);
            RateLimitResult::blocked((limit as f64 - current_tokens) as u64, limit, retry_after)
        }
    }

    async fn record(&self, weight: u64) {
        let now = Instant::now();
        let mut last_refill = self.last_refill.write().await;
        let elapsed = now.duration_since(*last_refill);

        // Refill tokens
        let mut tokens = self.tokens.write().await;
        let capacity = *self.capacity.read().await;
        let tokens_to_add = self.refill_rate * elapsed.as_secs_f64();
        *tokens = (*tokens + tokens_to_add).min(capacity as f64);
        *last_refill = now;

        // Consume tokens
        *tokens = (*tokens - weight as f64).max(0.0);
    }

    async fn update_limit(&self, limit: u64) {
        // Update capacity, but keep refill rate the same
        // This assumes the interval stays the same
        *self.capacity.write().await = limit;
    }

    async fn current_count(&self) -> u64 {
        let now = Instant::now();
        let mut last_refill = self.last_refill.write().await;
        let elapsed = now.duration_since(*last_refill);

        // Refill tokens
        let mut tokens = self.tokens.write().await;
        let capacity = *self.capacity.read().await;
        let tokens_to_add = self.refill_rate * elapsed.as_secs_f64();
        *tokens = (*tokens + tokens_to_add).min(capacity as f64);
        *last_refill = now;

        let current_tokens = *tokens;
        (capacity as f64 - current_tokens) as u64
    }

    async fn current_limit(&self) -> u64 {
        *self.capacity.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_fixed_window_basic() {
        let limiter = FixedWindowRateLimiter::new(10, Duration::from_secs(1));

        // First 10 requests should be allowed
        for i in 0..10 {
            let result = limiter.check(1).await;
            assert!(result.allowed, "Request {} should be allowed", i);
            limiter.record(1).await;
        }

        // 11th request should be blocked
        let result = limiter.check(1).await;
        assert!(!result.allowed, "11th request should be blocked");
        assert!(result.retry_after.is_some());
    }

    #[tokio::test]
    async fn test_fixed_window_reset() {
        let limiter = FixedWindowRateLimiter::new(5, Duration::from_millis(100));

        // Use all 5 requests
        for _ in 0..5 {
            let result = limiter.check(1).await;
            assert!(result.allowed);
            limiter.record(1).await;
        }

        // Wait for window to reset
        sleep(Duration::from_millis(150)).await;

        // Should be able to make requests again
        let result = limiter.check(1).await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_token_bucket_basic() {
        let limiter = TokenBucketRateLimiter::new(10, 1.0); // 1 token per second, capacity 10

        // First 10 requests should be allowed (burst)
        for i in 0..10 {
            let result = limiter.check(1).await;
            assert!(result.allowed, "Request {} should be allowed", i);
            limiter.record(1).await;
        }

        // 11th request should be blocked
        let result = limiter.check(1).await;
        assert!(!result.allowed, "11th request should be blocked");
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let limiter = TokenBucketRateLimiter::new(5, 1.0); // 1 token per second, capacity 5

        // Use all 5 tokens
        for _ in 0..5 {
            limiter.record(1).await;
        }

        // Wait for 1 token to refill
        sleep(Duration::from_millis(1100)).await;

        // Should be able to make 1 more request
        let result = limiter.check(1).await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_token_bucket_from_limit() {
        // 100 requests per minute = 100/60 = 1.67 requests per second
        let limiter = TokenBucketRateLimiter::from_limit(100, Duration::from_secs(60), 1.0);

        let limit = limiter.current_limit().await;
        assert_eq!(limit, 100);

        // Should be able to make requests up to capacity
        for _ in 0..100 {
            let result = limiter.check(1).await;
            assert!(result.allowed);
            limiter.record(1).await;
        }
    }
}
