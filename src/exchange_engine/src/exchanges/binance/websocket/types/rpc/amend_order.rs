use crate::exchanges::binance::types::{BinanceOrderStatus, BinanceSide};
use trading_core::{
    error::EngineError,
    types::{common::OrderId, exchange::Order},
};

use super::super::common::{BinanceRateLimit, SessionLogonParams};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

// https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/trading-requests#modify-order-trade
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsAmendOrderRequest {
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
    pub params: BinanceWsAmendOrderReqParams,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsAmendOrderReqParams {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    // Use orderId (numeric) if available, otherwise origClientOrderId (string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_client_order_id: Option<SmolStr>,
    // New price (optional - only send if changing)
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub price: Option<Decimal>,
    // New quantity (optional - only send if changing)
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub quantity: Option<Decimal>,
    #[serde(flatten, default)]
    pub session: SessionLogonParams,
}

impl BinanceWsAmendOrderReqParams {
    /// Create amend params with numeric orderId (preferred)
    pub fn new_with_order_id(
        symbol: SmolStr,
        order_id: i64,
        price: Option<Decimal>,
        quantity: Option<Decimal>,
    ) -> Self {
        Self {
            symbol,
            order_id: Some(order_id),
            orig_client_order_id: None,
            price,
            quantity,
            session: SessionLogonParams::default(),
        }
    }

    /// Create amend params with client order ID (fallback)
    pub fn new_with_client_order_id(
        symbol: SmolStr,
        orig_client_order_id: SmolStr,
        price: Option<Decimal>,
        quantity: Option<Decimal>,
    ) -> Self {
        Self {
            symbol,
            order_id: None,
            orig_client_order_id: Some(orig_client_order_id),
            price,
            quantity,
            session: SessionLogonParams::default(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsAmendOrderResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    pub status: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<BinanceWsAmendOrderResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<BinanceWsAmendOrderError>,
    #[serde(
        default,
        rename = "rateLimits",
        skip_serializing_if = "Option::is_none"
    )]
    pub rate_limits: Option<Vec<BinanceRateLimit>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsAmendOrderResult {
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
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub orig_qty: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub executed_qty: Decimal,
    pub status: BinanceOrderStatus,
    pub side: BinanceSide,
    pub update_time: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsAmendOrderError {
    pub code: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub msg: SmolStr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<BinanceWsAmendOrderErrorData>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceWsAmendOrderErrorData {
    pub server_time: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

/// Map Binance amend order response to core Order type
pub fn map_amend_order_to_core(
    params: BinanceWsAmendOrderReqParams,
    resp: BinanceWsAmendOrderResult,
) -> Result<Order, EngineError> {
    use trading_core::types::common::{OrderStatus, Side};

    let side: Side = resp.side.into();
    let status: OrderStatus = resp.status.into();

    Ok(Order {
        order_id: OrderId::from(resp.order_id.to_string()),
        client_order_id: Some(resp.client_order_id.clone()),
        symbol: params.symbol.into(),
        side,
        status,
        price: resp.price,
        amount: resp.orig_qty,
        filled_amount: resp.executed_qty,
        internal_created_at: chrono::Utc::now().timestamp_millis() as u64,
        exchange_created_at: Some(resp.update_time as u64),
        instrument_type: trading_core::types::common::InstrumentType::Spot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_amend_order_request_serialization() {
        let request = BinanceWsAmendOrderRequest {
            id: "test-id".into(),
            method: "order.modify".into(),
            params: BinanceWsAmendOrderReqParams::new_with_order_id(
                "BTCUSDT".into(),
                12345,
                Some(Decimal::new(50000, 0)),
                Some(Decimal::new(1, 3)),
            ),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("order.modify"));
        assert!(json.contains("BTCUSDT"));
        assert!(json.contains("12345"));
    }
}
