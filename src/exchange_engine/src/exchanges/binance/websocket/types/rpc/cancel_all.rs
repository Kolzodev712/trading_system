use crate::exchanges::binance::common_types::{
    BinanceOrderStatus, BinanceOrderType, BinanceSelfTradePreventionMode, BinanceSide,
    BinanceTimeInForce,
};
use trading_core::types::common::Symbol;

use super::super::common::{BinanceRateLimit, SessionLogonParams};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

// https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/trading-requests#cancel-open-orders-trade
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsCancelAllOrdersRequest {
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
    pub params: BinanceWsCancelAllOrdersReqParams,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsCancelAllOrdersReqParams {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(flatten)]
    pub session: SessionLogonParams,
}

impl BinanceWsCancelAllOrdersReqParams {
    pub fn new(symbol: SmolStr) -> Self {
        Self {
            symbol,
            session: SessionLogonParams::default(),
        }
    }
}

impl From<Symbol> for BinanceWsCancelAllOrdersReqParams {
    fn from(symbol: Symbol) -> Self {
        Self {
            symbol: SmolStr::from(&symbol),
            session: SessionLogonParams::default(),
        }
    }
}

// https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/trading-requests#cancel-order-trade
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsCancelAllOrdersResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    pub status: i64,
    // Binance returns an array of cancelled orders
    pub result: Vec<SingleCancelOrder>,
    #[serde(default, rename = "rateLimits")]
    pub rate_limits: Option<Vec<BinanceRateLimit>>,
}

// Removed unused enum - cancel_all always returns Vec<SingleCancelOrder>
// OCO structures are not used in cancel_all responses

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleCancelOrder {
    // Fields always present
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
    // Conditional fields (only appear if set)
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub stop_price: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing_delta: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing_time: Option<i64>,
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
pub struct OcoCancelOrder {
    pub order_list_id: i64,
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
    // order id that was cancelled
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
    #[serde(with = "rust_decimal::serde::str")]
    pub orig_quote_order_qty: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cummulative_quote_qty: Decimal,
    pub status: BinanceOrderStatus,
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    pub type_: BinanceOrderType,
    pub side: BinanceSide,
    #[serde(with = "rust_decimal::serde::str")]
    pub stop_price: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json;
    use smol_str::SmolStr;

    #[test]
    fn test_binance_ws_cancel_all_orders_request_ser_deser() {
        let request = BinanceWsCancelAllOrdersRequest {
            id: SmolStr::new("cancelall-req-123"),
            method: SmolStr::new("order.cancelAll"),
            params: BinanceWsCancelAllOrdersReqParams {
                symbol: SmolStr::new("BTCUSDT"),
                session: SessionLogonParams {
                    api_key: SmolStr::new("api-key"),
                    timestamp: 1234567890,
                    signature: SmolStr::new("signature"),
                },
            },
        };

        let json = serde_json::to_string(&request).unwrap();
        let back: BinanceWsCancelAllOrdersRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, back);
    }

    #[test]
    fn test_binance_ws_cancel_all_orders_response_single_cancel_ser_deser() {
        let response = BinanceWsCancelAllOrdersResponse {
            id: SmolStr::new("cancelall-res-123"),
            status: 200,
            result: vec![SingleCancelOrder {
                symbol: SmolStr::new("BTCUSDT"),
                orig_client_order_id: SmolStr::new("original-client-id"),
                order_id: 12345678,
                order_list_id: -1,
                client_order_id: SmolStr::new("new-client-id"),
                transact_time: 1625256000000,
                price: Decimal::new(234161000, 5),
                orig_qty: Decimal::new(847000, 5),
                executed_qty: Decimal::new(0, 0),
                orig_quote_order_qty: None,
                cummulative_quote_qty: None,
                status: BinanceOrderStatus::Canceled,
                time_in_force: BinanceTimeInForce::Gtc,
                type_: BinanceOrderType::Limit,
                side: BinanceSide::Sell,
                stop_price: None,
                trailing_delta: None,
                trailing_time: None,
                iceberg_qty: None,
                strategy_id: None,
                strategy_type: None,
                self_trade_prevention_mode: None,
            }],
            rate_limits: Some(vec![BinanceRateLimit {
                rate_limit_type: SmolStr::new("REQUEST_WEIGHT"),
                interval: SmolStr::new("MINUTE"),
                interval_num: 1,
                limit: 6000,
                count: 1,
            }]),
        };

        let json = serde_json::to_string(&response).unwrap();
        let back: BinanceWsCancelAllOrdersResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, back);
    }

    #[test]
    #[ignore] // OCO structure not used in actual cancel_all response (returns array of SingleCancelOrder)
    fn test_binance_ws_cancel_all_orders_response_oco_cancel_ser_deser() {
        // This test is kept for reference but OCO structure is not used in actual responses
        // Actual cancel_all response returns Vec<SingleCancelOrder>, not OCO structure
    }
}
