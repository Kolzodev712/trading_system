//! CEX trading engines — levels and strategies mirror [`crate::dex`] layout.

pub mod completed_candles;
pub mod intent;
pub mod levels;
pub mod shadow;
pub mod state;
pub mod strategies;

pub use levels::{
    expanded_level_id, CancellationManager, IndicatorSubscriber, Level, LevelBasedEventLoop,
    LevelConfig, LevelExecutor, LevelIdentity, LevelManager, LevelState, LevelsCatalog,
};
pub use state::manager::TradingStateManager;
pub use state::{Strategy, TradingState};
pub use strategies::{
    BuyLowSellHigh, PropHold, StrategyExecutor, StrategyManager, StrategyManagerWrapper,
};
