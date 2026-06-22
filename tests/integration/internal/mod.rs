//! Internal Integration Tests
//!
//! These tests verify internal system interactions:
//! - Risk management, indicators, trading state, circuit breakers
//! - Level management, workflows, live config
//! - Strategy execution, channel communication
//!
//! These tests do NOT require external connections and test internal component integration.

pub mod channels;
pub mod circuit_breaker;
pub mod config;
pub mod dex;
pub mod futures;
pub mod indicator;
pub mod level_execution;
pub mod levels;
pub mod live_config;
pub mod risk;
pub mod risk_metrics;
pub mod strategy;
pub mod trading_state;
pub mod workflows;
