use crate::exchanges::binance::common_types::{
    BinanceOrderStatus, BinanceOrderType, BinanceSelfTradePreventionMode, BinanceSide,
    BinanceTimeInForce,
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
    // timeInForce is optional for some order types, but we always set it
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<BinanceTimeInForce>,
    // price is optional for MARKET orders, but we always set it for LIMIT
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub price: Option<Decimal>,
    // quantity is optional for MARKET with quoteOrderQty, but we always set it
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub quantity: Option<Decimal>,
    // quoteOrderQty is alternative to quantity for MARKET orders
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub quote_order_qty: Option<Decimal>,
    #[serde(flatten, default)]
    pub session: SessionLogonParams,
}

impl BinanceWsOrderReqParams {
    pub fn new(
        symbol: SmolStr,
        side: BinanceSide,
        order_type: BinanceOrderType,
        time_in_force: Option<BinanceTimeInForce>,
        price: Option<Decimal>,
        quantity: Option<Decimal>,
    ) -> Self {
        Self {
            symbol,
            side,
            order_type,
            time_in_force,
            price,
            quantity,
            quote_order_qty: None,
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
            time_in_force: Some(tif),
            price: Some(req.price),
            quantity: Some(req.amount),
            quote_order_qty: None,
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
    #[serde(
        default,
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub msg: SmolStr,
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
        order_id: OrderId::from(result.order_id as u64),
        client_order_id: Some(result.client_order_id.clone()),
        symbol: result.symbol.into(),
        price: params.price.unwrap_or_default(),
        amount: params.quantity.unwrap_or_default(),
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
    // Fields we actually use
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub order_id: i64,
    pub transact_time: i64,
    // Fields that are always present in RESULT/FULL response but we don't use
    pub order_list_id: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub client_order_id: SmolStr,
    // Optional fields that may appear in RESULT/FULL response (we don't use them)
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub price: Option<Decimal>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub orig_qty: Option<Decimal>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub executed_qty: Option<Decimal>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<BinanceOrderStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<BinanceTimeInForce>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub type_: Option<BinanceOrderType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<BinanceSide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_trade_prevention_mode: Option<BinanceSelfTradePreventionMode>,
    // Conditional fields (only appear if set in request)
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
    // FULL response only - fills array
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Fill>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fill {
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub qty: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub commission: Decimal,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub commission_asset: SmolStr,
    pub trade_id: i64,
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
                time_in_force: Some(BinanceTimeInForce::Gtc),
                price: Some(Decimal::new(200005, 4)),
                quantity: Some(Decimal::new(1, 2)),
                quote_order_qty: None,
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
                symbol: SmolStr::new("ETHUSDT"),
                order_id: 78910,
                transact_time: 1625256000000,
                order_list_id: 1,
                client_order_id: SmolStr::new("myorder123"),
                price: None,
                orig_qty: None,
                executed_qty: None,
                orig_quote_order_qty: None,
                cummulative_quote_qty: None,
                status: None,
                time_in_force: None,
                type_: None,
                side: None,
                working_time: None,
                self_trade_prevention_mode: None,
                stop_price: None,
                trailing_delta: None,
                iceberg_qty: None,
                strategy_id: None,
                strategy_type: None,
                fills: vec![],
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
            symbol: SmolStr::new("ADAUSDT"),
            order_id: 111222333,
            transact_time: 1625257000000,
            order_list_id: 0,
            client_order_id: SmolStr::new("order789"),
            price: None,
            orig_qty: None,
            executed_qty: None,
            orig_quote_order_qty: None,
            cummulative_quote_qty: None,
            status: None,
            time_in_force: None,
            type_: None,
            side: None,
            working_time: None,
            self_trade_prevention_mode: None,
            stop_price: None,
            trailing_delta: None,
            iceberg_qty: None,
            strategy_id: None,
            strategy_type: None,
            fills: vec![],
        };

        let json = serde_json::to_string(&result).unwrap();
        let back: BinanceWsOrderResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }
}
