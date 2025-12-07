use crate::exchanges::binance::common_types::{
    BinanceOrderType, BinanceSide, BinanceSymbol, BinanceTimeInForce,
};
use trading_core::{
    error::EngineError,
    types::{
        common::OrderId,
        exchange::{Order, OrderRequest},
    },
};

use super::super::common::{BinanceRateLimit, SessionLogonParams};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_enum_as_display, serialize_smol_str};

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
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub side: BinanceSide,
    #[serde(rename = "type", serialize_with = "serialize_enum_as_display")]
    pub order_type: BinanceOrderType,
    pub time_in_force: BinanceTimeInForce,
    pub price: Decimal,
    pub quantity: Decimal,
    #[serde(flatten, default)]
    pub session: SessionLogonParams,
}

impl BinanceWsOrderReqParams {
    pub fn new(
        symbol: SmolStr,
        side: BinanceSide,
        order_type: BinanceOrderType,
        time_in_force: BinanceTimeInForce,
        price: Decimal,
        quantity: Decimal,
    ) -> Self {
        Self {
            symbol,
            side,
            order_type,
            time_in_force,
            price,
            quantity,
            session: SessionLogonParams::default(),
        }
    }
}

impl TryFrom<OrderRequest> for BinanceWsOrderReqParams {
    type Error = EngineError;

    fn try_from(req: OrderRequest) -> Result<Self, Self::Error> {
        let side: BinanceSide = req.side.into();
        let order_type: BinanceOrderType = req.order_type.into();

        let tif = req
            .exchange_params
            .get("timeInForce")
            .or_else(|| req.exchange_params.get("tif"))
            .and_then(|raw| parse_tif(raw))
            .unwrap_or(match order_type {
                BinanceOrderType::Limit
                | BinanceOrderType::LimitMaker
                | BinanceOrderType::StopLossLimit
                | BinanceOrderType::TakeProfitLimit => BinanceTimeInForce::Gtc,
                _ => BinanceTimeInForce::Gtc,
            });

        Ok(Self {
            symbol: SmolStr::from(&req.symbol),
            side,
            order_type,
            time_in_force: tif,
            price: req.price,
            quantity: req.amount,
            session: SessionLogonParams::default(),
        })
    }
}

fn parse_tif(s: &str) -> Option<BinanceTimeInForce> {
    match s.to_ascii_uppercase().as_str() {
        "GTC" => Some(BinanceTimeInForce::Gtc),
        "IOC" => Some(BinanceTimeInForce::Ioc),
        "FOK" => Some(BinanceTimeInForce::Fok),
        _ => None,
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsOrderError {
    pub code: i64,
    #[serde(default)]
    pub msg: String,
}

// https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/trading-requests#place-new-order-trade
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsOrderResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    pub status: i64,
    #[serde(default)]
    pub result: Option<BinanceWsOrderResult>,
    #[serde(default)]
    pub error: Option<BinanceWsOrderError>,
    #[serde(default, rename = "rateLimits")]
    pub rate_limits: Option<Vec<BinanceRateLimit>>,
}

pub fn map_open_order_to_core(
    params: BinanceWsOrderReqParams,
    resp: BinanceWsOrderResponse,
) -> Result<Order, EngineError> {
    if resp.status != 200 {
        let error_msg = resp
            .error
            .as_ref()
            .map(|e| format!("code={code}, msg={msg}", code = e.code, msg = e.msg))
            .unwrap_or_else(|| format!("status={status}", status = resp.status));
        return Err(EngineError::new_custom_error(
            format!("WS RPC error: {error_msg}", error_msg = error_msg).into(),
        ));
    }

    let result = resp.result.ok_or_else(|| {
        EngineError::new_custom_error("WS RPC success response missing result field".into())
    })?;

    Ok(Order {
        order_id: result
            .order_id
            .parse::<u64>()
            .map(OrderId::from)
            .map_err(|e| EngineError::new_custom_error(format!("invalid order_id: {e}").into()))?,
        symbol: result.symbol.convert_to_smolstr().into(),
        price: params.price,
        amount: params.quantity,
        side: params.side.into(),
        status: trading_core::types::common::OrderStatus::New,
        filled_amount: rust_decimal::Decimal::ZERO,
        internal_created_at: chrono::Utc::now().timestamp_millis() as u64,
        exchange_created_at: Some(result.transact_time as u64),
    })
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsOrderResult {
    pub symbol: BinanceSymbol,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub order_id: SmolStr,
    pub order_list_id: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub client_order_id: SmolStr,
    pub transact_time: i64,
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
                symbol: SmolStr::new("BTCUSDT"),
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
            result: Some(BinanceWsOrderResult {
                symbol: BinanceSymbol::new("ETH", "USDT"),
                order_id: SmolStr::new("78910"),
                order_list_id: 1,
                client_order_id: SmolStr::new("myorder123"),
                transact_time: 1625256000000,
            }),
            error: None,
            rate_limits: Some(vec![BinanceRateLimit {
                rate_limit_type: SmolStr::new("ORDERS"),
                interval: SmolStr::new("MINUTE"),
                interval_num: 1,
                limit: 1200,
                count: 10,
            }]),
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
