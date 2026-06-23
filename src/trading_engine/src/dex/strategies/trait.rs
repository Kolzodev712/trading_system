use trading_core::channels::market_data::MarketDataUpdate;
use trading_core::venue::dex::TradeCommand;

use dex_engine::state::DexState;

use super::types::{DexObserveSignal, DexStrategyKind};

/// Result of one strategy evaluation tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DexStrategyOutcome {
    /// No update for this market / data type.
    None,
    /// Observability only — no on-chain command.
    Observe(DexObserveSignal),
    /// Submit an on-chain command via `DexEngineRuntime`.
    Command(TradeCommand),
}

/// DEX trading policy — distinct from CEX `strategies/`. Implementations must stay
/// venue-agnostic (no chain- or DeepBook-specific types).
pub trait DexStrategy: Send + Sync {
    fn kind(&self) -> DexStrategyKind;

    fn evaluate(&self, market_data: &MarketDataUpdate, dex_state: &DexState) -> DexStrategyOutcome;
}
