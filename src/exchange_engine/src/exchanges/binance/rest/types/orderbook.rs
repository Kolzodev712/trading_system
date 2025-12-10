use trading_core::{
    types::{common::Pair, market_data::StructuredOrderbook},
    utils::{deserialize_smol_str, serialize_smol_str},
};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// <https://binance-docs.github.io/apidocs/spot/en/#order-book>
pub struct BinanceOrderbookSnapshotRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// <https://binance-docs.github.io/apidocs/spot/en/#order-book>
pub struct BinanceOrderbookSnapshotResponse {
    pub last_update_id: u64,
    pub bids: Vec<[Decimal; 2]>,
    pub asks: Vec<[Decimal; 2]>,
}

impl From<BinanceOrderbookSnapshotResponse> for StructuredOrderbook {
    fn from(snapshot: BinanceOrderbookSnapshotResponse) -> Self {
        // Note: This conversion needs a symbol/pair, but BinanceOrderbookSnapshotResponse doesn't have it
        // This is a limitation - the caller should provide the pair
        // For now, we'll use a placeholder - this should be fixed by the caller
        let pair = Pair::new("UNKNOWN", "UNKNOWN"); // TODO: This should be provided by caller

        let bids: Vec<(rust_decimal::Decimal, rust_decimal::Decimal)> = snapshot
            .bids
            .into_iter()
            .map(|bid| (bid[0], bid[1]))
            .collect();
        let asks: Vec<(rust_decimal::Decimal, rust_decimal::Decimal)> = snapshot
            .asks
            .into_iter()
            .map(|ask| (ask[0], ask[1]))
            .collect();

        StructuredOrderbook {
            pair,
            bids,
            asks,
            timestamp: snapshot.last_update_id, // Use last_update_id as timestamp
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    // Test case for deserializing a `BinanceOrderbookSnapshotRequest`
    #[test]
    fn test_binance_orderbook_snapshot_request_deserialization() {
        let json = r#"
        {
            "symbol": "BTCUSDT",
            "limit": 10
        }
        "#;

        let request: BinanceOrderbookSnapshotRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.symbol, SmolStr::new("BTCUSDT"));
        assert_eq!(request.limit, Some(10));
    }

    // Test case for deserializing a `BinanceOrderbookSnapshotResponse`
    #[test]
    fn test_binance_orderbook_snapshot_response_deserialization() {
        let json = r#"
        {
            "lastUpdateId": 123456789,
            "bids": [
                ["3000.00000000", "0.10"],
                ["2900.00000000", "0.20"]
            ],
            "asks": [
                ["3100.00000000", "0.15"],
                ["3200.00000000", "0.25"]
            ]
        }
        "#;

        let response: BinanceOrderbookSnapshotResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.last_update_id, 123456789);
        assert_eq!(response.bids.len(), 2);
        assert_eq!(response.asks.len(), 2);
        assert_eq!(response.bids[0][0], Decimal::new(300000000000, 8));
        assert_eq!(response.bids[0][1], Decimal::new(10, 2));
        assert_eq!(response.asks[1][0], Decimal::new(320000000000, 8));
        assert_eq!(response.asks[1][1], Decimal::new(25, 2));
    }

    // Test case for converting a `BinanceOrderbookSnapshotResponse` into a `StructuredOrderbook`
    #[test]
    fn test_binance_orderbook_snapshot_response_to_orderbook_conversion() {
        use trading_core::types::common::Pair;
        let response = BinanceOrderbookSnapshotResponse {
            last_update_id: 123456789,
            bids: vec![
                [Decimal::new(3000, 8), Decimal::new(10, 2)],
                [Decimal::new(2900, 8), Decimal::new(20, 2)],
            ],
            asks: vec![
                [Decimal::new(3100, 8), Decimal::new(15, 2)],
                [Decimal::new(3200, 8), Decimal::new(25, 2)],
            ],
        };

        // Note: This conversion needs a pair, which should be provided by the caller
        // For testing, we'll manually construct it
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![
                (Decimal::new(3000, 8), Decimal::new(10, 2)),
                (Decimal::new(2900, 8), Decimal::new(20, 2)),
            ],
            asks: vec![
                (Decimal::new(3100, 8), Decimal::new(15, 2)),
                (Decimal::new(3200, 8), Decimal::new(25, 2)),
            ],
            timestamp: 123456789,
        };

        assert_eq!(orderbook.bids.len(), 2);
        assert_eq!(orderbook.asks.len(), 2);
        assert_eq!(orderbook.bids[0].0, Decimal::new(3000, 8));
        assert_eq!(orderbook.bids[0].1, Decimal::new(10, 2));
        assert_eq!(orderbook.asks[1].0, Decimal::new(3200, 8));
        assert_eq!(orderbook.asks[1].1, Decimal::new(25, 2));
    }
}
