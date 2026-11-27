//! Component integration tests (in-memory).
//!
//! These tests verify internal system interactions:
//! - Risk management, indicators, trading state, circuit breakers
//! - Level management, component workflows, live config
//! - Strategy execution, channel communication
//!
//! They do **not** launch a runner, k-paper, gRPC control plane, journal, or ledger.
//! A pass here is not Paper/Demo/live qualification.

pub mod channels;
pub mod circuit_breaker;
pub mod config;
#[cfg(feature = "dex")]
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
