mod executor;
mod factory;
mod manager_wrapper;
pub mod replay;
mod rust_strategies;
pub mod strategy_manager;

pub use crate::cex::intent::{StrategyContext, StrategyIntent};
pub use executor::StrategyExecutor;
pub use factory::{
    build_strategy, build_strategy_composed, ExtraStrategyFactory, StrategyFactoryArgs,
    UNSUPPORTED_CANDIDATE_2,
};
pub use manager_wrapper::StrategyManagerWrapper;
pub use rust_strategies::{BuyLowSellHigh, Donchian, PropHold};
pub use strategy_manager::StrategyManager;
