//! k-paper paper exchange integration.
//!
//! Connects to k-paper WebSocket API, maps messages to trading_core types,
//! and implements TradingClient. Market data is provided by market_data_engine (separate connection).

pub mod client;
pub mod config;
pub mod driver;
pub mod map;
pub mod messages;

pub use client::{run_websocket_loop, KPaperClient, KPaperConfig};
pub use config::PaperConfig;
pub use driver::PaperDriver;
pub use map::{parse_instrument_id, to_instrument_id};
pub use messages::KPaperMessage;
