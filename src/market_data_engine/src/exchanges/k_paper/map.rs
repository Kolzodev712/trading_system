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

/// ISO 8601 timestamp to Unix **milliseconds**. Readiness and stale-MD checks
/// treat `StructuredOrderbook.timestamp` as ms (`ts / 1000` vs wall-clock seconds).
pub fn timestamp_iso_to_u64_millis(s: &str) -> u64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp_millis().max(0) as u64)
        .unwrap_or(0)
}

pub fn orderbook_to_structured(ob: &KPaperOrderbook) -> Option<StructuredOrderbook> {
    let pair = parse_instrument_id(ob.instrument_id.as_str())?;
    let bids: Vec<(Decimal, Decimal)> = ob.bids.iter().map(|l| (l.price, l.quantity)).collect();
    let asks: Vec<(Decimal, Decimal)> = ob.asks.iter().map(|l| (l.price, l.quantity)).collect();
    let timestamp = timestamp_iso_to_u64_millis(ob.timestamp.as_str());
    Some(StructuredOrderbook {
        pair,
        bids,
        asks,
        timestamp,
        update_kind: trading_core::types::market_data::OrderbookUpdateKind::Snapshot,
        sequence: None,
    })
}

pub fn public_trade_to_structured(t: &KPaperPublicTrade) -> Option<StructuredPublicTrade> {
    let pair = parse_instrument_id(t.instrument_id.as_str())?;
    Some(StructuredPublicTrade {
        pair,
        price: t.price,
        quantity: t.quantity,
        timestamp: timestamp_iso_to_u64_millis(t.timestamp.as_str()),
        is_market_maker: false,
        trade_id: Some(t.trade_id.clone().into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchanges::k_paper::messages::{KPaperPriceLevel, KPaperPublicTrade};

    fn dec(s: &str) -> Decimal {
        s.parse().expect("decimal")
    }

    fn sample_book(ts: &str) -> KPaperOrderbook {
        KPaperOrderbook {
            _type: "orderbook".into(),
            instrument_id: "BTC-USDT".into(),
            bids: vec![KPaperPriceLevel {
                price: dec("50000"),
                quantity: dec("1"),
            }],
            asks: vec![KPaperPriceLevel {
                price: dec("50010"),
                quantity: dec("1"),
            }],
            sequence: 1,
            timestamp: ts.into(),
        }
    }

    #[test]
    fn paper_book_timestamp_is_unix_milliseconds() {
        let book = orderbook_to_structured(&sample_book("2026-09-30T10:38:07.443Z")).unwrap();
        // Seconds would be ~1.75e9; milliseconds are ~1.75e12.
        assert!(
            book.timestamp > 10_000_000_000,
            "adapter must emit ms, got {}",
            book.timestamp
        );
        assert_eq!(book.timestamp, 1_790_764_687_443);
    }

    #[test]
    fn paper_trade_timestamp_is_unix_milliseconds() {
        let trade = public_trade_to_structured(&KPaperPublicTrade {
            _type: "public_trade".into(),
            instrument_id: "BTC-USDT".into(),
            price: dec("50000"),
            quantity: dec("0.01"),
            side: "buy".into(),
            timestamp: "2026-09-30T10:38:07.443Z".into(),
            trade_id: "t1".into(),
        })
        .unwrap();
        assert_eq!(trade.timestamp, 1_790_764_687_443);
    }
}
