use crate::exchanges::binance::common_types::{
    BinanceOrderType, BinanceSide, BinanceSymbol, BinanceTimeInForce,
};

use super::super::common::{BinanceRateLimit, SessionLogonParams};
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/trading-requests#place-new-order-trade
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsOrderRequest {
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
    pub params: BinanceWsOrderReqParams,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsOrderReqParams {
    symbol: BinanceSymbol,
    side: BinanceSide,
    order_type: BinanceOrderType,
    time_in_force: BinanceTimeInForce,
    price: Decimal,
    quantity: Decimal,
    #[serde(flatten)]
    session: SessionLogonParams,
}

// https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/trading-requests#place-new-order-trade
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsOrderResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    id: SmolStr,
    status: i64,
    result: BinanceWsOrderResult,
    rate_limits: BinanceWsOrderResRateLimit,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsOrderResult {
    symbol: BinanceSymbol,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    order_id: SmolStr,
    order_list_id: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    client_order_id: SmolStr,
    transact_time: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsOrderResRateLimit {
    rate_limit: Vec<BinanceRateLimit>,
}

#[cfg(test)]
mod tests {
    use super::super::super::common::BinanceRateLimit;
    use super::*;
    use rust_decimal::Decimal;
    use serde_json;
    use smol_str::SmolStr;

    #[test]
    fn test_binance_ws_order_request_ser_deser() {
        let request = BinanceWsOrderRequest {
            id: SmolStr::new("1234"),
            method: SmolStr::new("order.place"),
            params: BinanceWsOrderReqParams {
                symbol: BinanceSymbol::new("BTC", "USDT"),
                side: BinanceSide::Buy,
                order_type: BinanceOrderType::Limit,
                time_in_force: BinanceTimeInForce::Gtc,
                price: Decimal::new(200005, 4),
                quantity: Decimal::new(1, 2),
                session: SessionLogonParams {
                    api_key: SmolStr::new("api-key"),
                    timestamp: 1234567890,
                    signature: SmolStr::new("signature"),
                },
            },
        };

        let json = serde_json::to_string(&request).unwrap();
        let back: BinanceWsOrderRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, back);
    }

    #[test]
    fn test_binance_ws_order_response_ser_deser() {
        let response = BinanceWsOrderResponse {
            id: SmolStr::new("5678"),
            status: 200,
            result: BinanceWsOrderResult {
                symbol: BinanceSymbol::new("ETH", "USDT"),
                order_id: SmolStr::new("78910"),
                order_list_id: 1,
                client_order_id: SmolStr::new("myorder123"),
                transact_time: 1625256000000,
            },
            rate_limits: BinanceWsOrderResRateLimit {
                rate_limit: vec![BinanceRateLimit {
                    rate_limit_type: SmolStr::new("ORDERS"),
                    interval: SmolStr::new("MINUTE"),
                    interval_num: 1,
                    limit: 1200,
                    count: 10,
                }],
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let back: BinanceWsOrderResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, back);
    }

    #[test]
    fn test_binance_rate_limit_ser_deser() {
        let rate_limit = BinanceRateLimit {
            rate_limit_type: SmolStr::new("ORDERS"),
            interval: SmolStr::new("MINUTE"),
            interval_num: 1,
            limit: 1200,
            count: 10,
        };

        let json = serde_json::to_string(&rate_limit).unwrap();
        let back: BinanceRateLimit = serde_json::from_str(&json).unwrap();
        assert_eq!(rate_limit, back);
    }

    #[test]
    fn test_binance_ws_order_result_ser_deser() {
        let result = BinanceWsOrderResult {
            symbol: BinanceSymbol::new("ADA", "USDT"),
            order_id: SmolStr::new("111222333"),
            order_list_id: 0,
            client_order_id: SmolStr::new("order789"),
            transact_time: 1625257000000,
        };

        let json = serde_json::to_string(&result).unwrap();
        let back: BinanceWsOrderResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }
}
