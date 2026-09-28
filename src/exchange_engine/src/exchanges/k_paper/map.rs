//! Map k-paper wire types to trading_core types.

use rust_decimal::Decimal;
use smol_str::SmolStr;
use trading_core::types::common::{
    Instrument, InstrumentType, OrderId, OrderStatus, Pair, Side, Symbol,
};
use trading_core::types::exchange::{Balance, Order, OrderRequest, Position, Trade};
use trading_core::types::market_data::{StructuredOrderbook, StructuredPublicTrade};

use super::messages::{
    KPaperBalance, KPaperExecution, KPaperOrderbook, KPaperPosition, KPaperPublicTrade,
};

/// Parse k-paper instrument_id (e.g. "BTC-USDT") to Pair.
pub fn parse_instrument_id(s: &str) -> Option<Pair> {
    let parts: Vec<&str> = s.splitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
    }
    Some(Pair::new(parts[0], parts[1]))
}

/// Format Pair as k-paper instrument_id.
pub fn to_instrument_id(pair: &Pair) -> SmolStr {
    format!("{}-{}", pair.base, pair.quote).into()
}

/// ISO 8601 timestamp string to Unix seconds (u64). Best-effort.
pub fn timestamp_iso_to_u64_secs(s: &str) -> u64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp() as u64)
        .unwrap_or(0)
}

fn side_from_str(s: &str) -> Side {
    if s.eq_ignore_ascii_case("buy") {
        Side::Buy
    } else {
        Side::Sell
    }
}

/// Convert k-paper orderbook to StructuredOrderbook.
pub fn orderbook_to_structured(ob: &KPaperOrderbook) -> Option<StructuredOrderbook> {
    let pair = parse_instrument_id(ob.instrument_id.as_str())?;
    let bids: Vec<(Decimal, Decimal)> = ob.bids.iter().map(|l| (l.price, l.quantity)).collect();
    let asks: Vec<(Decimal, Decimal)> = ob.asks.iter().map(|l| (l.price, l.quantity)).collect();
    let timestamp = timestamp_iso_to_u64_secs(ob.timestamp.as_str());
    Some(StructuredOrderbook {
        pair,
        bids,
        asks,
        timestamp,
        update_kind: trading_core::types::market_data::OrderbookUpdateKind::Snapshot,
        sequence: None,
    })
}

/// Convert k-paper public trade to StructuredPublicTrade.
pub fn public_trade_to_structured(t: &KPaperPublicTrade) -> Option<StructuredPublicTrade> {
    let pair = parse_instrument_id(t.instrument_id.as_str())?;
    Some(StructuredPublicTrade {
        pair,
        price: t.price,
        quantity: t.quantity,
        timestamp: timestamp_iso_to_u64_secs(t.timestamp.as_str()),
        is_market_maker: false,
        trade_id: Some(t.trade_id.clone().into()),
    })
}

/// Build Order from k-paper Execution ack (order accepted, quantity=0).
/// Uses requested amount from the place_order request; status is New.
pub fn execution_ack_to_order(
    exec: &KPaperExecution,
    req: &OrderRequest,
    instrument_type: InstrumentType,
) -> Option<Order> {
    let pair = parse_instrument_id(exec.instrument_id.as_str())?;
    let instrument = Instrument::new(instrument_type, pair);
    let price = exec.price;
    let timestamp_secs = timestamp_iso_to_u64_secs(exec.timestamp.as_str());
    let timestamp_ms = timestamp_secs.saturating_mul(1000);
    Some(Order {
        order_id: OrderId::new(exec.order_id.clone().into()),
        client_order_id: req.exchange_params.get("orderLinkId").cloned(),
        instrument: instrument.clone(),
        side: side_from_str(exec.side.as_str()),
        price,
        amount: req.amount,
        internal_created_at: timestamp_ms,
        exchange_created_at: Some(timestamp_ms),
        status: OrderStatus::New,
        filled_amount: Decimal::ZERO,
    })
}

