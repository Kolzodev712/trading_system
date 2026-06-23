//! CEX trading engines — levels and strategies mirror [`crate::dex`] layout.

pub mod levels;
pub mod state;
pub mod strategies;

pub use levels::{
    CancellationManager, IndicatorSubscriber, Level, LevelBasedEventLoop, LevelConfig,
    LevelExecutor, LevelManager, LevelState,
};
pub use state::manager::TradingStateManager;
pub use state::{Strategy, TradingState};
pub use strategies::{BuyLowSellHigh, StrategyExecutor, StrategyManager, StrategyManagerWrapper};
