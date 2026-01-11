use crate::exchanges::binance::common_types::{
    BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::{
    types::exchange::Order,
    utils::{
        deserialize_option_smol_str, deserialize_smol_str, serialize_option_smol_str,
        serialize_smol_str,
    },
};

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

/// Response structure for a single order (used in the array response)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub order_id: u64,
    pub order_list_id: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub client_order_id: SmolStr,
    pub transact_time: Option<u64>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub orig_qty: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub executed_qty: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub cummulative_quote_qty: Option<Decimal>,
    pub status: Option<BinanceOrderStatus>,
    pub time_in_force: Option<BinanceTimeInForce>,
    #[serde(rename = "type")]
    pub order_type: Option<BinanceOrderType>,
    pub side: Option<BinanceSide>,
    pub working_time: Option<u64>,
}

impl From<BinanceOrderResponse> for Order {
    fn from(binance_res: BinanceOrderResponse) -> Self {
        let status = binance_res
            .status
            .map(|s| s.into())
            .unwrap_or(trading_core::types::common::OrderStatus::New);
        let filled_amount = binance_res.executed_qty.unwrap_or_default();
        // amount should be the full, original order amount (orig_qty), not the executed amount
        let amount = binance_res.orig_qty.unwrap_or_default();

        Order {
            order_id: binance_res.order_id.into(),
            client_order_id: Some(binance_res.client_order_id.clone()),
            symbol: binance_res.symbol.into(),
            side: binance_res.side.unwrap_or_default().into(),
            price: binance_res.price.unwrap_or_default(),
            amount,
            exchange_created_at: binance_res.transact_time,
            internal_created_at: binance_res.working_time.unwrap_or_default(),
            status,
            filled_amount,
            instrument_type: trading_core::types::common::InstrumentType::Spot,
        }
    }
}

/// Response for open orders (array of orders)
pub type BinanceOpenOrdersResponse = Vec<BinanceOrderResponse>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use trading_core::types::common::{OrderId, Side};

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

    /// ✅ Test serialization of BinanceOrderResponse (struct → JSON)
    #[test]
    fn test_binance_order_response_serialization() {
        let order_response = BinanceOrderResponse {
            symbol: "BTCUSDT".into(),
            order_id: 12345,
            order_list_id: -1,
            client_order_id: "test_order".into(),
            transact_time: Some(1734819696263),
            price: Some(Decimal::new(90000, 0)),
            orig_qty: Some(Decimal::new(10, 2)),
            executed_qty: Some(Decimal::new(5, 2)), // Partially filled
            cummulative_quote_qty: Some(Decimal::new(45000, 0)),
            status: Some(BinanceOrderStatus::New),
            time_in_force: Some(BinanceTimeInForce::Gtc),
            order_type: Some(BinanceOrderType::Limit),
            side: Some(BinanceSide::Buy),
            working_time: Some(1734819700000),
        };

        let serialized = serde_json::to_string(&order_response).unwrap();
        println!("Serialized BinanceOrderResponse: {}", serialized);
    }

    /// ✅ Test deserialization of BinanceOrderResponse (JSON → struct)
    #[test]
    fn test_binance_order_response_deserialization() {
        let json_data = r#"
        {
            "symbol": "BTCUSDT",
            "orderId": 12345,
            "orderListId": -1,
            "clientOrderId": "test_order",
            "transactTime": 1734819696263,
            "price": "90000",
            "origQty": "0.10",
            "executedQty": "0.05",
            "cummulativeQuoteQty": "45000",
            "status": "NEW",
            "timeInForce": "GTC",
            "type": "LIMIT",
            "side": "BUY",
            "workingTime": 1734819700000
        }
        "#;

        let order_response: BinanceOrderResponse = serde_json::from_str(json_data).unwrap();
        println!("Deserialized BinanceOrderResponse: {:?}", order_response);

        assert_eq!(order_response.symbol, "BTCUSDT");
        assert_eq!(order_response.order_id, 12345);
        assert_eq!(order_response.client_order_id, "test_order");
        assert_eq!(order_response.transact_time, Some(1734819696263));
        assert_eq!(order_response.price, Some(Decimal::new(90000, 0)));
        assert_eq!(order_response.orig_qty, Some(Decimal::new(10, 2)));
        assert_eq!(order_response.executed_qty, Some(Decimal::new(5, 2)));
        assert_eq!(
            order_response.cummulative_quote_qty,
            Some(Decimal::new(45000, 0))
        );
        assert_eq!(order_response.status, Some(BinanceOrderStatus::New));
        assert_eq!(order_response.time_in_force, Some(BinanceTimeInForce::Gtc));
        assert_eq!(order_response.order_type, Some(BinanceOrderType::Limit));
        assert_eq!(order_response.side, Some(BinanceSide::Buy));
        assert_eq!(order_response.working_time, Some(1734819700000));
    }

    /// ✅ Test `BinanceOrderResponse` to `Order` conversion
    #[test]
    fn test_binance_order_response_to_order_conversion() {
        let binance_response = BinanceOrderResponse {
            symbol: "BTCUSDT".into(),
            order_id: 12345,
            order_list_id: -1,
            client_order_id: "test_order".into(),
            transact_time: Some(1734819696263),
            price: Some(Decimal::new(90000, 0)),
            orig_qty: Some(Decimal::new(10, 2)),
            executed_qty: Some(Decimal::new(5, 2)), // Partially filled
            cummulative_quote_qty: Some(Decimal::new(45000, 0)),
            status: Some(BinanceOrderStatus::New),
            time_in_force: Some(BinanceTimeInForce::Gtc),
            order_type: Some(BinanceOrderType::Limit),
            side: Some(BinanceSide::Buy),
            working_time: Some(1734819700000),
        };

        let order: Order = binance_response.into();

        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, Decimal::new(90000, 0));
        assert_eq!(order.amount, Decimal::new(10, 2)); // Full original order amount (orig_qty)
        assert_eq!(order.filled_amount, Decimal::new(5, 2)); // Executed/filled amount
        assert_eq!(order.order_id, OrderId::new("12345".into()));
        assert_eq!(order.exchange_created_at, Some(1734819696263));
        assert_eq!(order.internal_created_at, 1734819700000_u64);
    }
}
