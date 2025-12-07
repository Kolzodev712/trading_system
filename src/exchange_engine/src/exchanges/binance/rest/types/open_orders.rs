use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_option_smol_str, serialize_option_smol_str};

use super::open_order::BinanceOrderResponse;

/// Request parameters for fetching open orders
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOpenOrdersRequest {
    /// Trading pair symbol (optional - if omitted, returns all open orders)
    #[serde(
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub symbol: Option<SmolStr>,
    /// Timestamp for request signing
    pub timestamp: u64,
}

/// Response for open orders (array of orders)
pub type BinanceOpenOrdersResponse = Vec<BinanceOrderResponse>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    /// Test serialization of BinanceOpenOrdersRequest with symbol
    #[test]
    fn test_binance_open_orders_request_serialization_with_symbol() {
        let request = BinanceOpenOrdersRequest {
            symbol: Some("BTCUSDT".into()),
            timestamp: 1734819696263,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        println!("Serialized BinanceOpenOrdersRequest: {}", serialized);

        assert!(serialized.contains("BTCUSDT"));
        assert!(serialized.contains("1734819696263"));
    }

    /// Test serialization of BinanceOpenOrdersRequest without symbol (all orders)
    #[test]
    fn test_binance_open_orders_request_serialization_without_symbol() {
        let request = BinanceOpenOrdersRequest {
            symbol: None,
            timestamp: 1734819696263,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        println!(
            "Serialized BinanceOpenOrdersRequest (no symbol): {}",
            serialized
        );

        // Symbol should be omitted when None
        assert!(!serialized.contains("symbol"));
        assert!(serialized.contains("1734819696263"));
    }

    /// Test deserialization of BinanceOpenOrdersRequest
    #[test]
    fn test_binance_open_orders_request_deserialization() {
        let json_data = r#"
        {
            "symbol": "BTCUSDT",
            "timestamp": 1734819696263
        }
        "#;

        let request: BinanceOpenOrdersRequest = serde_json::from_str(json_data).unwrap();
        println!("Deserialized BinanceOpenOrdersRequest: {:?}", request);

        assert_eq!(request.symbol, Some("BTCUSDT".into()));
        assert_eq!(request.timestamp, 1734819696263);
    }

    /// Test deserialization of BinanceOpenOrdersRequest without symbol
    #[test]
    fn test_binance_open_orders_request_deserialization_without_symbol() {
        let json_data = r#"
        {
            "timestamp": 1734819696263
        }
        "#;

        let request: BinanceOpenOrdersRequest = serde_json::from_str(json_data).unwrap();
        println!("Deserialized BinanceOpenOrdersRequest: {:?}", request);

        assert_eq!(request.symbol, None);
        assert_eq!(request.timestamp, 1734819696263);
    }
}
