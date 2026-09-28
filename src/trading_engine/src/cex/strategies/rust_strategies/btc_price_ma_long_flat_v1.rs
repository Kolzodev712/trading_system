//! BTCPriceMALongFlatV1 research wrappers.
//!
//! Temporary re-export until research_cli owns the offline entry points.

#[allow(unused_imports)]
pub use research_engine::btc_price_ma_long_flat_v1::{
    decimal_ln, experiment, gates, geometric_mean, model_config_hash, offline, on_candle,
    position_qty, wants_long, Direction, ModelState, SignalDiagnostics, Step, ALLOW_SHORT,
    DIAGNOSTIC_LOOKBACKS, PERFORMANCE_END_EXCLUSIVE_MS, PERFORMANCE_START_MS, PRIMARY_LOOKBACK,
    SIGNAL_ID, STRATEGY_NAME, STRATEGY_VERSION,
};
