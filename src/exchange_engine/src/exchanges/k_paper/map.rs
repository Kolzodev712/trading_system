//! Map k-paper wire types to trading_core types.

use rust_decimal::Decimal;
use smol_str::SmolStr;
use trading_core::types::common::{
    Instrument, InstrumentType, OrderId, OrderStatus, Pair, Side, Symbol,
};
use trading_core::types::exchange::{Balance, Order, OrderRequest, Position, Trade};
use trading_core::types::market_data::{StructuredOrderbook, StructuredPublicTrade};

use super::messages::{
    DecimalOrStr, KPaperBalance, KPaperExecution, KPaperOrderbook, KPaperPosition,
    KPaperPublicTrade,
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

/// Unix seconds to ISO 8601 (for optional client use).
#[allow(dead_code)]
pub fn timestamp_u64_to_iso(secs: u64) -> String {
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

fn decimal_or_str(d: &DecimalOrStr) -> Decimal {
    d.to_decimal().unwrap_or(Decimal::ZERO)
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
    let bids: Vec<(Decimal, Decimal)> = ob
        .bids
        .iter()
        .map(|l| (decimal_or_str(&l.price), decimal_or_str(&l.quantity)))
        .collect();
    let asks: Vec<(Decimal, Decimal)> = ob
        .asks
        .iter()
        .map(|l| (decimal_or_str(&l.price), decimal_or_str(&l.quantity)))
        .collect();
    let timestamp = timestamp_iso_to_u64_secs(ob.timestamp.as_str());
    Some(StructuredOrderbook {
        pair,
        bids,
        asks,
        timestamp,
    })
}

/// Convert k-paper public trade to StructuredPublicTrade.
pub fn public_trade_to_structured(t: &KPaperPublicTrade) -> Option<StructuredPublicTrade> {
    let pair = parse_instrument_id(t.instrument_id.as_str())?;
    Some(StructuredPublicTrade {
        pair,
        price: decimal_or_str(&t.price),
        quantity: decimal_or_str(&t.quantity),
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
    let price = decimal_or_str(&exec.price);
    let timestamp_secs = timestamp_iso_to_u64_secs(exec.timestamp.as_str());
    let timestamp_ms = timestamp_secs.saturating_mul(1000);
    Some(Order {
        order_id: OrderId::new(exec.order_id.clone().into()),
        client_order_id: None,
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

/// Convert k-paper execution to Order (for ExchangeUpdate::Order).
/// Filled or rejected; we use Filled when reject_reason is null.
pub fn execution_to_order(
    exec: &KPaperExecution,
    instrument_type: InstrumentType,
) -> Option<Order> {
    let pair = parse_instrument_id(exec.instrument_id.as_str())?;
    let instrument = Instrument::new(instrument_type, pair);
    let price = decimal_or_str(&exec.price);
    let quantity = decimal_or_str(&exec.quantity);
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

/// Convert k-paper execution to Trade (for ExchangeUpdate::Trade).
pub fn execution_to_trade(exec: &KPaperExecution) -> Option<Trade> {
    let pair = parse_instrument_id(exec.instrument_id.as_str())?;
    let trade_id = exec.trade_id.as_deref().unwrap_or("").to_string().into();
    let timestamp_secs = timestamp_iso_to_u64_secs(exec.timestamp.as_str());
    let timestamp_ms = timestamp_secs.saturating_mul(1000);
    Some(Trade {
        trade_id,
        order_id: OrderId::new(exec.order_id.clone().into()),
        pair,
        price: decimal_or_str(&exec.price),
        quantity: decimal_or_str(&exec.quantity),
        side: side_from_str(exec.side.as_str()),
        timestamp: timestamp_ms,
        fee: exec
            .fee
            .as_ref()
            .map(decimal_or_str)
            .unwrap_or(Decimal::ZERO),
        fee_currency: Symbol::from("USDT"),
    })
}

/// Convert k-paper balance to Balance.
pub fn balance_to_core(b: &KPaperBalance) -> Balance {
    Balance {
        symbol: Symbol::from(b.asset.as_str()),
        free: decimal_or_str(&b.available),
        used: decimal_or_str(&b.locked),
        total: decimal_or_str(&b.total),
    }
}

/// Convert k-paper position to Position (spot-style: quantity and entry_price).
pub fn position_to_core(pos: &KPaperPosition, instrument_type: InstrumentType) -> Option<Position> {
    let pair = parse_instrument_id(pos.instrument_id.as_str())?;
    let instrument = Instrument::new(instrument_type, pair);
    let quantity = decimal_or_str(&pos.quantity);
    let entry_price = decimal_or_str(&pos.entry_price);
    let unrealized_pnl = pos
        .unrealized_pnl
        .as_ref()
        .map(decimal_or_str)
        .unwrap_or(Decimal::ZERO);
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
