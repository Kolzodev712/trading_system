//! k-paper REST API client (reconciliation: balances, open_orders, trades).
//!
//! Base URL is derived from the WebSocket URL: ws://host:port/path -> http://host:port.

use rust_decimal::Decimal;
use serde::Deserialize;
use trading_core::types::common::{Instrument, InstrumentType, OrderId, OrderStatus, Side};
use trading_core::types::exchange::{Balance, Order, Position, Trade};

use super::map::{parse_instrument_id, timestamp_iso_to_u64_secs};

/// Derive HTTP base URL from WebSocket URL (e.g. ws://127.0.0.1:19999/ws/trade -> http://127.0.0.1:19999).
pub fn ws_url_to_http_base(ws_url: &str) -> String {
    if let Ok(u) = url::Url::parse(ws_url) {
        let mut base = u;
        base.set_scheme(if base.scheme() == "wss" {
            "https"
        } else {
            "http"
        })
        .ok();
        base.set_path("");
        base.set_query(None);
        base.set_fragment(None);
        return base.to_string().trim_end_matches('/').to_string();
    }
    let s = ws_url.trim_end_matches('/');
    let without_path = s
        .strip_prefix("wss://")
        .or_else(|| s.strip_prefix("ws://"))
        .unwrap_or(s);
    let (scheme, rest) = if s.starts_with("wss://") {
        ("https://", without_path)
    } else {
        ("http://", without_path)
    };
    let host_port = rest.split('/').next().unwrap_or(rest);
    format!("{scheme}{host_port}")
}

/// REST balance item (same shape as WS balance).
#[derive(Debug, Clone, Deserialize)]
pub struct RestBalance {
    pub asset: String,
    #[serde(deserialize_with = "super::messages::decimal_flexible::deserialize")]
    pub available: Decimal,
    #[serde(deserialize_with = "super::messages::decimal_flexible::deserialize")]
    pub locked: Decimal,
    #[serde(deserialize_with = "super::messages::decimal_flexible::deserialize")]
    pub total: Decimal,
}

/// REST open order item (GET /open_orders).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestOrder {
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub instrument_id: String,
    pub side: String,
    pub price: Decimal,
    pub amount: Decimal,
    pub filled_amount: Decimal,
    pub status: String,
    pub created_at: String, // ISO 8601 or we accept ms number via custom
}

/// REST trade item (GET /trades).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestTrade {
    pub trade_id: String,
    pub order_id: String,
    pub instrument_id: String,
    pub side: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub timestamp: String,
    #[serde(default)]
    pub fee: Option<Decimal>,
    #[serde(default)]
    pub fee_currency: Option<String>,
}

fn side_from_str(s: &str) -> Side {
    if s.eq_ignore_ascii_case("buy") {
        Side::Buy
    } else {
        Side::Sell
    }
}

fn status_from_str(s: &str) -> OrderStatus {
    match s {
        "New" => OrderStatus::New,
        "PartiallyFilled" => OrderStatus::PartiallyFilled,
        "Filled" => OrderStatus::Filled,
        "Canceled" | "Cancelled" => OrderStatus::Canceled,
        "PendingCancel" => OrderStatus::PendingCancel,
        "Rejected" => OrderStatus::Rejected,
        "Expired" => OrderStatus::Expired,
        "Amended" => OrderStatus::Amended,
        _ => OrderStatus::New,
    }
}

pub fn rest_balance_to_core(b: &RestBalance) -> Balance {
    Balance {
        symbol: trading_core::types::common::Symbol::from(b.asset.as_str()),
        free: b.available,
        used: b.locked,
        total: b.total,
    }
}

pub fn rest_order_to_core(o: &RestOrder, instrument_type: InstrumentType) -> Option<Order> {
    let pair = parse_instrument_id(&o.instrument_id)?;
    let instrument = Instrument::new(instrument_type, pair);
    let created_ms = timestamp_iso_to_u64_secs(&o.created_at).saturating_mul(1000);
    Some(Order {
        order_id: OrderId::new(o.order_id.clone().into()),
        client_order_id: o.client_order_id.as_ref().map(smol_str::SmolStr::new),
        instrument,
        side: side_from_str(&o.side),
        price: o.price,
        amount: o.amount,
        internal_created_at: created_ms,
        exchange_created_at: Some(created_ms),
        status: status_from_str(&o.status),
        filled_amount: o.filled_amount,
    })
}

pub fn rest_trade_to_core(t: &RestTrade) -> Option<Trade> {
    let pair = parse_instrument_id(&t.instrument_id)?;
    let ts_ms = timestamp_iso_to_u64_secs(&t.timestamp).saturating_mul(1000);
    Some(Trade {
        trade_id: t.trade_id.clone().into(),
        order_id: OrderId::new(t.order_id.clone().into()),
        pair,
        price: t.price,
        quantity: t.quantity,
        side: side_from_str(&t.side),
        timestamp: ts_ms,
        fee: t.fee.unwrap_or(Decimal::ZERO),
        fee_currency: t
            .fee_currency
            .as_deref()
            .map(trading_core::types::common::Symbol::from)
            .unwrap_or_else(|| trading_core::types::common::Symbol::from("USDT")),
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct RestPosition {
    pub instrument_id: String,
    pub side: String,
    #[serde(deserialize_with = "super::messages::decimal_flexible::deserialize")]
    pub quantity: Decimal,
    #[serde(deserialize_with = "super::messages::decimal_flexible::deserialize")]
    pub entry_price: Decimal,
    #[serde(default)]
    pub unrealized_pnl: Option<Decimal>,
}

pub fn rest_position_to_core(
    p: &RestPosition,
    instrument_type: InstrumentType,
) -> Option<Position> {
    let pair = parse_instrument_id(&p.instrument_id)?;
    Some(Position {
        instrument: Instrument::new(instrument_type, pair),
        quantity: p.quantity,
        avg_price: p.entry_price,
        current_price: p.entry_price,
        unrealized_pnl: p.unrealized_pnl.unwrap_or(Decimal::ZERO),
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

    #[test]
    fn ws_url_to_http_base_ws() {
        assert_eq!(
            ws_url_to_http_base("ws://127.0.0.1:19999/ws/trade"),
            "http://127.0.0.1:19999"
        );
    }

    #[test]
    fn ws_url_to_http_base_wss() {
        assert_eq!(
            ws_url_to_http_base("wss://example.com:443/ws/market"),
            "https://example.com"
        );
    }
}
