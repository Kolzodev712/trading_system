//! Integration Tests
//!
//! Split into two categories:
//!
//! ## External Integration Tests (`integration/external/`)
//! Tests that verify external connections work correctly:
//! - Asset Engine: Exchange info fetching, market data retrieval
//! - Market Data Engine: WebSocket connections, data streaming
//! - Exchange Engine: Trading API calls, order management
//!
//! These tests may require actual exchange API access (marked with #[ignore])
//! Run with: cargo test --test integration integration::external -- --ignored
//!
//! ## Internal Integration Tests (`integration/internal/`)
//! Tests that verify internal system interactions:
//! - Risk management, indicators, trading state
//! - Level management, workflows, live config
//! - Strategy execution, channel communication
//!
//! These tests do NOT require external connections and test internal component integration.
//! Run with: cargo test --test integration integration::internal

mod integration {
    pub mod external;
    pub mod helpers;
    pub mod internal;
}

// Re-export for easier access
pub use integration::*;
