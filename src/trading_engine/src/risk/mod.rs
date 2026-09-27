//! Risk management system for the trading engine
//!
//! This module provides comprehensive risk management that applies to both
//! levels-based and strategy-based trading modes.

pub mod circuit_breaker;
pub mod limits;
pub mod manager;
pub mod metrics;

pub use limits::{RiskCheckResult, RiskLimits};
pub use manager::{check_order_balance, order_increases_exposure, RiskManager};
pub use metrics::{DailyPnLTracker, RiskMetrics, RiskMetricsCalculator};
