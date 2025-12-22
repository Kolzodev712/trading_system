//! Binance-specific rate limiter implementation
//!
//! This module implements rate limiting for Binance API according to their documentation:
//! https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/rate-limits
//!
//! Binance has multiple rate limit types:
//! - REQUEST_WEIGHT: Weight-based limits (IP-based, shared across connections)
//! - ORDERS: Order count limits (account-based, shared across API keys)
//!
//! Each rate limit can have multiple intervals (SECOND, MINUTE, HOUR, DAY).

use crate::exchanges::binance::websocket::types::common::BinanceRateLimit;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use trading_core::rate_limiter::{FixedWindowRateLimiter, RateLimitResult, RateLimiter};

/// Binance rate limit type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinanceRateLimitType {
    RequestWeight,
    Orders,
}

impl From<&str> for BinanceRateLimitType {
    fn from(s: &str) -> Self {
        match s {
            "REQUEST_WEIGHT" => BinanceRateLimitType::RequestWeight,
            "ORDERS" => BinanceRateLimitType::Orders,
            _ => BinanceRateLimitType::RequestWeight, // Default fallback
        }
    }
}

impl From<&SmolStr> for BinanceRateLimitType {
    fn from(s: &SmolStr) -> Self {
        Self::from(s.as_str())
    }
}

/// Binance rate limit interval
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinanceRateLimitInterval {
    Second,
    Minute,
    Hour,
    Day,
}

impl From<&str> for BinanceRateLimitInterval {
    fn from(s: &str) -> Self {
        match s {
            "SECOND" => BinanceRateLimitInterval::Second,
            "MINUTE" => BinanceRateLimitInterval::Minute,
            "HOUR" => BinanceRateLimitInterval::Hour,
            "DAY" => BinanceRateLimitInterval::Day,
            _ => BinanceRateLimitInterval::Minute, // Default fallback
        }
    }
}

impl From<&SmolStr> for BinanceRateLimitInterval {
    fn from(s: &SmolStr) -> Self {
        Self::from(s.as_str())
    }
}

impl BinanceRateLimitInterval {
    /// Convert to Duration
    pub fn to_duration(&self, interval_num: i64) -> Duration {
        let base = match self {
            BinanceRateLimitInterval::Second => Duration::from_secs(1),
            BinanceRateLimitInterval::Minute => Duration::from_secs(60),
            BinanceRateLimitInterval::Hour => Duration::from_secs(3600),
            BinanceRateLimitInterval::Day => Duration::from_secs(86400),
        };
        base * interval_num as u32
    }
}

/// Key for identifying a specific rate limit
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RateLimitKey {
    rate_limit_type: BinanceRateLimitType,
    interval: BinanceRateLimitInterval,
    interval_num: i64,
}

/// Binance rate limiter manager
///
/// Manages multiple rate limiters for different types and intervals.
/// Uses fixed window algorithm for all intervals as per Binance's behavior.
pub struct BinanceRateLimiter {
    limiters: Arc<RwLock<HashMap<RateLimitKey, Arc<dyn RateLimiter + Send + Sync>>>>,
}

impl Debug for BinanceRateLimiter {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("BinanceRateLimiter")
    }
}

