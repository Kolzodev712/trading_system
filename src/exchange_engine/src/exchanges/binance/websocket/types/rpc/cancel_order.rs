use crate::exchanges::binance::common::types::{
    BinanceCancelRestrictions, BinanceOrderStatus, BinanceOrderType,
    BinanceSelfTradePreventionMode, BinanceSide, BinanceTimeInForce,
};
use trading_core::types::common::Symbol;

use super::super::common::{BinanceRateLimit, SessionLogonParams};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

// https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/trading-requests#cancel-order-trade
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsCancelOrderRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub method: SmolStr,
    pub params: BinanceWsCancelOrderReqParams,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsCancelOrderReqParams {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    // Either orderId OR origClientOrderId must be provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_client_order_id: Option<SmolStr>,
    // Optional fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_client_order_id: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel_restrictions: Option<BinanceCancelRestrictions>,
    #[serde(flatten)]
    pub session: SessionLogonParams,
}

impl BinanceWsCancelOrderReqParams {
    pub fn new_with_client_order_id(
        symbol: SmolStr,
        orig_client_order_id: impl Into<SmolStr>,
    ) -> Self {
        Self {
            symbol,
            order_id: None,
            orig_client_order_id: Some(orig_client_order_id.into()),
            new_client_order_id: None,
            cancel_restrictions: None,
            session: SessionLogonParams::default(),
        }
    }

    pub fn new_with_order_id(symbol: SmolStr, order_id: i64) -> Self {
        Self {
            symbol,
            order_id: Some(order_id),
            orig_client_order_id: None,
            new_client_order_id: None,
            cancel_restrictions: None,
            session: SessionLogonParams::default(),
        }
    }
}

impl From<(Symbol, SmolStr)> for BinanceWsCancelOrderReqParams {
    fn from((symbol, client_id): (Symbol, SmolStr)) -> Self {
        Self::new_with_client_order_id(SmolStr::from(&symbol), client_id)
    }
}

// https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/trading-requests#cancel-order-trade
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsCancelOrderResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    pub status: i64,
    // Response can be a single order OR an order list (optional for error responses)
    #[serde(default)]
    pub result: Option<BinanceWsCancelOrderResult>,
    // Error field for error responses
    #[serde(default)]
    pub error: Option<BinanceWsCancelOrderError>,
    #[serde(default, rename = "rateLimits")]
    pub rate_limits: Option<Vec<BinanceRateLimit>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsCancelOrderError {
    pub code: i64,
    #[serde(
        default,
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub msg: SmolStr,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum BinanceWsCancelOrderResult {
    // Single order cancellation
    SingleOrder(SingleCancelOrderResult),
    // Order list cancellation (OCO)
    OrderList(OrderListCancelResult),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleCancelOrderResult {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub orig_client_order_id: SmolStr,
    pub order_id: i64,
    pub order_list_id: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub client_order_id: SmolStr,
    pub transact_time: i64,
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub orig_qty: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub executed_qty: Decimal,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub orig_quote_order_qty: Option<Decimal>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub cummulative_quote_qty: Option<Decimal>,
    pub status: BinanceOrderStatus,
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    pub type_: BinanceOrderType,
    pub side: BinanceSide,
    // Conditional fields
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub stop_price: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing_delta: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub iceberg_qty: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_type: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_trade_prevention_mode: Option<BinanceSelfTradePreventionMode>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderListCancelResult {
    pub order_list_id: i64,
    pub contingency_type: SmolStr,
    pub list_status_type: SmolStr,
    pub list_order_status: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub list_client_order_id: SmolStr,
    pub transaction_time: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub orders: Vec<OcoOrder>,
    pub order_reports: Vec<OcoOrderReport>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcoOrder {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub order_id: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub client_order_id: SmolStr,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcoOrderReport {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub orig_client_order_id: SmolStr,
    pub order_id: i64,
    pub order_list_id: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub client_order_id: SmolStr,
    pub transact_time: i64,
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub orig_qty: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub executed_qty: Decimal,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub orig_quote_order_qty: Option<Decimal>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub cummulative_quote_qty: Option<Decimal>,
    pub status: BinanceOrderStatus,
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    pub type_: BinanceOrderType,
    pub side: BinanceSide,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub stop_price: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_trade_prevention_mode: Option<BinanceSelfTradePreventionMode>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json;
    use smol_str::SmolStr;

    #[test]
    fn test_binance_ws_cancel_order_request_ser_deser() {
        let request = BinanceWsCancelOrderRequest {
            id: SmolStr::new("cancel-123"),
            method: SmolStr::new("order.cancel"),
            params: BinanceWsCancelOrderReqParams {
                symbol: SmolStr::new("BTCUSDT"),
                order_id: None,
                orig_client_order_id: Some(SmolStr::new("myorder-789")),
                new_client_order_id: None,
                cancel_restrictions: None,
                session: SessionLogonParams {
                    api_key: SmolStr::new("api-key"),
                    timestamp: 1234567890,
                    signature: SmolStr::new("signature"),
                },
            },
        };

        let json = serde_json::to_string(&request).unwrap();
        let back: BinanceWsCancelOrderRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, back);
    }

    #[test]
    fn test_binance_ws_cancel_order_response_ser_deser() {
        let response = BinanceWsCancelOrderResponse {
            id: SmolStr::new("cancel-resp-123"),
            status: 200,
            result: Some(BinanceWsCancelOrderResult::SingleOrder(
                SingleCancelOrderResult {
                    symbol: SmolStr::new("BTCUSDT"),
                    orig_client_order_id: SmolStr::new("myorder-789"),
                    order_id: 12569099453,
                    order_list_id: -1,
                    client_order_id: SmolStr::new("order-123"),
                    transact_time: 1684804350068,
                    price: Decimal::new(200005, 4),
                    orig_qty: Decimal::new(100, 2),
                    executed_qty: Decimal::new(10, 3),
                    orig_quote_order_qty: None,
                    cummulative_quote_qty: Some(Decimal::new(234161, 5)),
                    status: BinanceOrderStatus::Canceled,
                    time_in_force: BinanceTimeInForce::Gtc,
                    type_: BinanceOrderType::Limit,
                    side: BinanceSide::Buy,
                    stop_price: None,
                    trailing_delta: None,
                    iceberg_qty: None,
                    strategy_id: None,
                    strategy_type: None,
                    self_trade_prevention_mode: Some(BinanceSelfTradePreventionMode::None),
                },
            )),
            error: None,
            rate_limits: Some(vec![BinanceRateLimit {
                rate_limit_type: SmolStr::new("ORDERS"),
                interval: SmolStr::new("MINUTE"),
                interval_num: 1,
                limit: 1200,
                count: 50,
            }]),
        };

        let json = serde_json::to_string(&response).unwrap();
        let back: BinanceWsCancelOrderResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, back);
    }
}
