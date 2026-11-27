//! Map in-process `trading_core` channel updates to external wire events.
//!
//! Single source of truth for the internal → `trading_networking` boundary (used by `runner::event_bridge`).

use trading_core::channels::market_data::MarketDataUpdate;
use trading_core::venue::cex::channels::CexVenueUpdate;
use trading_core::venue::dex::channels::DexVenueUpdate;

use super::{ColdEvent, DexColdEvent, HotEvent};

/// Convert market data update to a hot-lane event, if exportable.
pub fn hot_event_from_market_data(update: MarketDataUpdate) -> Option<HotEvent> {
    match update {
        MarketDataUpdate::Orderbook {
            orderbook,
            trace_context,
        } => Some(HotEvent::orderbook_from_structured(
            &orderbook,
            trace_context,
        )),
        MarketDataUpdate::Trade {
            trade,
            trace_context,
        } => Some(HotEvent::Trade {
            pair: trade.pair.clone(),
            price: trade.price,
            quantity: trade.quantity,
            timestamp: trade.timestamp,
            trade_id: trade.trade_id.clone(),
            trace_context,
        }),
        MarketDataUpdate::DexAccountHealth { .. } => None,
    }
}

/// Convert CEX venue update to cold-lane event.
pub fn cold_event_from_cex(update: CexVenueUpdate) -> ColdEvent {
    match update {
        CexVenueUpdate::Order {
            order,
            trace_context,
        } => ColdEvent::Order {
            order,
            trace_context,
        },
        CexVenueUpdate::Balance {
            balance,
            trace_context,
        } => ColdEvent::Balance {
            balance,
            trace_context,
        },
        CexVenueUpdate::Position {
            position,
            trace_context,
        } => ColdEvent::Position {
            position,
            trace_context,
        },
        CexVenueUpdate::Trade {
            trade,
            trace_context,
        } => ColdEvent::Trade {
            trade,
            trace_context,
        },
    }
}

/// Convert DEX venue update to dex-cold-lane event.
pub fn dex_cold_event_from_dex(update: DexVenueUpdate) -> DexColdEvent {
    match update {
        DexVenueUpdate::WalletBalance {
            balances,
            trace_context,
        } => DexColdEvent::WalletBalance {
            balances,
            trace_context,
        },
        DexVenueUpdate::OpenOrders {
            orders,
            trace_context,
        } => DexColdEvent::OpenOrders {
            orders,
            trace_context,
        },
        DexVenueUpdate::PendingTransaction {
            pending,
            trace_context,
        } => DexColdEvent::PendingTransaction {
            signature: pending.signature.clone(),
            status: pending.status,
            trace_context,
        },
        DexVenueUpdate::ExecutionResult {
            result,
            trace_context,
        } => DexColdEvent::ExecutionResult {
            result,
            trace_context,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use smol_str::SmolStr;
    use std::sync::Arc;
    use trading_core::types::common::Pair;
    use trading_core::types::market_data::StructuredOrderbook;
    use trading_core::venue::cex::types::exchange::Balance;
    use trading_core::venue::dex::types::{ExecutionResult, TxStatus};

    #[test]
    fn market_data_orderbook_maps_to_hot() {
        let ob = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            timestamp: 1,
            bids: vec![(Decimal::new(100, 0), Decimal::ONE)],
            asks: vec![(Decimal::new(101, 0), Decimal::ONE)],
            update_kind: Default::default(),
            sequence: None,
        };
        let hot = hot_event_from_market_data(MarketDataUpdate::Orderbook {
            orderbook: Arc::new(ob),
            trace_context: None,
        })
        .expect("orderbook");
        assert!(matches!(hot, HotEvent::Orderbook { .. }));
    }

    #[test]
    fn cex_balance_maps_to_cold() {
        let balance = Balance {
            symbol: "USDT".into(),
            free: Decimal::ONE,
            used: Decimal::ZERO,
            total: Decimal::ONE,
        };
        let cold = cold_event_from_cex(CexVenueUpdate::Balance {
            balance: balance.clone(),
            trace_context: None,
        });
        assert!(matches!(cold, ColdEvent::Balance { .. }));
    }

    #[test]
    fn dex_execution_maps_to_dex_cold() {
        let dex = dex_cold_event_from_dex(DexVenueUpdate::ExecutionResult {
            result: ExecutionResult {
                signature: SmolStr::new("sig"),
                status: TxStatus::Confirmed,
                error: None,
            },
            trace_context: None,
        });
        assert!(matches!(dex, DexColdEvent::ExecutionResult { .. }));
    }
}
