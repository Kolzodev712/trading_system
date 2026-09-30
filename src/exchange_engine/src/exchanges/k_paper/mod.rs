//! k-paper paper exchange integration.
//!
//! Connects to k-paper WebSocket API and REST API. WebSocket for orders and live updates;
//! REST for reconciliation (balances, open_orders, trades). Market data is provided by
//! market_data_engine (separate connection).

pub mod client;
pub mod config;
pub mod driver;
pub mod map;
pub mod messages;
pub mod rest;

pub use client::{run_websocket_loop, KPaperClient, KPaperConfig, KPaperWebsocketLoop};
pub use config::PaperConfig;
pub use driver::PaperDriver;
pub use map::{parse_instrument_id, to_instrument_id};
pub use messages::KPaperMessage;
pub use rest::ws_url_to_http_base;
