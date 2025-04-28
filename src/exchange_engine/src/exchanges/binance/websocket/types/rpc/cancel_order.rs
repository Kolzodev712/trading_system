use crate::exchanges::binance::common_types::{
    BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceSymbol, BinanceTimeInForce,
};

use super::super::common::{BinanceRateLimit, SessionLogonParams};
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

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
    symbol: BinanceSymbol,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    orig_client_order_id: SmolStr,
    order_type: BinanceOrderType,
    #[serde(flatten)]
    session: SessionLogonParams,
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
    pub result: BinanceWsCancelOrderResult,
    pub rate_limits: BinanceRateLimit,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsCancelOrderResult {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    symbol: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    // order id that was cancelled
    orig_client_order_id: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    order_id: SmolStr,
    price: Decimal,
    orig_qty: Decimal,
    status: BinanceOrderStatus,
    time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    type_: BinanceOrderType,
    side: BinanceSide,
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
                symbol: BinanceSymbol::new("BTC", "USDT"),
                orig_client_order_id: SmolStr::new("myorder-789"),
                order_type: BinanceOrderType::Limit,
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
            result: BinanceWsCancelOrderResult {
                symbol: SmolStr::new("BTCUSDT"),
                orig_client_order_id: SmolStr::new("myorder-789"),
                order_id: SmolStr::new("order-123"),
                price: Decimal::new(200005, 4),
                orig_qty: Decimal::new(100, 2),
                status: BinanceOrderStatus::Canceled,
                time_in_force: BinanceTimeInForce::Gtc,
                type_: BinanceOrderType::Limit,
                side: BinanceSide::Buy,
            },
            rate_limits: BinanceRateLimit {
                rate_limit_type: SmolStr::new("ORDERS"),
                interval: SmolStr::new("MINUTE"),
                interval_num: 1,
                limit: 1200,
                count: 50,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let back: BinanceWsCancelOrderResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, back);
    }

    #[test]
    fn test_binance_ws_cancel_order_result_ser_deser() {
        let result = BinanceWsCancelOrderResult {
            symbol: SmolStr::new("ETHUSDT"),
            orig_client_order_id: SmolStr::new("original-order-id"),
            order_id: SmolStr::new("order-id-123"),
            price: Decimal::new(180000, 4),
            orig_qty: Decimal::new(50, 2),
            status: BinanceOrderStatus::New,
            time_in_force: BinanceTimeInForce::Gtc,
            type_: BinanceOrderType::Limit,
            side: BinanceSide::Sell,
        };

        let json = serde_json::to_string(&result).unwrap();
        let back: BinanceWsCancelOrderResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }
}
