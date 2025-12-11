//! Comprehensive integration tests for the full trading system
//!
//! These tests verify that all components work together correctly in a monolith setup.
//! Tests are organized by functionality area but run against the full system.
//!
//! Tests that require external API calls (exchange, network) are marked with #[ignore]
//! and can be run with: cargo test --test integration -- --ignored

mod integration {
    pub mod asset;
    pub mod channels;
    pub mod circuit_breaker;
    pub mod config;
    pub mod exchange;
    pub mod helpers;
    pub mod indicator;
    pub mod level_execution;
    pub mod levels;
    pub mod live_config;
    pub mod market_data;
    pub mod risk;
    pub mod risk_metrics;
    pub mod trading_state;
    pub mod workflows;
}
