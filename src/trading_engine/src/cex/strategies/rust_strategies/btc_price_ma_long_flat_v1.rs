//! BTCPriceMALongFlatV1 research wrappers.
//!
//! The pure model lives in `strategy_core`. Historical replay, scoring, and
//! gates remain here until research_engine is extracted.

pub mod experiment;
pub mod gates;
pub mod offline;

#[allow(unused_imports)] // keep the extracted model names reachable from this module
pub use strategy_core::btc_price_ma_long_flat_v1::{
    config_hash as model_config_hash, decimal_ln, geometric_mean, on_candle, position_qty,
    wants_long, Direction, ModelState, SignalDiagnostics, Step, ALLOW_SHORT, DIAGNOSTIC_LOOKBACKS,
    PRIMARY_LOOKBACK, SIGNAL_ID, STRATEGY_NAME, STRATEGY_VERSION,
};

pub const PERFORMANCE_START_MS: u64 = 1_674_259_200_000;
pub const PERFORMANCE_END_EXCLUSIVE_MS: u64 = 1_767_225_600_000;