/// Apply an incremental fill to a working order. Paper fill events carry only the
/// last fill quantity; they are not themselves a terminal `Filled` status.
pub fn merge_fill_into_working_order(existing: &Order, fill_qty: Decimal) -> Order {
    let filled = existing.filled_amount + fill_qty;
    let status = if filled >= existing.amount {
        OrderStatus::Filled
    } else {
        OrderStatus::PartiallyFilled
    };
    Order {
        filled_amount: filled,
        status,
        ..existing.clone()
    }
}

/// Convert k-paper execution to Order (for CexVenueUpdate::Order).
/// Rejects stay rejected. A lone fill event is not enough to mark the full order
/// terminal; merge it with the working order via [`merge_fill_into_working_order`].
pub fn execution_to_order(
    exec: &KPaperExecution,
    instrument_type: InstrumentType,
) -> Option<Order> {
    let pair = parse_instrument_id(exec.instrument_id.as_str())?;
    let instrument = Instrument::new(instrument_type, pair);
    let price = exec.price;
    let quantity = exec.quantity;
    let status = if exec
        .reject_reason
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false)
    {
        OrderStatus::Rejected
    } else {
        OrderStatus::Filled
    };
    // Store milliseconds so UI 24h filter (hours_ago in ms) includes the order
    let timestamp_secs = timestamp_iso_to_u64_secs(exec.timestamp.as_str());
    let timestamp_ms = timestamp_secs.saturating_mul(1000);
    Some(Order {
        order_id: OrderId::new(exec.order_id.clone().into()),
        client_order_id: None,
        instrument: instrument.clone(),
        side: side_from_str(exec.side.as_str()),
        price,
        amount: quantity,
        internal_created_at: timestamp_ms,
        exchange_created_at: Some(timestamp_ms),
        status,
        filled_amount: quantity,
    })
}

/// Convert k-paper execution to Trade (for CexVenueUpdate::Trade).
pub fn execution_to_trade(exec: &KPaperExecution) -> Option<Trade> {
    let pair = parse_instrument_id(exec.instrument_id.as_str())?;
    let trade_id = exec.trade_id.as_deref().unwrap_or("").to_string().into();
    let timestamp_secs = timestamp_iso_to_u64_secs(exec.timestamp.as_str());
    let timestamp_ms = timestamp_secs.saturating_mul(1000);
    Some(Trade {
        trade_id,
        order_id: OrderId::new(exec.order_id.clone().into()),
        pair,
        price: exec.price,
        quantity: exec.quantity,
        side: side_from_str(exec.side.as_str()),
        timestamp: timestamp_ms,
        fee: exec.fee.unwrap_or(Decimal::ZERO),
        fee_currency: Symbol::from("USDT"),
    })
}

/// Convert k-paper balance to Balance.
pub fn balance_to_core(b: &KPaperBalance) -> Balance {
    Balance {
        symbol: Symbol::from(b.asset.as_str()),
        free: b.available,
        used: b.locked,
        total: b.total,
    }
}

