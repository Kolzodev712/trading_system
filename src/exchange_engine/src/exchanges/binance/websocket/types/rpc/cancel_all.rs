use crate::exchanges::binance::common_types::{
    BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce,
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
    pub result: BinanceWsCancelAllOrdersResult,
    pub rate_limits: BinanceRateLimit,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum BinanceWsCancelAllOrdersResult {
    SingleCancel(SingleCancelOrder),
    OcoCancel(OcoCancelOrder),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleCancelOrder {
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
    pub price: Decimal,
    pub orig_qty: Decimal,
    pub status: BinanceOrderStatus,
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    pub type_: BinanceOrderType,
    pub side: BinanceSide,
    pub stop_price: Decimal,
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
    pub price: Decimal,
    pub orig_qty: Decimal,
    pub executed_qty: Decimal,
    pub orig_quote_order_qty: Decimal,
    pub cummulative_quote_qty: Decimal,
    pub status: BinanceOrderStatus,
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    pub type_: BinanceOrderType,
    pub side: BinanceSide,
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
            result: BinanceWsCancelAllOrdersResult::SingleCancel(SingleCancelOrder {
                symbol: SmolStr::new("BTCUSDT"),
                orig_client_order_id: SmolStr::new("original-client-id"),
                order_id: 12345678,
                order_list_id: -1,
                client_order_id: SmolStr::new("new-client-id"),
                transact_time: 1625256000000,
                price: Decimal::new(234161000, 5),
                orig_qty: Decimal::new(847000, 5),
                status: BinanceOrderStatus::Canceled,
                time_in_force: BinanceTimeInForce::Gtc,
                type_: BinanceOrderType::Limit,
                side: BinanceSide::Sell,
                stop_price: Decimal::new(0, 8),
            }),
            rate_limits: BinanceRateLimit {
                rate_limit_type: SmolStr::new("REQUEST_WEIGHT"),
                interval: SmolStr::new("MINUTE"),
                interval_num: 1,
                limit: 6000,
                count: 1,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let back: BinanceWsCancelAllOrdersResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, back);
    }

    #[test]
    fn test_binance_ws_cancel_all_orders_response_oco_cancel_ser_deser() {
        let response = BinanceWsCancelAllOrdersResponse {
            id: SmolStr::new("cancelall-res-456"),
            status: 200,
            result: BinanceWsCancelAllOrdersResult::OcoCancel(OcoCancelOrder {
                order_list_id: 19431,
                list_client_order_id: SmolStr::new("oco-client-id"),
                transaction_time: 1660803702431,
                symbol: SmolStr::new("BTCUSDT"),
                orders: vec![
                    OcoOrder {
                        symbol: SmolStr::new("BTCUSDT"),
                        order_id: 123456789,
                        client_order_id: SmolStr::new("order-client-id-1"),
                    },
                    OcoOrder {
                        symbol: SmolStr::new("BTCUSDT"),
                        order_id: 123456790,
                        client_order_id: SmolStr::new("order-client-id-2"),
                    },
                ],
                order_reports: vec![
                    OcoOrderReport {
                        symbol: SmolStr::new("BTCUSDT"),
                        orig_client_order_id: SmolStr::new("orig-id-1"),
                        order_id: 123456789,
                        order_list_id: 19431,
                        client_order_id: SmolStr::new("client-id-1"),
                        transact_time: 1684804350068,
                        price: Decimal::new(234505000, 5),
                        orig_qty: Decimal::new(850000, 5),
                        executed_qty: Decimal::new(0, 5),
                        orig_quote_order_qty: Decimal::new(0, 6),
                        cummulative_quote_qty: Decimal::new(0, 6),
                        status: BinanceOrderStatus::Canceled,
                        time_in_force: BinanceTimeInForce::Gtc,
                        type_: BinanceOrderType::StopLossLimit,
                        side: BinanceSide::Buy,
                        stop_price: Decimal::new(234300000, 5),
                    },
                    OcoOrderReport {
                        symbol: SmolStr::new("BTCUSDT"),
                        orig_client_order_id: SmolStr::new("orig-id-2"),
                        order_id: 123456790,
                        order_list_id: 19431,
                        client_order_id: SmolStr::new("client-id-2"),
                        transact_time: 1684804350068,
                        price: Decimal::new(234000000, 5),
                        orig_qty: Decimal::new(850000, 5),
                        executed_qty: Decimal::new(0, 5),
                        orig_quote_order_qty: Decimal::new(0, 6),
                        cummulative_quote_qty: Decimal::new(0, 6),
                        status: BinanceOrderStatus::Canceled,
                        time_in_force: BinanceTimeInForce::Gtc,
                        type_: BinanceOrderType::LimitMaker,
                        side: BinanceSide::Buy,
                        stop_price: Decimal::new(0, 8),
                    },
                ],
            }),
            rate_limits: BinanceRateLimit {
                rate_limit_type: SmolStr::new("REQUEST_WEIGHT"),
                interval: SmolStr::new("MINUTE"),
                interval_num: 1,
                limit: 6000,
                count: 1,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let back: BinanceWsCancelAllOrdersResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, back);
    }
}
