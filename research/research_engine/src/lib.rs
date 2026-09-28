//! Historical research and economic evaluation.
//!
//! This crate must not depend on `trading_engine`. Pure Candidate #2 signal
//! logic lives in `strategy_core`; this crate owns datasets, replay, simulation,
//! gates, and public-data loaders.

pub mod btc_price_ma_long_flat_v1;
pub mod bybit_public_klines;
pub mod bybit_public_market;
pub mod donchian_daily_v1;
