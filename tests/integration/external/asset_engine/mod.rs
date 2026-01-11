//! Asset Engine External Integration Tests
//!
//! Tests external API calls for asset data retrieval.
//! Each exchange implements the same test suite to ensure consistent behavior.
//!
//! Run all: cargo test --test integration integration::external::asset_engine
//! Run specific exchange: cargo test --test integration integration::external::asset_engine::exchanges::binance

pub mod exchanges;
