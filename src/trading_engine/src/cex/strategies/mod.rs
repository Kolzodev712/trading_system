mod executor;
mod factory;
mod manager_wrapper;
pub mod replay;
mod rust_strategies;
pub mod strategy_manager;

pub use crate::cex::intent::{StrategyContext, StrategyIntent};
pub use executor::StrategyExecutor;
pub use factory::{build_strategy, StrategyFactoryArgs};
pub use manager_wrapper::StrategyManagerWrapper;
pub use rust_strategies::{
    btc_price_ma_experiment, btc_price_ma_gates, btc_price_ma_offline, donchian_daily_aux,
    donchian_daily_backtest, donchian_daily_offline, position_qty, BuyLowSellHigh, Donchian,
    PropHold, DIAGNOSTIC_LOOKBACKS, PRIMARY_LOOKBACK,
};
pub use strategy_manager::StrategyManager;
