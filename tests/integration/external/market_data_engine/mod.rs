//! Market Data Engine External Integration Tests
//!
//! Tests external WebSocket connections and data streaming.
//! Each exchange implements the same test suite to ensure consistent behavior.
//!
//! Run all: cargo test --test integration integration::external::market_data_engine -- --ignored
//! Run specific exchange:
//!   - Binance: cargo test --test integration integration::external::market_data_engine::exchanges::binance -- --ignored
//!   - Bybit: cargo test --test integration integration::external::market_data_engine::exchanges::bybit -- --ignored

pub mod exchanges;
