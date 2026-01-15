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

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use trading_core::rate_limiter::{FixedWindowRateLimiter, RateLimitResult, RateLimiter};
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

/// Binance rate limit structure from API responses
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceRateLimit {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub rate_limit_type: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub interval: SmolStr,
    pub interval_num: i64,
    pub limit: i64,
    pub count: i64,
}

/// Rate limit type for Binance API
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinanceRateLimitType {
    RequestWeight,
    Orders,
}

impl From<&SmolStr> for BinanceRateLimitType {
    fn from(s: &SmolStr) -> Self {
        match s.as_str() {
            "REQUEST_WEIGHT" => BinanceRateLimitType::RequestWeight,
            "ORDERS" => BinanceRateLimitType::Orders,
            _ => BinanceRateLimitType::RequestWeight, // Default fallback
        }
    }
}

/// Rate limit interval
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinanceRateLimitInterval {
    Second,
    Minute,
    Hour,
    Day,
}

impl From<&SmolStr> for BinanceRateLimitInterval {
    fn from(s: &SmolStr) -> Self {
        match s.as_str() {
            "SECOND" => BinanceRateLimitInterval::Second,
            "MINUTE" => BinanceRateLimitInterval::Minute,
            "HOUR" => BinanceRateLimitInterval::Hour,
            "DAY" => BinanceRateLimitInterval::Day,
            _ => BinanceRateLimitInterval::Minute, // Default fallback
        }
    }
}

impl BinanceRateLimitInterval {
    fn to_duration(self, interval_num: i64) -> Duration {
        let base = match self {
            BinanceRateLimitInterval::Second => Duration::from_secs(1),
            BinanceRateLimitInterval::Minute => Duration::from_secs(60),
            BinanceRateLimitInterval::Hour => Duration::from_secs(3600),
            BinanceRateLimitInterval::Day => Duration::from_secs(86400),
        };
        base * interval_num as u32
    }
}

/// Internal key for rate limit tracking
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RateLimitKey {
    rate_limit_type: BinanceRateLimitType,
    interval: BinanceRateLimitInterval,
    interval_num: i64,
}

/// Binance-specific rate limiter
pub struct BinanceRateLimiter {
    limiters: Arc<RwLock<HashMap<RateLimitKey, Arc<dyn RateLimiter + Send + Sync>>>>,
}

impl Debug for BinanceRateLimiter {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("BinanceRateLimiter")
    }
}

impl BinanceRateLimiter {
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

    /// Check if a request would exceed rate limits
    async fn check(&self, rate_limit_type: BinanceRateLimitType, weight: u64) -> RateLimitResult {
        let limiters = self.limiters.read().await;

        // Find all limiters matching this rate limit type
        let matching_limiters: Vec<_> = limiters
            .iter()
            .filter(|(key, _)| key.rate_limit_type == rate_limit_type)
            .map(|(_, limiter)| limiter.clone())
            .collect();

        drop(limiters);

        // Check all matching limiters - if any would block, return blocking result
        for limiter in &matching_limiters {
            let result = limiter.check(weight).await;
            if !result.allowed {
                return result;
            }
        }

        // All limiters allowed - return allowed result from first limiter
        if matching_limiters.is_empty() {
            // No limiters configured - allow
            RateLimitResult::allowed(0, u64::MAX)
        } else {
            // Return the first limiter's result (they all allowed, so any will do)
            matching_limiters[0].check(weight).await
        }
    }

    /// Wait if rate limit would be exceeded
    pub async fn wait_if_needed(&self, rate_limit_type: BinanceRateLimitType, weight: u64) {
        loop {
            let result = self.check(rate_limit_type, weight).await;

            if result.allowed {
                break;
            }
            if let Some(wait_time) = result.retry_after {
                tokio::time::sleep(wait_time).await;
            } else {
                // No retry_after but not allowed - should not happen, but break anyway
                break;
            }
        }
    }

    /// Record a request (consume rate limit quota)
    pub async fn record(&self, rate_limit_type: BinanceRateLimitType, weight: u64) {
        let limiters = self.limiters.read().await;

        // Record in all limiters matching this rate limit type
        for (key, limiter) in limiters.iter() {
            if key.rate_limit_type == rate_limit_type {
                limiter.record(weight).await;
            }
        }
    }

    /// Get current count for a rate limit type
    pub async fn current_count(&self, rate_limit_type: BinanceRateLimitType) -> u64 {
        let limiters = self.limiters.read().await;

        // Sum counts from all limiters matching this rate limit type
        let mut total = 0;
        for (key, limiter) in limiters.iter() {
            if key.rate_limit_type == rate_limit_type {
                total += limiter.current_count().await;
            }
        }

        total
    }

    /// Initialize default rate limits (from Binance documentation)
    pub async fn initialize_defaults(&self) {
        let default_limits = vec![
            BinanceRateLimit {
                rate_limit_type: SmolStr::new("REQUEST_WEIGHT"),
                interval: SmolStr::new("MINUTE"),
                interval_num: 1,
                limit: 1200,
                count: 0,
            },
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
                limit: 200000,
                count: 0,
            },
        ];

        self.update_from_response(&default_limits).await;
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
    use serde_json;

    #[tokio::test]
    async fn test_rate_limiter_creation() {
        let limiter = BinanceRateLimiter::new();
        limiter.initialize_defaults().await;

        // Should be able to check without panicking
        let result = limiter.check(BinanceRateLimitType::RequestWeight, 1).await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_rate_limiter_update() {
        let limiter = BinanceRateLimiter::new();

        let limits = vec![BinanceRateLimit {
            rate_limit_type: SmolStr::new("REQUEST_WEIGHT"),
            interval: SmolStr::new("MINUTE"),
            interval_num: 1,
            limit: 100,
            count: 0,
        }];

        limiter.update_from_response(&limits).await;

        let count = limiter
            .current_count(BinanceRateLimitType::RequestWeight)
            .await;
        assert_eq!(count, 0);
    }

    #[test]
    fn test_binance_rate_limit_serde() {
        let rate_limit = BinanceRateLimit {
            rate_limit_type: SmolStr::new("REQUEST_WEIGHT"),
            interval: SmolStr::new("MINUTE"),
            interval_num: 1,
            limit: 1200,
            count: 5,
        };

        let json = serde_json::to_string(&rate_limit).unwrap();
        let deserialized: BinanceRateLimit = serde_json::from_str(&json).unwrap();

        assert_eq!(rate_limit, deserialized);
    }
}