/// Convert k-paper position to Position (spot-style: quantity and entry_price).
pub fn position_to_core(pos: &KPaperPosition, instrument_type: InstrumentType) -> Option<Position> {
    let pair = parse_instrument_id(pos.instrument_id.as_str())?;
    let instrument = Instrument::new(instrument_type, pair);
    let quantity = pos.quantity;
    let entry_price = pos.entry_price;
    let unrealized_pnl = pos.unrealized_pnl.unwrap_or(Decimal::ZERO);
    Some(Position {
        instrument,
        quantity,
        avg_price: entry_price,
        current_price: entry_price,
        unrealized_pnl,
        last_update: 0,
        mark_price: None,
        index_price: None,
        realized_pnl: None,
        initial_margin: None,
        maintenance_margin: None,
        leverage: None,
        position_mode: None,
        margin_type: None,
        liquidation_price: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::types::common::Instrument;

    #[test]
    fn parse_instrument_id_ok() {
        assert_eq!(
            parse_instrument_id("BTC-USDT"),
            Some(Pair::new("BTC", "USDT"))
        );
        assert_eq!(
            parse_instrument_id("ETH-USDT"),
            Some(Pair::new("ETH", "USDT"))
        );
    }

    #[test]
    fn parse_instrument_id_invalid() {
        assert_eq!(parse_instrument_id("BTC"), None);
        assert_eq!(parse_instrument_id(""), None);
    }

    #[test]
    fn to_instrument_id_roundtrip() {
        let pair = Pair::new("BTC", "USDT");
        let id = to_instrument_id(&pair);
        assert_eq!(id.as_str(), "BTC-USDT");
        assert_eq!(parse_instrument_id(&id), Some(pair));
    }

    #[test]
    fn balance_to_core_ok() {
        let b = KPaperBalance {
            asset: "USDT".to_string(),
            available: Decimal::new(1000, 0),
            locked: Decimal::new(100, 0),
            total: Decimal::new(1100, 0),
        };
        let core = balance_to_core(&b);
        assert_eq!(core.symbol, Symbol::from("USDT"));
        assert_eq!(core.free, Decimal::new(1000, 0));
        assert_eq!(core.used, Decimal::new(100, 0));
        assert_eq!(core.total, Decimal::new(1100, 0));
    }

    #[test]
    fn execution_ack_to_order_ok() {
        use trading_core::types::exchange::OrderRequest;
        let exec = KPaperExecution {
            order_id: "ord-1".to_string(),
            trade_id: None,
            instrument_id: "BTC-USDT".to_string(),
            side: "Buy".to_string(),
            price: Decimal::new(50000, 0),
            quantity: Decimal::ZERO,
            fee: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            reject_reason: None,
            client_order_id: None,
        };
        let req = OrderRequest {
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: trading_core::types::common::Side::Buy,
            order_type: trading_core::types::common::OrderType::Limit,
            price: Decimal::new(50000, 0),
            amount: Decimal::new(2, 3),
            timestamp: None,
            exchange_params: std::collections::HashMap::new(),
        };
        let order = execution_ack_to_order(&exec, &req, InstrumentType::Spot).unwrap();
        assert_eq!(order.order_id.as_str(), "ord-1");
        assert_eq!(order.instrument.pair, Pair::new("BTC", "USDT"));
        assert_eq!(order.status, OrderStatus::New);
        assert_eq!(order.amount, Decimal::new(2, 3));
    }

    #[test]
    fn merge_fill_keeps_partial_remainder() {
        let existing = Order {
            order_id: OrderId::new("ord-p".into()),
            client_order_id: Some(SmolStr::new("cid-p")),
            instrument: Instrument::new(InstrumentType::Futures, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(49000, 0),
            amount: Decimal::new(2, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };
        let partial = merge_fill_into_working_order(&existing, Decimal::new(1, 3));
        assert_eq!(partial.status, OrderStatus::PartiallyFilled);
        assert_eq!(partial.filled_amount, Decimal::new(1, 3));
        assert_eq!(partial.amount, Decimal::new(2, 3));
        let filled = merge_fill_into_working_order(&partial, Decimal::new(1, 3));
        assert_eq!(filled.status, OrderStatus::Filled);
        assert_eq!(filled.filled_amount, Decimal::new(2, 3));
    }

    #[test]
    fn execution_to_trade_copies_paper_fee() {
        let exec = KPaperExecution {
            order_id: "ord-fill".to_string(),
            trade_id: Some("paper-exec-9".to_string()),
            instrument_id: "BTC-USDT".to_string(),
            side: "Buy".to_string(),
            price: Decimal::new(100, 0),
            quantity: Decimal::new(1, 2),
            fee: Some(Decimal::new(6, 2)),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            reject_reason: None,
            client_order_id: None,
        };
        let trade = execution_to_trade(&exec).unwrap();
        assert_eq!(trade.trade_id.as_str(), "paper-exec-9");
        assert_eq!(trade.fee, Decimal::new(6, 2));
        assert_eq!(trade.fee_currency, Symbol::from("USDT"));
        assert_eq!(trade.quantity, Decimal::new(1, 2));
    }
}
