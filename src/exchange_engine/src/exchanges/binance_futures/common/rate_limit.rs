//! Binance Futures-specific rate limiter implementation
//!
//! This module implements rate limiting for Binance Futures API.
//! Reuses the same rate limiter structure as Spot but with futures-specific limits.

// For now, reuse the Spot rate limiter structure
// Futures-specific limits can be configured differently if needed
pub use crate::exchanges::binance::common::rate_limit::{
    BinanceRateLimit, BinanceRateLimitInterval, BinanceRateLimitType, BinanceRateLimiter,
};

// Alias for futures-specific usage
pub type BinanceFuturesRateLimiter = BinanceRateLimiter;
