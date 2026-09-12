//! Strategy intent: desired exposure, not an exchange order.
//! See design log 008.

use rust_decimal::Decimal;
use trading_core::types::{
    common::{Instrument, InstrumentType, OrderType, Side},
    exchange::OrderRequest,
    market_data::Orderbook,
};

use crate::cex::state::TradingState;

pub type StrategyId = String;

/// Read-only inputs for one strategy evaluation.
pub struct StrategyContext<'a> {
    pub orderbook: &'a Orderbook,
    pub account: &'a TradingState,
    /// Venue instrument type from bot config (Spot or Futures). Strategies must not hardcode this.
    pub instrument_type: InstrumentType,
}

/// Desired exposure. Execution turns this into orders.
#[derive(Debug, Clone, PartialEq)]
pub enum StrategyIntent {
    /// No change (warmup, missing data, filters, unchanged).
    Hold { strategy_id: StrategyId },
    /// Signed target quantity. Zero means flat.
    TargetPosition {
        strategy_id: StrategyId,
        instrument: Instrument,
        qty: Decimal,
    },
}

impl StrategyIntent {
    pub fn hold(strategy_id: impl Into<StrategyId>) -> Self {
        Self::Hold {
            strategy_id: strategy_id.into(),
        }
    }

    pub fn target_position(
        strategy_id: impl Into<StrategyId>,
        instrument: Instrument,
        qty: Decimal,
    ) -> Self {
        Self::TargetPosition {
            strategy_id: strategy_id.into(),
            instrument,
            qty,
        }
    }

    pub fn strategy_id(&self) -> &StrategyId {
        match self {
            Self::Hold { strategy_id } | Self::TargetPosition { strategy_id, .. } => strategy_id,
        }
    }
}

/// `target − current`. Positive = buy, negative = sell.
pub fn position_delta(target_qty: Decimal, current_qty: Decimal) -> Decimal {
    target_qty - current_qty
}

/// Risk may reduce an unsigned cap on absolute target quantity.
pub fn apply_abs_qty_cap(target_qty: Decimal, max_abs_qty: Option<Decimal>) -> Decimal {
    let Some(cap) = max_abs_qty else {
        return target_qty;
    };
    if cap <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    target_qty.clamp(-cap, cap)
}

/// Per-strategy abs qty cap. Empty map means not configured. Unknown id fails closed (`0`).
pub fn strategy_abs_cap(
    strategy_id: &str,
    caps: &std::collections::HashMap<StrategyId, Decimal>,
) -> Option<Decimal> {
    if caps.is_empty() {
        return None;
    }
    Some(caps.get(strategy_id).copied().unwrap_or(Decimal::ZERO))
}

/// Clip a target to that strategy's configured `max_position`.
pub fn approve_target_qty(
    strategy_id: &str,
    target_qty: Decimal,
    caps: &std::collections::HashMap<StrategyId, Decimal>,
) -> Decimal {
    apply_abs_qty_cap(target_qty, strategy_abs_cap(strategy_id, caps))
}

/// Convert an approved target into a single limit `OrderRequest`, or `None` if already there.
pub fn intent_to_order_request(
    intent: &StrategyIntent,
    current_qty: Decimal,
    orderbook: &Orderbook,
) -> Option<OrderRequest> {
    let StrategyIntent::TargetPosition {
        instrument, qty, ..
    } = intent
    else {
        return None;
    };
    let delta = position_delta(*qty, current_qty);
    if delta.abs() <= Decimal::new(1, 8) {
        return None;
    }
    let (side, price) = if delta > Decimal::ZERO {
        (
            Side::Buy,
            orderbook
                .best_ask_price()
                .or_else(|| orderbook.best_bid_price())
                .unwrap_or(Decimal::ZERO),
        )
    } else {
        (
            Side::Sell,
            orderbook
                .best_bid_price()
                .or_else(|| orderbook.best_ask_price())
                .unwrap_or(Decimal::ZERO),
        )
    };
    if price <= Decimal::ZERO {
        return None;
    }
    Some(OrderRequest {
        instrument: Instrument::new(
            instrument.instrument_type().clone(),
            instrument.pair.clone(),
        ),
        side,
        order_type: OrderType::Limit,
        price,
        amount: delta.abs(),
        timestamp: None,
        exchange_params: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_long_short_flat_and_already_there() {
        assert_eq!(
            position_delta(Decimal::new(8, 1), Decimal::new(3, 1)),
            Decimal::new(5, 1)
        );
        assert_eq!(
            position_delta(Decimal::new(-5, 1), Decimal::new(2, 1)),
            Decimal::new(-7, 1)
        );
        assert_eq!(
            position_delta(Decimal::ZERO, Decimal::new(4, 1)),
            Decimal::new(-4, 1)
        );
        assert_eq!(
            position_delta(Decimal::new(2, 1), Decimal::new(2, 1)),
            Decimal::ZERO
        );
    }

    #[test]
    fn risk_reduces_target_abs() {
        assert_eq!(
            apply_abs_qty_cap(Decimal::new(5, 0), Some(Decimal::new(1, 0))),
            Decimal::new(1, 0)
        );
        assert_eq!(
            apply_abs_qty_cap(Decimal::new(-5, 0), Some(Decimal::new(1, 0))),
            Decimal::new(-1, 0)
        );
        assert_eq!(
            apply_abs_qty_cap(Decimal::new(5, 0), None),
            Decimal::new(5, 0)
        );
    }

    #[test]
    fn per_strategy_caps_are_independent_and_unknown_fails_closed() {
        let mut caps = std::collections::HashMap::new();
        caps.insert("A:BTC/USDT".into(), Decimal::new(1, 3));
        caps.insert("B:BTC/USDT".into(), Decimal::new(1, 2));
        assert_eq!(
            approve_target_qty("A:BTC/USDT", Decimal::new(5, 0), &caps),
            Decimal::new(1, 3)
        );
        assert_eq!(
            approve_target_qty("B:BTC/USDT", Decimal::new(-5, 0), &caps),
            Decimal::new(-1, 2)
        );
        assert_eq!(
            approve_target_qty("A:BTC/USDT", Decimal::new(5, 4), &caps),
            Decimal::new(5, 4)
        );
        assert_eq!(
            approve_target_qty("missing:BTC/USDT", Decimal::new(5, 0), &caps),
            Decimal::ZERO
        );
        assert_eq!(
            apply_abs_qty_cap(Decimal::ZERO, strategy_abs_cap("A:BTC/USDT", &caps)),
            Decimal::ZERO
        );
        let empty = std::collections::HashMap::new();
        assert_eq!(
            approve_target_qty("A:BTC/USDT", Decimal::new(5, 0), &empty),
            Decimal::new(5, 0)
        );
    }

    #[test]
    fn order_request_keeps_intent_instrument_type() {
        let pair = trading_core::types::common::Pair::new("BTC", "USDT");
        let intent = StrategyIntent::target_position(
            "BuyLowSellHigh:BTC/USDT",
            Instrument::new(InstrumentType::Futures, pair.clone()),
            Decimal::new(2, 3),
        );
        let book = Orderbook {
            pair,
            bids: vec![(Decimal::new(77000, 0), Decimal::ONE)],
            asks: vec![(Decimal::new(77010, 0), Decimal::ONE)],
            timestamp: 1,
        };
        let req = intent_to_order_request(&intent, Decimal::ZERO, &book).expect("delta");
        assert_eq!(req.instrument.instrument_type(), InstrumentType::Futures);
        assert_eq!(req.amount, Decimal::new(2, 3));
    }
}
