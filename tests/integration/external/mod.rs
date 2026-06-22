//! External Integration Tests
//!
//! Tests that verify external connections to exchange APIs work correctly.
//! Each service (asset, market_data, exchange) has its own test suite,
//! with exchange-specific implementations in their exchanges/ subdirectories.
//!
//! **Important**: These tests ONLY verify external API calls work.
//! State management and internal logic are tested in unit tests.

pub mod asset_engine;
pub mod dex_engine;
pub mod exchange_engine;
pub mod market_data_engine;
