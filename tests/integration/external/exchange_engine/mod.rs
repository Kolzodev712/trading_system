//! Exchange Engine External Integration Tests
//!
//! Tests external trading API calls (orders, balances, positions, etc.).
//! Each exchange implements the same test suite to ensure consistent behavior.
//!
//! Run all: cargo test --test integration integration::external::exchange_engine
//! Run specific exchange: cargo test --test integration integration::external::exchange_engine::exchanges::binance

pub mod exchanges;
