use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::debug;

use trading_core::channels::market_data::MarketDataUpdate;
use trading_core::venue::dex::TradeCommand;

use dex_engine::state::DexState;

use super::super::r#trait::{DexStrategy, DexStrategyOutcome};
use super::super::types::DexStrategyKind;

/// Liquidation policy stub — evaluates `DexAccountHealth` MD; execution via `dex_engine`.
#[derive(Debug, Clone)]
pub struct LiquidationStrategy {
    pub min_health_factor: Decimal,
    pub max_liquidation_amount: Decimal,
}

impl Default for LiquidationStrategy {
    fn default() -> Self {
        Self {
            min_health_factor: dec!(0.995),
            max_liquidation_amount: dec!(1000),
        }
    }
}

impl DexStrategy for LiquidationStrategy {
    fn kind(&self) -> DexStrategyKind {
        DexStrategyKind::Liquidation
    }

    fn evaluate(
        &self,
        market_data: &MarketDataUpdate,
        _dex_state: &DexState,
    ) -> DexStrategyOutcome {
        let MarketDataUpdate::DexAccountHealth { health, .. } = market_data else {
            return DexStrategyOutcome::None;
        };
        if health.health_factor >= self.min_health_factor {
            return DexStrategyOutcome::None;
        }
        debug!(
            account = %health.account,
            health_factor = %health.health_factor,
            "liquidation candidate"
        );
        DexStrategyOutcome::Command(TradeCommand::Liquidate {
            protocol: health.protocol,
            account: health.account.clone(),
            max_amount: self.max_liquidation_amount,
            bonus_bps: None,
        })
    }
}
