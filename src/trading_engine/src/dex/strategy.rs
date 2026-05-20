use trading_core::channels::market_data::MarketDataUpdate;
use trading_core::venue::dex::TradeCommand;

use dex_engine::state::DexState;

/// DEX trading policy (liquidations, swaps). Distinct from CEX `levels/`.
pub trait DexStrategy: Send + Sync {
    fn evaluate(
        &self,
        market_data: &MarketDataUpdate,
        dex_state: &DexState,
    ) -> Option<TradeCommand>;
}
