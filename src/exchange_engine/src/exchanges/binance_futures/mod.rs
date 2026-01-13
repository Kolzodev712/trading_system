//! Binance USD Futures Exchange Driver
//!
//! This module implements the Binance USD Futures integration for the Exchange Engine.
//! It follows the same structure as the Binance Spot integration but uses different
//! endpoints and handles futures-specific features (positions, leverage, margin, etc.).
//!
//! See: `proposals/02-binance-usd-futures-integration.md` for detailed implementation plan.

pub mod common;
pub mod config;
pub mod driver;
pub mod event_loop;
pub mod rest;
pub mod websocket;

// Re-export main types (will be added as implementation progresses)
// pub use config::BinanceFuturesConfig;
// pub use driver::BinanceFuturesDriver;
// pub use rest::client::BinanceFuturesHttpClient;
// pub use websocket::ws_client::BinanceFuturesClient;
