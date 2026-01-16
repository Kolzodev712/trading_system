//! Exchange-specific asset engine tests
//!
//! Each exchange implements the same test suite to ensure consistent behavior
//! across all exchange integrations.
//!
//! Run all exchange tests together:
//!   cargo test --test integration integration::external::asset_engine::exchanges -- --ignored
//!
//! This will run all tests from both Binance and Bybit modules.

pub mod binance;
pub mod bybit;
