//! ObserveMid — reports orderbook mid for a configured market; no on-chain orders.

use rust_decimal::Decimal;
use trading_core::channels::market_data::MarketDataUpdate;

use dex_engine::state::DexState;

use super::super::r#trait::{DexStrategy, DexStrategyOutcome};
use super::super::types::{DexMarket, DexObserveSignal, DexStrategyKind};

/// Infrastructure / monitoring strategy: emits mid price observations only.
#[derive(Debug, Clone)]
pub struct ObserveMidStrategy {
    market: DexMarket,
}

impl ObserveMidStrategy {
    pub fn new(market: DexMarket) -> Self {
        Self { market }
    }
}

impl DexStrategy for ObserveMidStrategy {
    fn kind(&self) -> DexStrategyKind {
        DexStrategyKind::ObserveMid
    }

    fn evaluate(
        &self,
        market_data: &MarketDataUpdate,
        _dex_state: &DexState,
    ) -> DexStrategyOutcome {
        let Some(mid) = mid_price_for_market(market_data, &self.market) else {
            return DexStrategyOutcome::None;
        };
        DexStrategyOutcome::Observe(DexObserveSignal::MidPrice { mid })
    }
}

fn mid_price_for_market(md: &MarketDataUpdate, market: &DexMarket) -> Option<Decimal> {
    let MarketDataUpdate::Orderbook { orderbook, .. } = md else {
        return None;
    };
    if !market.matches_pair(&orderbook.pair) {
        return None;
    }
    let best_bid = orderbook.bids.first().map(|(p, _)| *p);
    let best_ask = orderbook.asks.first().map(|(p, _)| *p);
    match (best_bid, best_ask) {
        (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::TWO),
        (Some(bid), None) => Some(bid),
        (None, Some(ask)) => Some(ask),
        _ => None,
    }
}
