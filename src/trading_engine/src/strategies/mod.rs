pub mod backtest;
pub mod executor;
pub mod persistence;
#[allow(clippy::module_inception)]
pub mod strategies;
pub mod strategy_manager;

pub use executor::StrategyExecutor;
pub use strategies::BuyLowSellHigh;
pub use strategy_manager::StrategyManager;
