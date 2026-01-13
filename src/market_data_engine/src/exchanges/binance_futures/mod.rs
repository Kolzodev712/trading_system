//! Binance Futures Market Data Client
//!
//! This module implements the Binance USD Futures market data integration.
//! Handles mark price, funding rate, index price, and open interest streams.

pub mod client;
pub mod config;
pub mod websocket;

// Re-export main types (will be added as implementation progresses)
// pub use config::BinanceFuturesMarketDataConfig;
// pub use client::BinanceFuturesMarketDataClient;
