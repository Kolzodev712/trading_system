use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::sync::Arc;
use trading_core::{
    registry::symbol::SymbolRegistry,
    types::market_data::StructuredPublicTrade,
    utils::{
        deserialize_option_smol_str, deserialize_smol_str, serialize_option_smol_str,
        serialize_smol_str,
    },
};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BybitTradeMessage {
    #[serde(
        rename = "topic",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub topic: SmolStr,
    #[serde(rename = "ts")]
    pub timestamp: u64,
    #[serde(rename = "data")]
    pub data: Vec<BybitTradeData>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BybitTradeData {
    #[serde(rename = "T")]
    pub trade_time: u64,
    #[serde(
        rename = "s",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(rename = "S")]
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub side: SmolStr, // "Buy" or "Sell"
    #[serde(rename = "v")]
    #[serde(with = "rust_decimal::serde::str")]
    pub volume: Decimal,
    #[serde(rename = "p")]
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(rename = "L", default)]
    #[serde(
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    pub trade_id: Option<SmolStr>,
    #[serde(rename = "BT")]
    pub block_trade: bool,
}

impl BybitTradeMessage {
    /// Convert to Vec<StructuredPublicTrade> with a provided SymbolRegistry
    pub fn into_with_symbol_registry(
        self,
        symbol_registry: Arc<SymbolRegistry>,
    ) -> Vec<StructuredPublicTrade> {
        self.data
            .into_iter()
            .map(|trade_data| {
                // Parse symbol to Pair using symbol registry
                let symbol_str = trade_data.symbol.as_str();
                let pair = symbol_registry.get_or_parse_pair(symbol_str);

                // Bybit: "Buy" = buyer-initiated trade, "Sell" = seller-initiated trade
                // For public trades, we don't have reliable market maker information
                // Store side in trade_id for display purposes (format: "SIDE:ID" or just "SIDE" if no ID)
                let side = trade_data.side.as_str();
                let trade_id_with_side = match &trade_data.trade_id {
                    Some(id) => Some(format!("{}:{}", side, id).into()),
                    None => Some(side.into()),
                };

                StructuredPublicTrade {
                    pair,
                    price: trade_data.price,
                    quantity: trade_data.volume,
                    timestamp: trade_data.trade_time,
                    is_market_maker: false, // Not reliably available for public trades
                    trade_id: trade_id_with_side,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_bybit_trade() {
        let json = json!({
            "topic": "publicTrade.BTCUSDT",
            "ts": 1625158399000u64,
            "data": [
                {
                    "T": 1625158399000u64,
                    "s": "BTCUSDT",
                    "S": "Buy",
                    "v": "10.0",
                    "p": "3000.00",
                    "L": "123456789",
                    "BT": false
                }
            ]
        });

        let trade: BybitTradeMessage = serde_json::from_value(json).unwrap();

        assert_eq!(trade.topic, "publicTrade.BTCUSDT");
        assert_eq!(trade.timestamp, 1625158399000u64);
        assert_eq!(trade.data.len(), 1);
        assert_eq!(trade.data[0].symbol, "BTCUSDT");
        assert_eq!(trade.data[0].side, "Buy");
    }

    #[test]
    fn test_bybit_trade_to_structured() {
        let json = json!({
            "topic": "publicTrade.BTCUSDT",
            "ts": 1625158399000u64,
            "data": [
                {
                    "T": 1625158399000u64,
                    "s": "BTCUSDT",
                    "S": "Sell",
                    "v": "10.0",
                    "p": "3000.00",
                    "L": "123456789",
                    "BT": false
                }
            ]
        });

        let bybit_trade: BybitTradeMessage = serde_json::from_value(json).unwrap();
        let symbol_registry = Arc::new(SymbolRegistry::new());
        let structured_trades = bybit_trade.into_with_symbol_registry(symbol_registry);

        assert_eq!(structured_trades.len(), 1);
        let trade = &structured_trades[0];
        assert_eq!(trade.pair.base, "BTC");
        assert_eq!(trade.pair.quote, "USDT");
        assert_eq!(trade.price, Decimal::new(300000, 2));
        assert_eq!(trade.quantity, Decimal::new(100, 1));
        assert!(!trade.is_market_maker); // Not reliably available for public trades
        assert_eq!(trade.trade_id, Some(SmolStr::new("Sell:123456789")));
    }

    #[test]
    fn test_deserialize_bybit_trade_without_l() {
        // Test that L field is optional
        let json = json!({
            "topic": "publicTrade.BTCUSDT",
            "ts": 1625158399000u64,
            "data": [
                {
                    "T": 1625158399000u64,
                    "s": "BTCUSDT",
                    "S": "Buy",
                    "v": "10.0",
                    "p": "3000.00",
                    "BT": false
                }
            ]
        });

        let trade: BybitTradeMessage = serde_json::from_value(json).unwrap();

        assert_eq!(trade.topic, "publicTrade.BTCUSDT");
        assert_eq!(trade.data.len(), 1);
        assert_eq!(trade.data[0].symbol, "BTCUSDT");
        assert_eq!(trade.data[0].trade_id, None); // L field is optional
    }
}
