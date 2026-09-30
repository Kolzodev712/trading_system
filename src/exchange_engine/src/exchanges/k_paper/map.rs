//! Map k-paper wire types to trading_core types.

use std::collections::HashSet;

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
        client_order_id: execution_client_order_id(exec).map(SmolStr::new),
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

/// Record a fill trade id. Returns false when this id was already applied.
/// Empty trade ids cannot be deduplicated and always apply.
pub fn fill_trade_id_is_new(applied: &mut HashSet<String>, trade_id: Option<&str>) -> bool {
    match trade_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => applied.insert(id.to_string()),
        None => true,
    }
}

/// `cancel_all` may attempt local cancels after a REST failure, but the outcome
/// stays unverified: an empty local set is exactly when REST was required.
pub fn cancel_all_after_rest(
    rest_ok: bool,
    first_cancel_err: Option<String>,
) -> Result<(), String> {
    if !rest_ok {
        return Err("CancelWorking unverified: REST GET /open_orders failed".into());
    }
    match first_cancel_err {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Client id from a Paper execution, if the venue included one.
pub fn execution_client_order_id(exec: &KPaperExecution) -> Option<String> {
    match &exec.client_order_id {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .find_map(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn execution_is_reject(exec: &KPaperExecution) -> bool {
    exec.reject_reason
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

/// Whether this execution may complete `pending_place`.
///
/// Fills never complete a place. An ACK without a client id, or with a
/// different client id, must not bind to the in-flight request.
pub fn execution_may_complete_pending_place(
    exec: &KPaperExecution,
    pending: &OrderRequest,
) -> bool {
    if !execution_is_reject(exec) && exec.quantity > Decimal::ZERO {
        return false;
    }
    let Some(exec_cid) = execution_client_order_id(exec) else {
        return false;
    };
    pending
        .exchange_params
        .get("orderLinkId")
        .is_some_and(|cid| cid.as_str() == exec_cid)
}

/// Apply an amend ACK onto the cached working order. Paper's AmendAck is
/// order-id only; price/qty come from the in-flight request.
pub fn apply_amend_ack_to_order(existing: &Order, new_request: &OrderRequest) -> Order {
    Order {
        price: new_request.price,
        amount: new_request.amount,
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
        client_order_id: execution_client_order_id(exec).map(SmolStr::new),
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
        assert!(order.client_order_id.is_none());
    }

    #[test]
    fn execution_ack_to_order_uses_exec_client_id_not_request() {
        use trading_core::types::exchange::OrderRequest;
        let mut exec = KPaperExecution {
            order_id: "ord-1".to_string(),
            trade_id: None,
            instrument_id: "BTC-USDT".to_string(),
            side: "Buy".to_string(),
            price: Decimal::new(50000, 0),
            quantity: Decimal::ZERO,
            fee: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            reject_reason: None,
            client_order_id: Some(serde_json::Value::String("cid-ack".to_string())),
        };
        let mut req = OrderRequest {
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: trading_core::types::common::Side::Buy,
            order_type: trading_core::types::common::OrderType::Limit,
            price: Decimal::new(50000, 0),
            amount: Decimal::new(2, 3),
            timestamp: None,
            exchange_params: std::collections::HashMap::new(),
        };
        req.exchange_params
            .insert(SmolStr::new("orderLinkId"), SmolStr::new("cid-pending"));
        let order = execution_ack_to_order(&exec, &req, InstrumentType::Spot).unwrap();
        assert_eq!(order.client_order_id.as_deref(), Some("cid-ack"));
        exec.client_order_id = None;
        let uncorrelated = execution_ack_to_order(&exec, &req, InstrumentType::Spot).unwrap();
        assert!(uncorrelated.client_order_id.is_none());
    }

    #[test]
    fn apply_amend_ack_updates_price_and_qty() {
        let existing = Order {
            order_id: OrderId::new("ord-a".into()),
            client_order_id: Some(SmolStr::new("cid-working")),
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(1010, 0),
            amount: Decimal::new(1, 2),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };
        let req = OrderRequest {
            instrument: existing.instrument.clone(),
            side: existing.side.clone(),
            order_type: trading_core::types::common::OrderType::Limit,
            price: Decimal::new(1020, 0),
            amount: Decimal::new(1, 2),
            timestamp: None,
            exchange_params: std::collections::HashMap::new(),
        };
        let amended = apply_amend_ack_to_order(&existing, &req);
        assert_eq!(amended.price, Decimal::new(1020, 0));
        assert_eq!(amended.amount, Decimal::new(1, 2));
        assert_eq!(amended.client_order_id.as_deref(), Some("cid-working"));
        assert_eq!(amended.order_id.as_str(), "ord-a");
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
    fn duplicate_trade_id_does_not_consume_remainder() {
        let existing = Order {
            order_id: OrderId::new("ord-dup".into()),
            client_order_id: Some(SmolStr::new("cid-dup")),
            instrument: Instrument::new(InstrumentType::Futures, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(49000, 0),
            amount: Decimal::new(2, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };
        let mut applied = HashSet::new();
        assert!(fill_trade_id_is_new(&mut applied, Some("paper-exec-1")));
        let partial = merge_fill_into_working_order(&existing, Decimal::new(1, 3));
        assert!(!fill_trade_id_is_new(&mut applied, Some("paper-exec-1")));
        assert_eq!(partial.status, OrderStatus::PartiallyFilled);
        assert_eq!(partial.filled_amount, Decimal::new(1, 3));
        assert_eq!(
            (partial.amount - partial.filled_amount),
            Decimal::new(1, 3),
            "replaying the same trade id must leave the working remainder"
        );
        assert!(fill_trade_id_is_new(&mut applied, None));
    }

    #[test]
    fn cancel_all_is_unverified_when_rest_fails() {
        assert!(cancel_all_after_rest(false, None).is_err());
        assert!(cancel_all_after_rest(false, Some("local cancel failed".into())).is_err());
        assert!(cancel_all_after_rest(true, None).is_ok());
        assert!(cancel_all_after_rest(true, Some("unknown order".into())).is_err());
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

    fn pending_place(client_id: &str) -> OrderRequest {
        let mut req = OrderRequest {
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            order_type: trading_core::types::common::OrderType::Limit,
            price: Decimal::new(1000, 0),
            amount: Decimal::new(1, 2),
            timestamp: None,
            exchange_params: std::collections::HashMap::new(),
        };
        req.exchange_params
            .insert(SmolStr::new("orderLinkId"), SmolStr::new(client_id));
        req
    }

    fn exec(qty: i64, scale: u32, reject: Option<&str>, client: Option<&str>) -> KPaperExecution {
        KPaperExecution {
            order_id: "ord-other".to_string(),
            trade_id: Some("fill-1".to_string()),
            instrument_id: "BTC-USDT".to_string(),
            side: "Buy".to_string(),
            price: Decimal::new(1000, 0),
            quantity: Decimal::new(qty, scale),
            fee: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            reject_reason: reject.map(str::to_string),
            client_order_id: client.map(|s| serde_json::Value::String(s.to_string())),
        }
    }

    #[test]
    fn fill_never_completes_pending_place() {
        let pending = pending_place("cid-b");
        assert!(!execution_may_complete_pending_place(
            &exec(1, 2, None, None),
            &pending
        ));
        assert!(!execution_may_complete_pending_place(
            &exec(1, 2, None, Some("cid-a")),
            &pending
        ));
    }

    #[test]
    fn zero_qty_ack_without_client_id_does_not_complete_pending_place() {
        let pending = pending_place("cid-b");
        assert!(!execution_may_complete_pending_place(
            &exec(0, 0, None, None),
            &pending
        ));
    }

    #[test]
    fn ack_with_other_client_id_does_not_complete_pending_place() {
        let pending = pending_place("cid-b");
        assert!(!execution_may_complete_pending_place(
            &exec(0, 0, None, Some("cid-a")),
            &pending
        ));
        assert!(execution_may_complete_pending_place(
            &exec(0, 0, None, Some("cid-b")),
            &pending
        ));
    }
}
