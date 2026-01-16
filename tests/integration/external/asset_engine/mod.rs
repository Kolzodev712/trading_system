//! Asset Engine External Integration Tests
//!
//! Tests external API calls for asset data retrieval.
//! Each exchange implements the same test suite to ensure consistent behavior.
//!
//! Run all exchange tests together (recommended):
//!   cargo test --test integration integration::external::asset_engine::exchanges -- --ignored
//!
//! Run all asset engine tests (includes exchanges):
//!   cargo test --test integration integration::external::asset_engine -- --ignored
//!
//! Run all tests for a specific exchange:
//!   cargo test --test integration integration::external::asset_engine::exchanges::binance -- --ignored
//!   cargo test --test integration integration::external::asset_engine::exchanges::bybit -- --ignored
//!
//! Run a specific test:
//!   cargo test --test integration integration::external::asset_engine::exchanges::binance::test_fetch_market_data -- --ignored

pub mod exchanges;
