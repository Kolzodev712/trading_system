use crate::exchanges::binance::common_types::{
    BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce,
};
use core_logic::{
    data::{
        base_data::{OrderType, Side},
        private_data::{Order, OrderRequest},
    },
    utils::{
        deserialize_smol_str, serialize_enum_as_display, serialize_option_enum_as_display,
        serialize_smol_str,
    },
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(serialize_with = "serialize_enum_as_display")]
    pub side: BinanceSide,
    #[serde(rename = "type", serialize_with = "serialize_enum_as_display")]
    pub order_type: BinanceOrderType,
    #[serde(serialize_with = "serialize_option_enum_as_display")]
    pub time_in_force: Option<BinanceTimeInForce>,
    pub quantity: Option<Decimal>,
    pub price: Option<Decimal>,
    pub timestamp: Option<u64>,
}

impl From<OrderRequest> for BinanceOrderRequest {
    fn from(order: OrderRequest) -> Self {
        BinanceOrderRequest {
            symbol: order.symbol.into(),
            side: match order.side {
                Side::Buy => BinanceSide::Buy,
                Side::Sell => BinanceSide::Sell,
            },
            order_type: match order.order_type {
                OrderType::Limit => BinanceOrderType::Limit,
                OrderType::Market => BinanceOrderType::Market,
                _ => BinanceOrderType::Limit,
            },
            time_in_force: order.exchange_params.get("timeInForce").and_then(|tif| {
                match tif.as_str() {
                    "GTC" => Some(BinanceTimeInForce::Gtc),
                    "IOC" => Some(BinanceTimeInForce::Ioc),
                    "FOK" => Some(BinanceTimeInForce::Fok),
                    _ => None,
                }
            }),
            quantity: Some(order.amount),
            price: Some(order.price),
            timestamp: None,
        }
    }
}

impl From<BinanceOrderRequest> for OrderRequest {
    fn from(binance_order: BinanceOrderRequest) -> Self {
        OrderRequest {
            symbol: binance_order.symbol.into(),
            side: match binance_order.side {
                BinanceSide::Buy => Side::Buy,
                BinanceSide::Sell => Side::Sell,
            },
            order_type: match binance_order.order_type {
                BinanceOrderType::Limit => OrderType::Limit,
                BinanceOrderType::Market => OrderType::Market,
                _ => OrderType::Limit,
            },
            amount: binance_order.quantity.unwrap_or_default(),
            price: binance_order.price.unwrap_or_default(),
            exchange_params: {
                let mut params = std::collections::HashMap::new();
                if let Some(time_in_force) = binance_order.time_in_force {
                    params.insert(
                        "timeInForce".to_string(),
                        match time_in_force {
                            BinanceTimeInForce::Gtc => "GTC".to_string(),
                            BinanceTimeInForce::Ioc => "IOC".to_string(),
                            BinanceTimeInForce::Fok => "FOK".to_string(),
                        },
                    );
                }
                if let Some(recv_window) = binance_order.timestamp {
                    params.insert("recvWindow".to_string(), recv_window.to_string());
                }
                params
            },
            timestamp: binance_order.timestamp,
        }
    }
}

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
    pub price: Option<Decimal>,
    pub orig_qty: Option<Decimal>,
    pub executed_qty: Option<Decimal>,
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
            .unwrap_or(core_logic::data::base_data::OrderStatus::New);
        let filled_amount = binance_res.executed_qty.unwrap_or_default();

        Order {
            symbol: binance_res.symbol.into(),
            side: binance_res.side.unwrap_or_default().into(),
            price: binance_res.price.unwrap_or_default(),
            amount: binance_res.executed_qty.unwrap_or_default(),
            order_id: binance_res.order_id.into(),
            exchange_created_at: binance_res.transact_time,
            internal_created_at: binance_res.working_time.unwrap_or_default(),
            status,
            filled_amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_logic::data::base_data::OrderId;
    use serde_json;

    /// Test serialization of BinanceOrderRequest (struct → JSON)
    #[test]
    fn test_binance_order_request_serialization() {
        let order_request = BinanceOrderRequest {
            symbol: "BTCUSDT".into(),
            side: BinanceSide::Buy,
            order_type: BinanceOrderType::Limit,
            time_in_force: Some(BinanceTimeInForce::Gtc),
            quantity: Some(Decimal::new(10, 2)),
            price: Some(Decimal::new(90000, 0)),
            timestamp: Some(1734819696263),
        };

        let serialized = serde_json::to_string(&order_request).unwrap();
        println!("Serialized BinanceOrderRequest: {}", serialized);

        assert_eq!(
            serialized,
            r#"{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","timeInForce":"GTC","quantity":"0.10","price":"90000","timestamp":1734819696263}"#
        );
    }

    /// ✅ Test deserialization of BinanceOrderRequest (JSON → struct)
    #[test]
    fn test_binance_order_request_deserialization() {
        let json_data = r#"
        {
            "symbol": "BTCUSDT",
            "side": "BUY",
            "type": "LIMIT",
            "timeInForce": "GTC",
            "quantity": "0.10",
            "price": "90000",
            "timestamp": 1734819696263
        }
        "#;

        let order_request: BinanceOrderRequest = serde_json::from_str(json_data).unwrap();
        println!("Deserialized BinanceOrderRequest: {:?}", order_request);

        assert_eq!(order_request.symbol, "BTCUSDT");
        assert_eq!(order_request.side, BinanceSide::Buy);
        assert_eq!(order_request.order_type, BinanceOrderType::Limit);
        assert_eq!(order_request.time_in_force, Some(BinanceTimeInForce::Gtc));
        assert_eq!(order_request.quantity, Some(Decimal::new(10, 2)));
        assert_eq!(order_request.price, Some(Decimal::new(90000, 0)));
        assert_eq!(order_request.timestamp, Some(1734819696263));
    }

    /// ✅ Test `OrderRequest` to `BinanceOrderRequest` conversion
    #[test]
    fn test_order_request_to_binance_order_request_conversion() {
        let mut exchange_params = std::collections::HashMap::new();
        exchange_params.insert("timeInForce".to_string(), "GTC".to_string());

        let order_request = core_logic::data::private_data::OrderRequest {
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            order_type: OrderType::Limit,
            timestamp: None,
            exchange_params,
        };

        let binance_order: BinanceOrderRequest = order_request.into();

        assert_eq!(binance_order.symbol, "BTCUSDT");
        assert_eq!(binance_order.side, BinanceSide::Buy);
        assert_eq!(binance_order.order_type, BinanceOrderType::Limit);
        assert_eq!(binance_order.time_in_force, Some(BinanceTimeInForce::Gtc));
        assert_eq!(binance_order.quantity, Some(Decimal::new(10, 2)));
        assert_eq!(binance_order.price, Some(Decimal::new(90000, 0)));
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
        assert_eq!(order.amount, Decimal::new(5, 2)); // Only executed amount matters
        assert_eq!(order.order_id, OrderId::new("12345".into()));
        assert_eq!(order.exchange_created_at, Some(1734819696263));
        assert_eq!(order.internal_created_at, 1734819700000_u64);
    }
}
