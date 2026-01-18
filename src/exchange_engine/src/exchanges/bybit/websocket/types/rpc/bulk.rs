//! Bybit WebSocket RPC batch operations types
//! See: <https://bybit-exchange.github.io/docs/v5/websocket/trade/guideline>

use super::{BybitRpcRequestHeader, BybitRpcResponse};
use crate::exchanges::bybit::common_types::{BybitCategory, BybitOrderType, BybitSide};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchCreateOrderRequest {
    #[serde(rename = "reqId", default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<SmolStr>,
    pub header: BybitRpcRequestHeader,
    pub op: SmolStr,
    pub args: Vec<BybitRpcBatchCreateOrderArgs>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchCreateOrderArgs {
    pub category: BybitCategory,
    pub request: Vec<BybitRpcBatchCreateOrderItem>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchCreateOrderItem {
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
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(rename = "timeInForce")]
    pub time_in_force: SmolStr,
    #[serde(
        rename = "orderLinkId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub order_link_id: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchAmendOrderRequest {
    #[serde(rename = "reqId", default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<SmolStr>,
    pub header: BybitRpcRequestHeader,
    pub op: SmolStr,
    pub args: Vec<BybitRpcBatchAmendOrderArgs>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchAmendOrderArgs {
    pub category: BybitCategory,
    pub request: Vec<BybitRpcBatchAmendOrderItem>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchAmendOrderItem {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
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
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchCancelOrderRequest {
    #[serde(rename = "reqId", default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<SmolStr>,
    pub header: BybitRpcRequestHeader,
    pub op: SmolStr,
    pub args: Vec<BybitRpcBatchCancelOrderArgs>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchCancelOrderArgs {
    pub category: BybitCategory,
    pub request: Vec<BybitRpcBatchCancelOrderItem>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchCancelOrderItem {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(rename = "orderId", default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<SmolStr>,
    #[serde(
        rename = "orderLinkId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub order_link_id: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchResponseData {
    pub list: Vec<BybitRpcBatchResponseItem>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcBatchResponseItem {
    pub category: BybitCategory,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub order_id: SmolStr,
    #[serde(
        rename = "orderLinkId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub order_link_id: Option<SmolStr>,
    #[serde(rename = "createAt", default, skip_serializing_if = "Option::is_none")]
    pub create_at: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitRpcBatchErrorInfo {
    pub list: Vec<BybitRpcBatchErrorItem>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitRpcBatchErrorItem {
    pub code: i32,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub msg: SmolStr,
}

pub type BybitRpcBatchCreateOrderResponse = BybitRpcResponse<BybitRpcBatchResponseData>;
pub type BybitRpcBatchAmendOrderResponse = BybitRpcResponse<BybitRpcBatchResponseData>;
pub type BybitRpcBatchCancelOrderResponse = BybitRpcResponse<BybitRpcBatchResponseData>;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_bybit_rpc_batch_create_order_request_serialization() {
        let request = BybitRpcBatchCreateOrderRequest {
            req_id: None,
            header: BybitRpcRequestHeader {
                x_bapi_timestamp: "1740453381256".into(),
                x_bapi_recv_window: Some("1000".into()),
                referer: None,
            },
            op: "order.create-batch".into(),
            args: vec![BybitRpcBatchCreateOrderArgs {
                category: BybitCategory::Linear,
                request: vec![BybitRpcBatchCreateOrderItem {
                    symbol: "SOLUSDT".into(),
                    side: BybitSide::Buy,
                    order_type: BybitOrderType::Limit,
                    qty: Decimal::new(100, 1),
                    price: Decimal::new(50000, 2),
                    time_in_force: "GTC".into(),
                    order_link_id: Some("-batch-000".into()),
                }],
            }],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("order.create-batch"));
        assert!(json.contains("SOLUSDT"));
    }

    #[test]
    fn test_bybit_rpc_batch_create_order_request_deserialization() {
        let json = r#"
        {
            "op": "order.create-batch",
            "header": {
                "X-BAPI-TIMESTAMP": "1740453381256",
                "X-BAPI-RECV-WINDOW": "1000"
            },
            "args": [
                {
                    "category": "linear",
                    "request": [
                        {
                            "symbol": "SOLUSDT",
                            "qty": "10",
                            "price": "500",
                            "orderType": "Limit",
                            "timeInForce": "GTC",
                            "orderLinkId": "-batch-000",
                            "side": "Buy"
                        }
                    ]
                }
            ]
        }
        "#;

        let request: BybitRpcBatchCreateOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.op, "order.create-batch");
        assert_eq!(request.args[0].category, BybitCategory::Linear);
        assert_eq!(request.args[0].request.len(), 1);
        assert_eq!(request.args[0].request[0].symbol, "SOLUSDT");
    }

    #[test]
    fn test_bybit_rpc_batch_response_deserialization() {
        let json = r#"
        {
            "retCode": 0,
            "retMsg": "OK",
            "op": "order.create-batch",
            "data": {
                "list": [
                    {
                        "category": "linear",
                        "symbol": "SOLUSDT",
                        "orderId": "",
                        "orderLinkId": "batch-000",
                        "createAt": ""
                    }
                ]
            },
            "retExtInfo": {
                "list": [
                    {
                        "code": 10001,
                        "msg": "position idx not match position mode"
                    }
                ]
            },
            "connId": "cupviqn88smf24t2kpb0-536o"
        }
        "#;

        let response: BybitRpcBatchCreateOrderResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert!(response.data.is_some());
        let data = response.data.unwrap();
        assert_eq!(data.list.len(), 1);
        assert_eq!(data.list[0].symbol, "SOLUSDT");
        assert_eq!(data.list[0].order_link_id, Some("batch-000".into()));
    }

    #[test]
    fn test_bybit_rpc_batch_amend_order_request() {
        let json = r#"
        {
            "op": "order.amend-batch",
            "header": {
                "X-BAPI-TIMESTAMP": "1740453381256"
            },
            "args": [
                {
                    "category": "linear",
                    "request": [
                        {
                            "symbol": "SOLUSDT",
                            "orderId": "order-123",
                            "price": "600"
                        }
                    ]
                }
            ]
        }
        "#;

        let request: BybitRpcBatchAmendOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.op, "order.amend-batch");
        assert_eq!(request.args[0].request[0].symbol, "SOLUSDT");
    }

    #[test]
    fn test_bybit_rpc_batch_cancel_order_request() {
        let json = r#"
        {
            "op": "order.cancel-batch",
            "header": {
                "X-BAPI-TIMESTAMP": "1740453381256"
            },
            "args": [
                {
                    "category": "linear",
                    "request": [
                        {
                            "symbol": "SOLUSDT",
                            "orderId": "order-123"
                        }
                    ]
                }
            ]
        }
        "#;

        let request: BybitRpcBatchCancelOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.op, "order.cancel-batch");
        assert_eq!(request.args[0].request[0].symbol, "SOLUSDT");
    }
}
