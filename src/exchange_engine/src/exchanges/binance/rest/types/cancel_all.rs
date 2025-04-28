use crate::exchanges::binance::common_types::{
    BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce,
};
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCancelAllOrdersRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCancelAllOrdersResponse {
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

    // Test case for deserializing a `BinanceCancelAllOrdersRequest` struct from JSON
    #[test]
    fn test_binance_cancel_all_orders_request_deserialization() {
        let json = r#"
        {
            "symbol": "BTCUSDT",
            "timestamp": 1627849182375
        }
        "#;

        let cancel_request: BinanceCancelAllOrdersRequest = serde_json::from_str(json).unwrap();

        assert_eq!(cancel_request.symbol, SmolStr::new("BTCUSDT"));
        assert_eq!(cancel_request.timestamp, 1627849182375);
    }

    // Test case for deserializing a `BinanceCancelAllOrdersResponse` struct from JSON
    #[test]
    fn test_binance_cancel_all_orders_response_deserialization() {
        let json = r#"
        {
            "symbol": "BTCUSDT",
            "orderId": 123456789,
            "price": "90000.00000000",
            "origQty": "0.10",
            "status": "FILLED",
            "timeInForce": "GTC",
            "type": "LIMIT",
            "side": "BUY"
        }
        "#;

        let response: BinanceCancelAllOrdersResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.symbol, SmolStr::new("BTCUSDT"));
        assert_eq!(response.order_id, 123456789);
        assert_eq!(response.price, Decimal::new(900000000, 4));
        assert_eq!(response.orig_qty, Decimal::new(10, 2));
        assert_eq!(response.status, BinanceOrderStatus::Filled);
        assert_eq!(response.time_in_force, BinanceTimeInForce::Gtc);
        assert_eq!(response.type_, BinanceOrderType::Limit);
        assert_eq!(response.side, BinanceSide::Buy);
    }

    // Test case for handling the `BinanceCancelAllOrdersRequest` with missing optional fields
    #[test]
    fn test_binance_cancel_all_orders_request_with_missing_optional_fields() {
        let json = r#"
        {
            "symbol": "BTCUSDT",
            "timestamp": 1627849182375
        }
        "#;

        let cancel_request: BinanceCancelAllOrdersRequest = serde_json::from_str(json).unwrap();

        // Optional fields are not present, so omit_zero_balances should be None
        assert_eq!(cancel_request.symbol, SmolStr::new("BTCUSDT"));
        assert_eq!(cancel_request.timestamp, 1627849182375);
    }

    // Test case for deserializing a `BinanceCancelAllOrdersResponse` with different values
    #[test]
    fn test_binance_cancel_all_orders_response_with_different_values() {
        let json = r#"
        {
            "symbol": "ETHUSDT",
            "orderId": 987654321,
            "price": "3000.00000000",
            "origQty": "5.00",
            "status": "CANCELED",
            "timeInForce": "IOC",
            "type": "LIMIT",
            "side": "SELL"
        }
        "#;

        let response: BinanceCancelAllOrdersResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.symbol, SmolStr::new("ETHUSDT"));
        assert_eq!(response.order_id, 987654321);
        assert_eq!(response.price, Decimal::new(30000000000, 7));
        assert_eq!(response.orig_qty, Decimal::new(500, 2));
        assert_eq!(response.status, BinanceOrderStatus::Canceled);
        assert_eq!(response.time_in_force, BinanceTimeInForce::Ioc);
        assert_eq!(response.type_, BinanceOrderType::Limit);
        assert_eq!(response.side, BinanceSide::Sell);
    }

    // Test case for handling the conversion of `BinanceOrderStatus` to string
    #[test]
    fn test_binance_order_status_to_string() {
        let status = BinanceOrderStatus::Filled;
        let status_str = format!("{:?}", status);
        assert_eq!(status_str, "Filled");

        let status = BinanceOrderStatus::Canceled;
        let status_str = format!("{:?}", status);
        assert_eq!(status_str, "Canceled");
    }

    // Test case for handling `BinanceTimeInForce` as a string
    #[test]
    fn test_binance_time_in_force_to_string() {
        let time_in_force = BinanceTimeInForce::Gtc;
        let time_in_force_str = format!("{:?}", time_in_force);
        assert_eq!(time_in_force_str, "Gtc");

        let time_in_force = BinanceTimeInForce::Ioc;
        let time_in_force_str = format!("{:?}", time_in_force);
        assert_eq!(time_in_force_str, "Ioc");
    }
}
