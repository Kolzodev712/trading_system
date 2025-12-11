//! Comprehensive integration tests for the full trading system
//!
//! These tests verify that all components work together correctly in a monolith setup.
//! Tests are organized by functionality area but run against the full system.
//!
//! Tests that require external API calls (exchange, network) are marked with #[ignore]
//! and can be run with: cargo test --test integration -- --ignored

pub mod helpers;
pub mod config;
pub mod levels;
pub mod risk;
pub mod trading_state;
pub mod circuit_breaker;
pub mod market_data;
pub mod indicator;
pub mod asset;
pub mod exchange;
pub mod live_config;
pub mod channels;
pub mod workflows;
pub mod risk_metrics;
pub mod level_execution;

