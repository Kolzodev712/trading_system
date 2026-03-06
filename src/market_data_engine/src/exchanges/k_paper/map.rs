//! Map k-paper wire types to trading_core market data types.

use rust_decimal::Decimal;
use trading_core::types::market_data::{StructuredOrderbook, StructuredPublicTrade};

use super::messages::{KPaperOrderbook, KPaperPublicTrade};
use trading_core::types::common::Pair;

pub fn parse_instrument_id(s: &str) -> Option<Pair> {
    let parts: Vec<&str> = s.splitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
    }
    Some(Pair::new(parts[0], parts[1]))
}

fn timestamp_iso_to_u64_secs(s: &str) -> u64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp() as u64)
        .unwrap_or(0)
}

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
    })
}

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
