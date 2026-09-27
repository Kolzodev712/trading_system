//! Live slot with no edge: stay flat, or flatten leftover qty.
//! See design log 011.

use crate::cex::intent::{StrategyContext, StrategyIntent};
use crate::cex::state::Strategy;
use crate::cex::strategies::strategy_manager::StrategyManager;
use async_trait::async_trait;
use rust_decimal::Decimal;
use tracing::info;
use trading_core::{
    error::EngineError,
    types::{
        common::{Instrument, Pair},
        exchange::Order,
    },
};

/// Qty at or below this is treated as flat (same epsilon as flatten / TSM).
pub(crate) const FLAT_EPS: Decimal = Decimal::from_parts(1, 0, 0, false, 6);

pub struct PropHold {
    symbol: Pair,
}

impl PropHold {
    pub fn new(symbol: Pair) -> Self {
        Self { symbol }
    }

    fn strategy_id(&self) -> String {
        StrategyManager::make_strategy_id("PropHold", &self.symbol)
    }
}

#[async_trait]
impl Strategy for PropHold {
    async fn evaluate(&mut self, ctx: &StrategyContext<'_>) -> Result<StrategyIntent, EngineError> {
        let current = ctx.account.effective_position_qty(&self.symbol);
        if current.abs() <= FLAT_EPS {
            return Ok(StrategyIntent::hold(self.strategy_id()));
        }
        info!(
            strategy_id = %self.strategy_id(),
            current = %current,
            "PropHold flattening leftover qty"
        );
        Ok(StrategyIntent::target_position(
            self.strategy_id(),
            Instrument::new(ctx.instrument_type.clone(), self.symbol.clone()),
            Decimal::ZERO,
        ))
    }

    async fn on_execution_report(&mut self, _order: &Order) -> Result<(), EngineError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cex::intent::StrategyContext;
    use crate::cex::state::TradingState;
    use trading_core::types::{common::InstrumentType, market_data::Orderbook};

    fn book(pair: Pair) -> Orderbook {
        Orderbook {
            pair,
            bids: vec![(Decimal::new(77000, 0), Decimal::ONE)],
            asks: vec![(Decimal::new(77010, 0), Decimal::ONE)],
            timestamp: 1,
            update_kind: Default::default(),
            sequence: None,
        }
    }

    #[tokio::test]
    async fn flat_account_holds() {
        let pair = Pair::new("BTC", "USDT");
        let mut strat = PropHold::new(pair.clone());
        let account = TradingState::new();
        let orderbook = book(pair);
        let ctx = StrategyContext {
            orderbook: &orderbook,
            account: &account,
            instrument_type: InstrumentType::Futures,
        };
        let intent = strat.evaluate(&ctx).await.unwrap();
        assert!(matches!(intent, StrategyIntent::Hold { .. }));
    }

    #[tokio::test]
    async fn leftover_targets_zero() {
        let pair = Pair::new("BTC", "USDT");
        let mut strat = PropHold::new(pair.clone());
        let mut account = TradingState::new();
        account.set_position(pair.clone(), Decimal::new(48, 3), Decimal::new(77000, 0));
        let orderbook = book(pair);
        let ctx = StrategyContext {
            orderbook: &orderbook,
            account: &account,
            instrument_type: InstrumentType::Futures,
        };
        match strat.evaluate(&ctx).await.unwrap() {
            StrategyIntent::TargetPosition {
                qty, instrument, ..
            } => {
                assert_eq!(qty, Decimal::ZERO);
                assert_eq!(instrument.instrument_type(), InstrumentType::Futures);
            }
            other => panic!("expected TargetPosition 0, got {other:?}"),
        }
    }
}
