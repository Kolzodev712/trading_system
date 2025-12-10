use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};
use trading_core::{
    types::{common::Side, exchange::Trade},
    utils::{deserialize_smol_str, serialize_smol_str},
};

/// Request parameters for fetching trade history
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceTradesRequest {
    /// Trading pair symbol (required)
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    /// Start time (optional)
    pub start_time: Option<u64>,
    /// End time (optional)
    pub end_time: Option<u64>,
    /// From trade ID (optional)
    pub from_id: Option<u64>,
    /// Limit (optional, default 500, max 1000)
    pub limit: Option<u16>,
    /// Timestamp for request signing
    pub timestamp: u64,
}

/// Response for trade history (array of trades)
pub type BinanceTradesResponse = Vec<BinanceTradeResponse>;

/// Single trade response
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceTradeResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub id: u64,
    pub order_id: u64,
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub qty: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quote_qty: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub commission: Decimal,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub commission_asset: SmolStr,
    pub time: u64,
    pub is_buyer: bool,
    pub is_maker: bool,
    pub is_best_match: bool,
}

impl From<BinanceTradeResponse> for Trade {
    fn from(binance_trade: BinanceTradeResponse) -> Self {
        // Convert symbol to Pair (assuming format like "BTCUSDT")
        // This is a simplified approach - in production, use proper symbol mapping
        let symbol_str = binance_trade.symbol.as_str();
        let pair = if symbol_str.len() >= 6 {
            // Try to split - common pattern: base + "USDT" or "USDC"
            let base_len = symbol_str.len() - 4; // Assume 4-char quote like "USDT"
            let base = &symbol_str[..base_len];
            let quote = &symbol_str[base_len..];
            trading_core::types::common::Pair::new(base, quote)
        } else {
            // Fallback - use symbol as base, empty quote
            trading_core::types::common::Pair::new(symbol_str, "")
        };

        Trade {
            trade_id: binance_trade.id.to_smolstr(),
            order_id: binance_trade.order_id.into(),
            pair,
            price: binance_trade.price,
            quantity: binance_trade.qty,
            side: if binance_trade.is_buyer {
                Side::Buy
            } else {
                Side::Sell
            },
            timestamp: binance_trade.time,
            fee: binance_trade.commission,
            fee_currency: binance_trade.commission_asset.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use trading_core::types::common::OrderId;

    /// Test serialization of BinanceTradesRequest
    #[test]
    fn test_binance_trades_request_serialization() {
        let request = BinanceTradesRequest {
            symbol: "BTCUSDT".into(),
            start_time: Some(1734819600000),
            end_time: Some(1734819696263),
            from_id: Some(12345),
            limit: Some(500),
            timestamp: 1734819696263,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        println!("Serialized BinanceTradesRequest: {}", serialized);

        assert!(serialized.contains("BTCUSDT"));
        assert!(serialized.contains("1734819600000"));
        assert!(serialized.contains("1734819696263"));
        assert!(serialized.contains("12345"));
        assert!(serialized.contains("500"));
    }

    /// Test serialization of BinanceTradesRequest with minimal fields
    #[test]
    fn test_binance_trades_request_serialization_minimal() {
        let request = BinanceTradesRequest {
            symbol: "BTCUSDT".into(),
            start_time: None,
            end_time: None,
            from_id: None,
            limit: None,
            timestamp: 1734819696263,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        println!("Serialized BinanceTradesRequest (minimal): {}", serialized);

        assert!(serialized.contains("BTCUSDT"));
        assert!(serialized.contains("1734819696263"));
    }

    /// Test deserialization of BinanceTradesRequest
    #[test]
    fn test_binance_trades_request_deserialization() {
        let json_data = r#"
        {
            "symbol": "BTCUSDT",
            "startTime": 1734819600000,
            "endTime": 1734819696263,
            "fromId": 12345,
            "limit": 500,
            "timestamp": 1734819696263
        }
        "#;

        let request: BinanceTradesRequest = serde_json::from_str(json_data).unwrap();
        println!("Deserialized BinanceTradesRequest: {:?}", request);

        assert_eq!(request.symbol, "BTCUSDT");
        assert_eq!(request.start_time, Some(1734819600000));
        assert_eq!(request.end_time, Some(1734819696263));
        assert_eq!(request.from_id, Some(12345));
        assert_eq!(request.limit, Some(500));
        assert_eq!(request.timestamp, 1734819696263);
    }

    /// Test deserialization of BinanceTradeResponse
    #[test]
    fn test_binance_trade_response_deserialization() {
        let json_data = r#"
        {
            "symbol": "BTCUSDT",
            "id": 12345,
            "orderId": 67890,
            "price": "90000.50",
            "qty": "0.10",
            "quoteQty": "9000.05",
            "commission": "0.001",
            "commissionAsset": "BTC",
            "time": 1734819696263,
            "isBuyer": true,
            "isMaker": false,
            "isBestMatch": true
        }
        "#;

        let trade_response: BinanceTradeResponse = serde_json::from_str(json_data).unwrap();
        println!("Deserialized BinanceTradeResponse: {:?}", trade_response);

        assert_eq!(trade_response.symbol, "BTCUSDT");
        assert_eq!(trade_response.id, 12345);
        assert_eq!(trade_response.order_id, 67890);
        assert_eq!(trade_response.price, Decimal::new(9000050, 2));
        assert_eq!(trade_response.qty, Decimal::new(10, 2));
        assert_eq!(trade_response.quote_qty, Decimal::new(900005, 2));
        assert_eq!(trade_response.commission, Decimal::new(1, 3));
        assert_eq!(trade_response.commission_asset, "BTC");
        assert_eq!(trade_response.time, 1734819696263);
        assert_eq!(trade_response.is_buyer, true);
        assert_eq!(trade_response.is_maker, false);
        assert_eq!(trade_response.is_best_match, true);
    }

    /// Test BinanceTradeResponse to Trade conversion (buy order)
    #[test]
    fn test_binance_trade_response_to_trade_conversion_buy() {
        let binance_trade = BinanceTradeResponse {
            symbol: "BTCUSDT".into(),
            id: 12345,
            order_id: 67890,
            price: Decimal::new(90000, 0),
            qty: Decimal::new(10, 2),
            quote_qty: Decimal::new(9000, 0),
            commission: Decimal::new(1, 3),
            commission_asset: "BTC".into(),
            time: 1734819696263,
            is_buyer: true,
            is_maker: false,
            is_best_match: true,
        };

        let trade: Trade = binance_trade.into();

        assert_eq!(trade.trade_id, "12345");
        assert_eq!(trade.order_id, OrderId::new("67890".into()));
        assert_eq!(trade.pair.base, "BTC");
        assert_eq!(trade.pair.quote, "USDT");
        assert_eq!(trade.price, Decimal::new(90000, 0));
        assert_eq!(trade.quantity, Decimal::new(10, 2));
        assert_eq!(trade.side, Side::Buy);
        assert_eq!(trade.timestamp, 1734819696263);
        assert_eq!(trade.fee, Decimal::new(1, 3));
        assert_eq!(trade.fee_currency, "BTC");
    }

    /// Test BinanceTradeResponse to Trade conversion (sell order)
    #[test]
    fn test_binance_trade_response_to_trade_conversion_sell() {
        let binance_trade = BinanceTradeResponse {
            symbol: "ETHUSDT".into(),
            id: 54321,
            order_id: 98765,
            price: Decimal::new(3000, 0),
            qty: Decimal::new(5, 1),
            quote_qty: Decimal::new(15000, 0),
            commission: Decimal::new(15, 0),
            commission_asset: "USDT".into(),
            time: 1734819700000,
            is_buyer: false,
            is_maker: true,
            is_best_match: false,
        };

        let trade: Trade = binance_trade.into();

        assert_eq!(trade.trade_id, "54321");
        assert_eq!(trade.order_id, OrderId::new("98765".into()));
        assert_eq!(trade.pair.base, "ETH");
        assert_eq!(trade.pair.quote, "USDT");
        assert_eq!(trade.price, Decimal::new(3000, 0));
        assert_eq!(trade.quantity, Decimal::new(5, 1));
        assert_eq!(trade.side, Side::Sell);
        assert_eq!(trade.timestamp, 1734819700000);
        assert_eq!(trade.fee, Decimal::new(15, 0));
        assert_eq!(trade.fee_currency, "USDT");
    }

    /// Test BinanceTradeResponse to Trade conversion with short symbol
    #[test]
    fn test_binance_trade_response_to_trade_conversion_short_symbol() {
        let binance_trade = BinanceTradeResponse {
            symbol: "BTC".into(), // Short symbol, should fallback
            id: 11111,
            order_id: 22222,
            price: Decimal::new(90000, 0),
            qty: Decimal::new(1, 0),
            quote_qty: Decimal::new(90000, 0),
            commission: Decimal::new(1, 2),
            commission_asset: "BTC".into(),
            time: 1734819696263,
            is_buyer: true,
            is_maker: false,
            is_best_match: true,
        };

        let trade: Trade = binance_trade.into();

        // Should handle short symbol gracefully
        assert_eq!(trade.pair.base, "BTC");
        assert_eq!(trade.trade_id, "11111");
    }

    /// Test BinanceTradesResponse (array) deserialization
    #[test]
    fn test_binance_trades_response_array_deserialization() {
        let json_data = r#"
        [
            {
                "symbol": "BTCUSDT",
                "id": 1,
                "orderId": 10,
                "price": "90000",
                "qty": "0.10",
                "quoteQty": "9000",
                "commission": "0.001",
                "commissionAsset": "BTC",
                "time": 1734819696263,
                "isBuyer": true,
                "isMaker": false,
                "isBestMatch": true
            },
            {
                "symbol": "BTCUSDT",
                "id": 2,
                "orderId": 11,
                "price": "90050",
                "qty": "0.05",
                "quoteQty": "4502.5",
                "commission": "0.0005",
                "commissionAsset": "BTC",
                "time": 1734819700000,
                "isBuyer": false,
                "isMaker": true,
                "isBestMatch": true
            }
        ]
        "#;

        let trades: BinanceTradesResponse = serde_json::from_str(json_data).unwrap();
        println!("Deserialized BinanceTradesResponse: {:?}", trades);

        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].id, 1);
        assert_eq!(trades[0].is_buyer, true);
        assert_eq!(trades[1].id, 2);
        assert_eq!(trades[1].is_buyer, false);
    }
}
