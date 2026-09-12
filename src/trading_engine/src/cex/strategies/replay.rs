//! Offline candle replay of `Strategy::evaluate`. See design log 011.
//!
//! Each candle becomes a one-tick book at `close`. That is enough to reject
//! junk rules, not to prove microstructure.

use crate::cex::intent::{intent_to_order_request, StrategyContext, StrategyIntent};
use crate::cex::state::{Strategy, TradingState};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trading_core::{
    error::EngineError,
    types::{
        common::{InstrumentType, Pair, Side},
        market_data::Orderbook,
    },
};

/// One OHLCV bar. Timestamps are unix milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candle {
    pub timestamp_ms: u64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayTrade {
    pub timestamp_ms: u64,
    pub side: Side,
    pub price: Decimal,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayResult {
    pub holds: usize,
    pub targets: usize,
    pub trades: Vec<ReplayTrade>,
    pub final_qty: Decimal,
}

/// Synthetic book: bid/ask one tick around close so mid ≈ close.
pub fn candle_to_book(pair: Pair, candle: &Candle) -> Orderbook {
    let tick = Decimal::new(1, 1);
    Orderbook {
        pair,
        bids: vec![(candle.close - tick, Decimal::ONE)],
        asks: vec![(candle.close + tick, Decimal::ONE)],
        timestamp: candle.timestamp_ms,
    }
}

/// Run `evaluate` on each candle. Fills (if any) apply at `close` into `account`.
pub async fn replay_candles(
    strategy: &mut dyn Strategy,
    candles: &[Candle],
    pair: &Pair,
    instrument_type: InstrumentType,
    account: &mut TradingState,
) -> Result<ReplayResult, EngineError> {
    let mut holds = 0usize;
    let mut targets = 0usize;
    let mut trades = Vec::new();

    for candle in candles {
        let orderbook = candle_to_book(pair.clone(), candle);
        let ctx = StrategyContext {
            orderbook: &orderbook,
            account,
            instrument_type: instrument_type.clone(),
        };
        let intent = strategy.evaluate(&ctx).await?;
        let current = account.effective_position_qty(pair);
        match &intent {
            StrategyIntent::Hold { .. } => {
                holds += 1;
                continue;
            }
            StrategyIntent::TargetPosition { .. } => targets += 1,
        }
        let Some(req) = intent_to_order_request(&intent, current, &orderbook) else {
            continue;
        };
        let signed = match req.side {
            Side::Buy => req.amount,
            Side::Sell => -req.amount,
        };
        account.update_position(pair.clone(), signed, candle.close);
        trades.push(ReplayTrade {
            timestamp_ms: candle.timestamp_ms,
            side: req.side,
            price: candle.close,
            quantity: req.amount,
        });
    }

    Ok(ReplayResult {
        holds,
        targets,
        trades,
        final_qty: account.effective_position_qty(pair),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cex::strategies::rust_strategies::PropHold;

    fn candles(closes: &[&str]) -> Vec<Candle> {
        closes
            .iter()
            .enumerate()
            .map(|(i, close)| Candle {
                timestamp_ms: (i as u64 + 1) * 60_000,
                open: close.parse().unwrap(),
                high: close.parse().unwrap(),
                low: close.parse().unwrap(),
                close: close.parse().unwrap(),
                volume: Decimal::ONE,
            })
            .collect()
    }

    #[tokio::test]
    async fn prop_hold_flat_history_does_not_trade() {
        let pair = Pair::new("BTC", "USDT");
        let mut strat = PropHold::new(pair.clone());
        let mut account = TradingState::new();
        let result = replay_candles(
            &mut strat,
            &candles(&["77000", "77100", "76900"]),
            &pair,
            InstrumentType::Futures,
            &mut account,
        )
        .await
        .unwrap();
        assert_eq!(result.trades.len(), 0);
        assert_eq!(result.final_qty, Decimal::ZERO);
        assert_eq!(result.holds, 3);
    }

    #[tokio::test]
    async fn prop_hold_leftover_flattens_once() {
        let pair = Pair::new("BTC", "USDT");
        let mut strat = PropHold::new(pair.clone());
        let mut account = TradingState::new();
        account.set_position(pair.clone(), Decimal::new(48, 3), Decimal::new(77000, 0));
        let result = replay_candles(
            &mut strat,
            &candles(&["77000", "77100"]),
            &pair,
            InstrumentType::Futures,
            &mut account,
        )
        .await
        .unwrap();
        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].side, Side::Sell);
        assert_eq!(result.trades[0].quantity, Decimal::new(48, 3));
        assert_eq!(result.final_qty, Decimal::ZERO);
        assert_eq!(result.holds, 1);
    }
}
