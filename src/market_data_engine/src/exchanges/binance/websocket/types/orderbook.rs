use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::primitive::str;
use trading_core::{
    types::{common::Pair, market_data::StructuredOrderbook},
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceOrderbook {
    #[serde(
        rename = "e",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub event_type: SmolStr,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(
        rename = "s",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(rename = "b")]
    pub bids: Vec<PriceLevel>,
    #[serde(rename = "a")]
    pub asks: Vec<PriceLevel>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PriceLevel {
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
}

impl From<BinanceOrderbook> for StructuredOrderbook {
    fn from(orderbook: BinanceOrderbook) -> Self {
        // Convert symbol to Pair - simple parsing for common Binance symbols
        // For "BTCUSDT", we split at common quote currencies
        let symbol_str = orderbook.symbol.as_str();
        let pair = if let Some(base) = symbol_str.strip_suffix("USDT") {
            Pair::new(base, "USDT")
        } else if let Some(base) = symbol_str.strip_suffix("BTC") {
            Pair::new(base, "BTC")
        } else if let Some(base) = symbol_str.strip_suffix("ETH") {
            Pair::new(base, "ETH")
        } else if let Some(base) = symbol_str.strip_suffix("BNB") {
            Pair::new(base, "BNB")
        } else {
            // Fallback: assume last 4 chars are quote, rest is base
            if symbol_str.len() >= 4 {
                let (base, quote) = symbol_str.split_at(symbol_str.len() - 4);
                Pair::new(base, quote)
            } else {
                // Last resort: use UNKNOWN
                Pair::new("UNKNOWN", "UNKNOWN")
            }
        };

        // Convert PriceLevel to (price, quantity) tuples
        let bids: Vec<(rust_decimal::Decimal, rust_decimal::Decimal)> = orderbook
            .bids
            .into_iter()
            .map(|level| (level.price, level.quantity))
            .collect();
        let asks: Vec<(rust_decimal::Decimal, rust_decimal::Decimal)> = orderbook
            .asks
            .into_iter()
            .map(|level| (level.price, level.quantity))
            .collect();

        StructuredOrderbook {
            pair,
            bids,
            asks,
            timestamp: orderbook.event_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json;

    // Test for deserialization of BinanceOrderbook struct
    #[test]
    fn test_binance_orderbook_deserialization() {
        let json = r#"
        {
            "e": "depthUpdate",
            "E": 1625158399000,
            "s": "BTCUSDT",
            "U": 12345,
            "u": 12346,
            "b": [
                ["3000.00000000", "1.00000000"],
                ["2995.00000000", "0.50000000"]
            ],
            "a": [
                ["3050.00000000", "0.20000000"],
                ["3060.00000000", "0.10000000"]
            ]
        }
        "#;

        let orderbook: BinanceOrderbook = serde_json::from_str(json).unwrap();

        assert_eq!(orderbook.event_type, SmolStr::new("depthUpdate"));
        assert_eq!(orderbook.event_time, 1625158399000);
        assert_eq!(orderbook.symbol, SmolStr::new("BTCUSDT"));
        assert_eq!(orderbook.first_update_id, 12345);
        assert_eq!(orderbook.final_update_id, 12346);

        // Assert bids
        assert_eq!(orderbook.bids.len(), 2);
        assert_eq!(orderbook.bids[0].price, Decimal::new(300000000000, 8));
        assert_eq!(orderbook.bids[0].quantity, Decimal::new(100000000, 8));

        // Assert asks
        assert_eq!(orderbook.asks.len(), 2);
        assert_eq!(orderbook.asks[0].price, Decimal::new(305000000000, 8));
        assert_eq!(orderbook.asks[0].quantity, Decimal::new(20000000, 8));
    }
}
