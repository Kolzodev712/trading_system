//! Bybit WebSocket RPC create order types
//! See: <https://bybit-exchange.github.io/docs/v5/websocket/trade/guideline>

use super::{BybitRpcRequestHeader, BybitRpcResponse};
use crate::exchanges::bybit::common_types::{BybitCategory, BybitOrderType, BybitSide};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{
    deserialize_option_smol_str, deserialize_smol_str, serialize_option_smol_str,
    serialize_smol_str,
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcCreateOrderRequest {
    #[serde(rename = "reqId", default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<SmolStr>,
    pub header: BybitRpcRequestHeader,
    pub op: SmolStr,
    pub args: Vec<BybitRpcCreateOrderArgs>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcCreateOrderArgs {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub side: BybitSide,
    #[serde(rename = "orderType")]
    pub order_type: BybitOrderType,
    #[serde(with = "rust_decimal::serde::str")]
    pub qty: Decimal,
    #[serde(
        with = "rust_decimal::serde::str_option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub price: Option<Decimal>,
    pub category: BybitCategory,
    #[serde(
        rename = "timeInForce",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub time_in_force: Option<SmolStr>,
    #[serde(
        rename = "orderLinkId",
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    pub order_link_id: Option<SmolStr>,
    #[serde(
        rename = "reduceOnly",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub reduce_only: Option<bool>,
    #[serde(
        rename = "positionIdx",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub position_idx: Option<i32>,
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
pub struct BybitRpcCreateOrderData {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub order_id: SmolStr,
    #[serde(
        rename = "orderLinkId",
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    pub order_link_id: Option<SmolStr>,
}

pub type BybitRpcCreateOrderResponse = BybitRpcResponse<BybitRpcCreateOrderData>;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_bybit_rpc_create_order_request_serialization() {
        let request = BybitRpcCreateOrderRequest {
            req_id: Some("test-005".into()),
            header: BybitRpcRequestHeader {
                x_bapi_timestamp: "1711001595207".into(),
                x_bapi_recv_window: Some("8000".into()),
                referer: None,
            },
            op: "order.create".into(),
            args: vec![BybitRpcCreateOrderArgs {
                symbol: "ETHUSDT".into(),
                side: BybitSide::Buy,
                order_type: BybitOrderType::Limit,
                qty: Decimal::new(20, 1),
                price: Some(Decimal::new(280000, 2)),
                category: BybitCategory::Linear,
                time_in_force: Some("PostOnly".into()),
                order_link_id: None,
                reduce_only: None,
                position_idx: None,
                take_profit: None,
                stop_loss: None,
                tp_trigger_by: None,
                sl_trigger_by: None,
            }],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("order.create"));
        assert!(json.contains("ETHUSDT"));
        assert!(json.contains("Buy"));
    }

    #[test]
    fn test_bybit_rpc_create_order_request_deserialization() {
        let json = r#"
        {
            "reqId": "test-005",
            "header": {
                "X-BAPI-TIMESTAMP": "1711001595207",
                "X-BAPI-RECV-WINDOW": "8000"
            },
            "op": "order.create",
            "args": [
                {
                    "symbol": "ETHUSDT",
                    "side": "Buy",
                    "orderType": "Limit",
                    "qty": "0.2",
                    "price": "2800",
                    "category": "linear",
                    "timeInForce": "PostOnly"
                }
            ]
        }
        "#;

        let request: BybitRpcCreateOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.req_id, Some("test-005".into()));
        assert_eq!(request.op, "order.create");
        assert_eq!(request.args.len(), 1);
        assert_eq!(request.args[0].symbol, "ETHUSDT");
        assert_eq!(request.args[0].side, BybitSide::Buy);
        assert_eq!(request.args[0].order_type, BybitOrderType::Limit);
    }

    #[test]
    fn test_bybit_rpc_create_order_response_deserialization() {
        let json = r#"
        {
            "reqId": "test-005",
            "retCode": 0,
            "retMsg": "OK",
            "op": "order.create",
            "data": {
                "orderId": "a4c1718e-fe53-4659-a118-1f6ecce04ad9",
                "orderLinkId": ""
            },
            "retExtInfo": {},
            "header": {
                "TraceId": "0e32b551b3e17aae77651aadf6a5be80",
                "Timenow": "1740453408556",
                "X-Bapi-Limit": "10",
                "X-Bapi-Limit-Status": "9",
                "X-Bapi-Limit-Reset-Timestamp": "1740453408555"
            },
            "connId": "cnt5leec0hvan15eukcg-2t"
        }
        "#;

        let response: BybitRpcCreateOrderResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert_eq!(response.ret_msg, "OK");
        assert!(response.data.is_some());
        let data = response.data.unwrap();
        assert_eq!(data.order_id, "a4c1718e-fe53-4659-a118-1f6ecce04ad9");
    }
}
