//! Offline integration tests for DeepBook indexer JSON fixtures.

use crate::helpers::dex_engine::load_fixture;
use dex_engine::chains::sui::indexer::IndexerOpenOrder;
use dex_engine::{DeepBookPoolMeta, IndexerOrderbook, OrderbookSnapshot};
use market_data_engine::exchanges::deepbook::map::orderbook_to_structured;
use rust_decimal::Decimal;
use std::collections::HashSet;
use std::str::FromStr;

#[test]
fn parses_get_pools_fixture_and_filters_by_name() {
    let pools: Vec<DeepBookPoolMeta> =
        serde_json::from_str(&load_fixture("get_pools.json")).expect("parse pools");
    assert_eq!(pools.len(), 1);
    assert_eq!(pools[0].pool_name.as_str(), "SUI_DBUSDC");

    let wanted: HashSet<&str> = ["SUI_DBUSDC"].into_iter().collect();
    let filtered: Vec<_> = pools
        .into_iter()
        .filter(|p| wanted.contains(p.pool_name.as_str()))
        .collect();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].tick_size, 10_000);
}

#[test]
fn parses_orderbook_fixture_and_maps_to_structured_md() {
    let raw: IndexerOrderbook =
        serde_json::from_str(&load_fixture("orderbook_sui_dbusdc.json")).expect("parse orderbook");
    let snapshot = OrderbookSnapshot {
        timestamp_ms: raw.timestamp.parse().unwrap_or(0),
        bids: raw
            .bids
            .iter()
            .map(|[p, q]| Ok((Decimal::from_str(p).unwrap(), Decimal::from_str(q).unwrap())))
            .collect::<Result<Vec<_>, ()>>()
            .unwrap(),
        asks: raw
            .asks
            .iter()
            .map(|[p, q]| Ok((Decimal::from_str(p).unwrap(), Decimal::from_str(q).unwrap())))
            .collect::<Result<Vec<_>, ()>>()
            .unwrap(),
    };
    let structured = orderbook_to_structured("SUI_DBUSDC", &snapshot);
    assert_eq!(structured.pair.base.as_str(), "SUI");
    assert_eq!(structured.bids.len(), 2);
    assert_eq!(structured.asks[0].0, Decimal::from_str("3.7150").unwrap());
}

#[test]
fn parses_open_orders_fixture_to_dex_orders() {
    let orders: Vec<IndexerOpenOrder> =
        serde_json::from_str(&load_fixture("open_orders.json")).expect("parse orders");
    let dex = orders[0].to_dex_open_order("SUI_DBUSDC").unwrap();
    assert_eq!(dex.order_id.as_str(), "987654321");
    assert_eq!(dex.side.as_str(), "buy");
}
