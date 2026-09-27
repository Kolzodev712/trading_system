//! Map DeepBook indexer responses to structured market data types.

use dex_engine::{pool_name_to_pair, IndexerTrade, OrderbookSnapshot};
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::str::FromStr;
use trading_core::types::market_data::{StructuredOrderbook, StructuredPublicTrade};

pub fn orderbook_to_structured(
    pool_name: &str,
    snapshot: &OrderbookSnapshot,
) -> StructuredOrderbook {
    StructuredOrderbook {
        pair: pool_name_to_pair(pool_name),
        bids: snapshot.bids.clone(),
        asks: snapshot.asks.clone(),
        timestamp: snapshot.timestamp_ms,
        update_kind: trading_core::types::market_data::OrderbookUpdateKind::Snapshot,
        sequence: None,
    }
}

pub fn trade_to_structured(pool_name: &str, trade: &IndexerTrade) -> Option<StructuredPublicTrade> {
    let price = Decimal::from_str(&trade.price.to_string()).ok()?;
    let quantity = Decimal::from_str(&trade.base_volume.to_string()).ok()?;
    Some(StructuredPublicTrade {
        pair: pool_name_to_pair(pool_name),
        price,
        quantity,
        timestamp: trade.timestamp,
        is_market_maker: false,
        trade_id: trade.trade_id.as_ref().map(|id| SmolStr::new(id.as_str())),
    })
}
