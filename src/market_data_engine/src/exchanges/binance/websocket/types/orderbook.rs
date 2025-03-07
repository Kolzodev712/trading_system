use core_logic::{
    data::public_data::Orderbook,
    utils::{deserialize_smol_str, serialize_smol_str},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::primitive::str;

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
    pub price: Decimal,
    pub quantity: Decimal,
}

impl From<BinanceOrderbook> for Orderbook {
    fn from(orderbook: BinanceOrderbook) -> Self {
        let bids = orderbook
            .bids
            .into_iter()
            .map(|level| level.price)
            .collect();
        let asks = orderbook
            .asks
            .into_iter()
            .map(|level| level.price)
            .collect();
        Orderbook { bids, asks }
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