impl BinanceRateLimiter {
    /// Create a new Binance rate limiter
    pub fn new() -> Self {
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update rate limits from API response
    ///
    /// This should be called whenever a response includes rateLimits field
    pub async fn update_from_response(&self, rate_limits: &[BinanceRateLimit]) {
        let mut limiters = self.limiters.write().await;

        for limit in rate_limits {
            let key = RateLimitKey {
                rate_limit_type: BinanceRateLimitType::from(&limit.rate_limit_type),
                interval: BinanceRateLimitInterval::from(&limit.interval),
                interval_num: limit.interval_num,
            };

            let duration = key.interval.to_duration(key.interval_num);

            // Create or update the limiter
            if let Some(limiter) = limiters.get(&key) {
                // Update existing limiter
                limiter.update_limit(limit.limit as u64).await;
            } else {
                // Create new limiter
                let limiter: Arc<dyn RateLimiter + Send + Sync> =
                    Arc::new(FixedWindowRateLimiter::new(limit.limit as u64, duration));
                limiters.insert(key, limiter);
            }
        }
    }

    /// Check if a request is allowed for a specific rate limit type
    ///
    /// This checks all intervals for the given rate limit type and returns
    /// the most restrictive result (the one that requires the longest wait).
    pub async fn check(
        &self,
        rate_limit_type: BinanceRateLimitType,
        weight: u64,
    ) -> RateLimitResult {
        let limiters = self.limiters.read().await;

        // Find all limiters for this rate limit type
        let relevant_limiters: Vec<_> = limiters
            .iter()
            .filter(|(key, _)| key.rate_limit_type == rate_limit_type)
            .collect();

        if relevant_limiters.is_empty() {
            // No limiters configured yet, allow the request
            return RateLimitResult::allowed(0, u64::MAX);
        }

        // Check all intervals and find the most restrictive
        let mut most_restrictive: Option<RateLimitResult> = None;

        for (_, limiter) in relevant_limiters {
            let result = limiter.check(weight).await;

            match &most_restrictive {
                None => most_restrictive = Some(result),
                Some(current) => {
                    if !result.allowed {
                        // If this result blocks, use it if it requires longer wait
                        if result.retry_after > current.retry_after {
                            most_restrictive = Some(result);
                        } else if current.allowed {
                            // Current allows but this blocks, use this
                            most_restrictive = Some(result);
                        }
                    } else if !current.allowed {
                        // Current blocks but this allows, keep current
                        // (don't change)
                    }
                }
            }
        }

        most_restrictive.unwrap_or_else(|| RateLimitResult::allowed(0, u64::MAX))
    }

    /// Wait if necessary before making a request
    pub async fn wait_if_needed(
        &self,
        rate_limit_type: BinanceRateLimitType,
        weight: u64,
    ) -> RateLimitResult {
        loop {
            let result = self.check(rate_limit_type.clone(), weight).await;
            if result.allowed {
                return result;
            }
            if let Some(duration) = result.retry_after {
                tokio::time::sleep(duration).await;
            } else {
                // No retry_after specified, wait a short time
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    /// Record that a request was made
    pub async fn record(&self, rate_limit_type: BinanceRateLimitType, weight: u64) {
        let limiters = self.limiters.read().await;

        // Record for all intervals of this rate limit type
        for (key, limiter) in limiters.iter() {
            if key.rate_limit_type == rate_limit_type {
                limiter.record(weight).await;
            }
        }
    }

    /// Get current usage for a rate limit type
    pub async fn current_count(&self, rate_limit_type: BinanceRateLimitType) -> u64 {
        let limiters = self.limiters.read().await;

        // Return the maximum count across all intervals (most restrictive)
        let mut max_count = 0u64;
        for (key, limiter) in limiters.iter() {
            if key.rate_limit_type == rate_limit_type {
                let count = limiter.current_count().await;
                max_count = max_count.max(count);
            }
        }
        max_count
    }

    /// Initialize with default Binance limits
    ///
    /// These are the typical limits from Binance's documentation.
    /// They will be updated from actual API responses.
    pub async fn initialize_defaults(&self) {
        let defaults = vec![
            // REQUEST_WEIGHT: 6000 per minute (IP-based)
            BinanceRateLimit {
                rate_limit_type: SmolStr::new("REQUEST_WEIGHT"),
                interval: SmolStr::new("MINUTE"),
                interval_num: 1,
                limit: 6000,
                count: 0,
            },
            // ORDERS: 50 per 10 seconds, 160000 per day (account-based)
            BinanceRateLimit {
                rate_limit_type: SmolStr::new("ORDERS"),
                interval: SmolStr::new("SECOND"),
                interval_num: 10,
                limit: 50,
                count: 0,
            },
            BinanceRateLimit {
                rate_limit_type: SmolStr::new("ORDERS"),
                interval: SmolStr::new("DAY"),
                interval_num: 1,
                limit: 160000,
                count: 0,
            },
        ];

        self.update_from_response(&defaults).await;
    }
}

impl Default for BinanceRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_creation() {
        let limiter = BinanceRateLimiter::new();
        limiter.initialize_defaults().await;

        // Check REQUEST_WEIGHT limit
        let result = limiter.check(BinanceRateLimitType::RequestWeight, 1).await;
        assert!(result.allowed);
        assert_eq!(result.limit, 6000);
    }

    #[tokio::test]
    async fn test_rate_limiter_update() {
        let limiter = BinanceRateLimiter::new();
        limiter.initialize_defaults().await;

        // Update with new limits
        let updates = vec![BinanceRateLimit {
            rate_limit_type: SmolStr::new("REQUEST_WEIGHT"),
            interval: SmolStr::new("MINUTE"),
            interval_num: 1,
            limit: 5000, // Reduced limit
            count: 100,
        }];

        limiter.update_from_response(&updates).await;

        let result = limiter.check(BinanceRateLimitType::RequestWeight, 1).await;
        assert!(result.allowed);
        assert_eq!(result.limit, 5000);
    }

    #[tokio::test]
    async fn test_multiple_intervals() {
        let limiter = BinanceRateLimiter::new();
        limiter.initialize_defaults().await;

        // ORDERS has both 10-second and 1-day limits
        let result = limiter.check(BinanceRateLimitType::Orders, 1).await;
        assert!(result.allowed);
    }
}
