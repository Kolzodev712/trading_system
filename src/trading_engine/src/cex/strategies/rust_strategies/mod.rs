//! Individual trading strategy implementations.

mod btc_price_ma_adapter;
mod btc_price_ma_long_flat_v1;
mod donchian;
mod low_high;
mod prop_hold;

pub use btc_price_ma_adapter::{
    BtcPriceMaLongFlatV1, BtcPriceMaLongFlatV1Config, EntryFreshness, Readiness, RecoveryState,
    RuntimeTarget,
};
pub use btc_price_ma_long_flat_v1::{
    experiment as btc_price_ma_experiment, gates as btc_price_ma_gates,
    offline as btc_price_ma_offline, position_qty, DIAGNOSTIC_LOOKBACKS, PRIMARY_LOOKBACK,
};
pub use donchian::Donchian;
pub use low_high::BuyLowSellHigh;
pub use prop_hold::PropHold;
pub use research_engine::donchian_daily_v1::aux as donchian_daily_aux;
pub use research_engine::donchian_daily_v1::backtest as donchian_daily_backtest;
pub use research_engine::donchian_daily_v1::offline as donchian_daily_offline;
