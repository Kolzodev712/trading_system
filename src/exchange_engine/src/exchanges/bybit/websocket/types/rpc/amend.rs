//! Bybit WebSocket RPC amend order types
//! See: <https://bybit-exchange.github.io/docs/v5/websocket/trade/guideline>

use super::{BybitRpcRequestHeader, BybitRpcResponse};
use crate::exchanges::bybit::common_types::BybitCategory;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcAmendOrderRequest {
    #[serde(rename = "reqId", default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<SmolStr>,
    pub header: BybitRpcRequestHeader,
    pub op: SmolStr,
    pub args: Vec<BybitRpcAmendOrderArgs>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcAmendOrderArgs {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub category: BybitCategory,
    #[serde(rename = "orderId", default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<SmolStr>,
    #[serde(
        rename = "orderLinkId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub order_link_id: Option<SmolStr>,
    #[serde(
        with = "rust_decimal::serde::str_option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub price: Option<Decimal>,
    #[serde(
        with = "rust_decimal::serde::str_option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub qty: Option<Decimal>,
    #[serde(
        rename = "takeProfit",
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub take_profit: Option<Decimal>,
    #[serde(
        rename = "stopLoss",
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub stop_loss: Option<Decimal>,
    #[serde(
        rename = "tpTriggerBy",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tp_trigger_by: Option<SmolStr>,
    #[serde(
        rename = "slTriggerBy",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub sl_trigger_by: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcAmendOrderData {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub order_id: SmolStr,
    #[serde(
        rename = "orderLinkId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub order_link_id: Option<SmolStr>,
}

pub type BybitRpcAmendOrderResponse = BybitRpcResponse<BybitRpcAmendOrderData>;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_bybit_rpc_amend_order_request_serialization() {
        let request = BybitRpcAmendOrderRequest {
            req_id: Some("amend-001".into()),
            header: BybitRpcRequestHeader {
                x_bapi_timestamp: "1711001595207".into(),
                x_bapi_recv_window: None,
                referer: None,
            },
            op: "order.amend".into(),
            args: vec![BybitRpcAmendOrderArgs {
                symbol: "ETHUSDT".into(),
                category: BybitCategory::Linear,
                order_id: Some("a4c1718e-fe53-4659-a118-1f6ecce04ad9".into()),
                order_link_id: None,
                price: Some(Decimal::new(290000, 2)),
                qty: Some(Decimal::new(30, 1)),
                take_profit: None,
                stop_loss: None,
                tp_trigger_by: None,
                sl_trigger_by: None,
            }],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("order.amend"));
        assert!(json.contains("ETHUSDT"));
    }

    #[test]
    fn test_bybit_rpc_amend_order_request_deserialization() {
        let json = r#"
        {
            "reqId": "amend-001",
            "header": {
                "X-BAPI-TIMESTAMP": "1711001595207"
            },
            "op": "order.amend",
            "args": [
                {
                    "symbol": "ETHUSDT",
                    "category": "linear",
                    "orderId": "a4c1718e-fe53-4659-a118-1f6ecce04ad9",
                    "price": "2900",
                    "qty": "0.3"
                }
            ]
        }
        "#;

        let request: BybitRpcAmendOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.op, "order.amend");
        assert_eq!(request.args[0].symbol, "ETHUSDT");
        assert_eq!(
            request.args[0].order_id,
            Some("a4c1718e-fe53-4659-a118-1f6ecce04ad9".into())
        );
    }

    #[test]
    fn test_bybit_rpc_amend_order_response_deserialization() {
        let json = r#"
        {
            "reqId": "amend-001",
            "retCode": 0,
            "retMsg": "OK",
            "op": "order.amend",
            "data": {
                "orderId": "a4c1718e-fe53-4659-a118-1f6ecce04ad9",
                "orderLinkId": ""
            },
            "retExtInfo": {},
            "connId": "cnt5leec0hvan15eukcg-2t"
        }
        "#;

        let response: BybitRpcAmendOrderResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert!(response.data.is_some());
        let data = response.data.unwrap();
        assert_eq!(data.order_id, "a4c1718e-fe53-4659-a118-1f6ecce04ad9");
    }
}
