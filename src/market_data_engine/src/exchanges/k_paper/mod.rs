//! k-paper market data integration.
//! One WebSocket connection; only orderbook and public_trade are handled.

pub mod client;
pub mod config;
pub mod map;
pub mod messages;

pub use client::PaperClient;
pub use config::PaperConfig;
