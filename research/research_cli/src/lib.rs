//! Offline research command implementations.
//!
//! ```text
//! cargo run -p research_cli -- donchian-daily evaluate ...
//! cargo run -p research_cli -- btc-price-ma run ...
//! ```

pub mod btc_liquidation_shock;
pub mod btc_price_ma;
pub mod donchian_daily;
