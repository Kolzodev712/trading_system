use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::{
    types::{common::Pair, market_data::StructuredOrderbook},
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BybitOrderbookMessage {
    #[serde(
        rename = "topic",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub topic: SmolStr,
    #[serde(rename = "type")]
    pub message_type: BybitMessageType,
    #[serde(rename = "ts")]
    pub timestamp: u64,
    #[serde(rename = "data")]
    pub data: BybitOrderbookData,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BybitMessageType {
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "delta")]
    Delta,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BybitOrderbookData {
    #[serde(rename = "s")]
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(rename = "b")]
    pub bids: Vec<[SmolStr; 2]>, // [price, size]
    #[serde(rename = "a")]
    pub asks: Vec<[SmolStr; 2]>, // [price, size]
    #[serde(rename = "u")]
    pub update_id: u64,
    #[serde(rename = "seq")]
    pub sequence: u64,
}

impl BybitOrderbookMessage {
    /// Convert to StructuredOrderbook with a provided Pair
    pub fn into_with_pair(self, pair: Pair) -> StructuredOrderbook {
        // Convert price/size strings to Decimal tuples
        let bids: Vec<(Decimal, Decimal)> = self
            .data
            .bids
            .into_iter()
            .filter_map(|level| {
                let price = level[0].parse::<Decimal>().ok()?;
                let size = level[1].parse::<Decimal>().ok()?;
                Some((price, size))
            })
            .collect();

        let asks: Vec<(Decimal, Decimal)> = self
            .data
            .asks
            .into_iter()
            .filter_map(|level| {
                let price = level[0].parse::<Decimal>().ok()?;
                let size = level[1].parse::<Decimal>().ok()?;
                Some((price, size))
            })
            .collect();

        StructuredOrderbook {
            pair,
            bids,
            asks,
            timestamp: self.timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_bybit_orderbook_snapshot() {
        let json = json!({
            "topic": "orderbook.1.BTCUSDT",
            "type": "snapshot",
            "ts": 1625158399000u64,
            "data": {
                "s": "BTCUSDT",
                "b": [
                    ["3000.00", "1.0"],
                    ["2995.00", "0.5"]
                ],
                "a": [
                    ["3050.00", "0.2"],
                    ["3060.00", "0.1"]
                ],
                "u": 12345,
                "seq": 12346
            }
        });

        let orderbook: BybitOrderbookMessage = serde_json::from_value(json).unwrap();

        assert_eq!(orderbook.topic, "orderbook.1.BTCUSDT");
        assert_eq!(orderbook.message_type, BybitMessageType::Snapshot);
        assert_eq!(orderbook.timestamp, 1625158399000u64);
        assert_eq!(orderbook.data.symbol, "BTCUSDT");
        assert_eq!(orderbook.data.bids.len(), 2);
        assert_eq!(orderbook.data.asks.len(), 2);
    }

    #[test]
    fn test_bybit_orderbook_to_structured() {
        let json = json!({
            "topic": "orderbook.1.BTCUSDT",
            "type": "snapshot",
            "ts": 1625158399000u64,
            "data": {
                "s": "BTCUSDT",
                "b": [
                    ["3000.00", "1.0"]
                ],
                "a": [
                    ["3050.00", "0.2"]
                ],
                "u": 12345,
                "seq": 12346
            }
        });

        let bybit_orderbook: BybitOrderbookMessage = serde_json::from_value(json).unwrap();
        let pair = Pair::new("BTC", "USDT");
        let structured: StructuredOrderbook = bybit_orderbook.into_with_pair(pair.clone());

        assert_eq!(structured.pair.base, "BTC");
        assert_eq!(structured.pair.quote, "USDT");
        assert_eq!(structured.bids.len(), 1);
        assert_eq!(structured.asks.len(), 1);
        assert_eq!(structured.timestamp, 1625158399000u64);
    }
}
