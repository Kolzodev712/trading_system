//! Offline research command implementations.
//!
//! The live runner still exposes the previous binaries as thin wrappers. Prefer:
//!
//! ```text
//! cargo run -p research_cli -- donchian-daily evaluate ...
//! cargo run -p research_cli -- btc-price-ma run ...
//! ```

pub mod btc_price_ma;
pub mod donchian_daily;
