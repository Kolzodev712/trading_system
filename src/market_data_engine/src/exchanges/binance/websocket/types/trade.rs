use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::primitive::str;
use trading_core::{
    types::{common::Pair, market_data::StructuredPublicTrade},
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceTrade {
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
    #[serde(rename = "t")]
    pub trade_id: u64,
    #[serde(rename = "p")]
    pub price: Decimal,
    #[serde(rename = "q")]
    pub quantity: Decimal,
    #[serde(rename = "T")]
    pub trade_time: u64,
    #[serde(rename = "m")]
    pub is_market_maker: bool,
}

impl From<BinanceTrade> for StructuredPublicTrade {
    fn from(trade: BinanceTrade) -> Self {
        // Convert symbol to Pair - simple parsing for common Binance symbols
        let symbol_str = trade.symbol.as_str();
        let pair = if symbol_str.ends_with("USDT") {
            let base = &symbol_str[..symbol_str.len() - 4];
            Pair::new(base, "USDT")
        } else if symbol_str.ends_with("BTC") {
            let base = &symbol_str[..symbol_str.len() - 3];
            Pair::new(base, "BTC")
        } else if symbol_str.ends_with("ETH") {
            let base = &symbol_str[..symbol_str.len() - 3];
            Pair::new(base, "ETH")
        } else if symbol_str.ends_with("BNB") {
            let base = &symbol_str[..symbol_str.len() - 3];
            Pair::new(base, "BNB")
        } else {
            // Fallback: assume last 4 chars are quote, rest is base
            if symbol_str.len() >= 4 {
                let base = &symbol_str[..symbol_str.len() - 4];
                let quote = &symbol_str[symbol_str.len() - 4..];
                Pair::new(base, quote)
            } else {
                // Last resort: use UNKNOWN
                Pair::new("UNKNOWN", "UNKNOWN")
            }
        };

        StructuredPublicTrade {
            pair,
            price: trade.price,
            quantity: trade.quantity,
            timestamp: trade.trade_time,
            is_market_maker: trade.is_market_maker,
            trade_id: Some(smol_str::SmolStr::new(&trade.trade_id.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json;

    // Test for deserialization of a BinanceTrade struct
    #[test]
    fn test_binance_trade_deserialization() {
        let json = r#"
        {
            "e": "trade",
            "E": 1625158399000,
            "s": "BTCUSDT",
            "t": 123456789,
            "p": "3000.00000000",
            "q": "10.00000000",
            "T": 1625158399000,
            "m": true
        }
        "#;

        let trade: BinanceTrade = serde_json::from_str(json).unwrap();

        assert_eq!(trade.event_type, SmolStr::new("trade"));
        assert_eq!(trade.event_time, 1625158399000);
        assert_eq!(trade.symbol, SmolStr::new("BTCUSDT"));
        assert_eq!(trade.trade_id, 123456789);
        assert_eq!(trade.price, Decimal::new(30000000000, 7));
        assert_eq!(trade.quantity, Decimal::new(1000000000, 8));
        assert_eq!(trade.trade_time, 1625158399000);
        assert_eq!(trade.is_market_maker, true);
    }
}
