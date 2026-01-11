//! Market Data Engine External Integration Tests
//!
//! Tests external WebSocket connections and data streaming.
//! Each exchange implements the same test suite to ensure consistent behavior.
//!
//! Run all: cargo test --test integration integration::external::market_data_engine
//! Run specific exchange: cargo test --test integration integration::external::market_data_engine::exchanges::binance

pub mod exchanges;
