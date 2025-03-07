use super::common::{BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce};
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCancelOrderByIdRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub order_id: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCancelOrderByIdResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub order_id: u64,
    pub price: Decimal,
    pub orig_qty: Decimal,
    pub status: BinanceOrderStatus,
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    pub type_: BinanceOrderType,
    pub side: BinanceSide,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json;
    use smol_str::SmolStr;

    // Test case for deserializing a `BinanceCancelOrderByIdRequest`
    #[test]
    fn test_binance_cancel_order_by_id_request_deserialization() {
        let json = r#"
        {
            "symbol": "BTCUSDT",
            "orderId": 123456789,
            "timestamp": 1623456789000
        }
        "#;

        let request: BinanceCancelOrderByIdRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.symbol, SmolStr::new("BTCUSDT"));
        assert_eq!(request.order_id, 123456789);
        assert_eq!(request.timestamp, 1623456789000);
    }

    // Test case for deserializing a `BinanceCancelOrderByIdResponse`
    #[test]
    fn test_binance_cancel_order_by_id_response_deserialization() {
        let json = r#"
        {
            "symbol": "BTCUSDT",
            "orderId": 123456789,
            "price": "1000.00000000",
            "origQty": "10.00",
            "status": "FILLED",
            "timeInForce": "GTC",
            "type": "LIMIT",
            "side": "BUY"
        }
        "#;

        let response: BinanceCancelOrderByIdResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.symbol, SmolStr::new("BTCUSDT"));
        assert_eq!(response.order_id, 123456789);
        assert_eq!(response.price, Decimal::new(100000000000, 8));
        assert_eq!(response.orig_qty, Decimal::new(1000, 2));
        assert_eq!(response.status, BinanceOrderStatus::Filled);
        assert_eq!(response.time_in_force, BinanceTimeInForce::Gtc);
        assert_eq!(response.type_, BinanceOrderType::Limit);
        assert_eq!(response.side, BinanceSide::Buy);
    }

    // Test case for deserialization with invalid JSON (Error Case)
    #[test]
    fn test_binance_cancel_order_by_id_response_invalid_json() {
        let invalid_json = r#"
        {
            "symbol": "BTCUSDT",
            "orderId": "123456789",  // orderId should be a number, not a string
            "price": "1000.00000000",
            "origQty": "10.00",
            "status": "FILLED",
            "timeInForce": "GTC",
            "type": "LIMIT",
            "side": "BUY"
        }
        "#;

        let result: Result<BinanceCancelOrderByIdResponse, _> = serde_json::from_str(invalid_json);

        assert!(result.is_err());
    }

    // Test case for deserialization with missing fields (Error Case)
    #[test]
    fn test_binance_cancel_order_by_id_response_missing_field() {
        let json_with_missing_field = r#"
        {
            "symbol": "BTCUSDT",
            "price": "1000.00000000",
            "origQty": "10.00",
            "status": "FILLED",
            "timeInForce": "GTC",
            "type": "LIMIT",
            "side": "BUY"
        }
        "#;

        let result: Result<BinanceCancelOrderByIdResponse, _> =
            serde_json::from_str(json_with_missing_field);

        assert!(result.is_err());
    }

    // Test case for correct mapping between request and response
    #[test]
    fn test_binance_cancel_order_by_id_request_response_mapping() {
        let request = BinanceCancelOrderByIdRequest {
            symbol: SmolStr::new("BTCUSDT"),
            order_id: 123456789,
            timestamp: 1623456789000,
        };

        // Simulate response data
        let response = BinanceCancelOrderByIdResponse {
            symbol: SmolStr::new("BTCUSDT"),
            order_id: 123456789,
            price: Decimal::new(1000000000, 8),
            orig_qty: Decimal::new(1000, 2),
            status: BinanceOrderStatus::Filled,
            time_in_force: BinanceTimeInForce::Gtc,
            type_: BinanceOrderType::Limit,
            side: BinanceSide::Buy,
        };

        assert_eq!(request.symbol, response.symbol);
        assert_eq!(request.order_id, response.order_id);
        assert_eq!(response.price, Decimal::new(1000000000, 8));
        assert_eq!(response.orig_qty, Decimal::new(1000, 2));
    }

    // Test case for deserializing a `BinanceCancelOrderByIdRequest` with invalid `orderId` type
    #[test]
    fn test_binance_cancel_order_by_id_request_invalid_order_id() {
        let json = r#"
        {
            "symbol": "BTCUSDT",
            "orderId": "INVALID_ID",  // orderId should be a number
            "timestamp": 1623456789000
        }
        "#;

        let result: Result<BinanceCancelOrderByIdRequest, _> = serde_json::from_str(json);

        assert!(result.is_err());
    }
}
