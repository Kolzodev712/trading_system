use trading_core::{
    types::market_data::LegacyOrderbook as Orderbook,
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

impl From<BinanceOrderbookSnapshotResponse> for Orderbook {
    fn from(snapshot: BinanceOrderbookSnapshotResponse) -> Self {
        Orderbook {
            bids: snapshot
                .bids
                .into_iter()
                .flat_map(|bid| vec![bid[0], bid[1]])
                .collect(),
            asks: snapshot
                .asks
                .into_iter()
                .flat_map(|ask| vec![ask[0], ask[1]])
                .collect(),
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

    // Test case for converting a `BinanceOrderbookSnapshotResponse` into an `Orderbook`
    #[test]
    fn test_binance_orderbook_snapshot_response_to_orderbook_conversion() {
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

        let orderbook: Orderbook = response.into();

        assert_eq!(orderbook.bids.len(), 4);
        assert_eq!(orderbook.asks.len(), 4);
        assert_eq!(orderbook.bids[0], Decimal::new(3000, 8));
        assert_eq!(orderbook.bids[1], Decimal::new(10, 2));
        assert_eq!(orderbook.asks[2], Decimal::new(3200, 8));
        assert_eq!(orderbook.asks[3], Decimal::new(25, 2));
    }
}
