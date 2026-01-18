//! Bybit WebSocket RPC cancel order by ID types
//! See: <https://bybit-exchange.github.io/docs/v5/websocket/trade/guideline>

use super::{BybitRpcRequestHeader, BybitRpcResponse};
use crate::exchanges::bybit::common_types::BybitCategory;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcCancelOrderRequest {
    #[serde(rename = "reqId", default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<SmolStr>,
    pub header: BybitRpcRequestHeader,
    pub op: SmolStr,
    pub args: Vec<BybitRpcCancelOrderArgs>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcCancelOrderArgs {
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
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcCancelOrderData {
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

pub type BybitRpcCancelOrderResponse = BybitRpcResponse<BybitRpcCancelOrderData>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bybit_rpc_cancel_order_request_serialization() {
        let request = BybitRpcCancelOrderRequest {
            req_id: Some("cancel-001".into()),
            header: BybitRpcRequestHeader {
                x_bapi_timestamp: "1711001595207".into(),
                x_bapi_recv_window: Some("8000".into()),
                referer: None,
            },
            op: "order.cancel".into(),
            args: vec![BybitRpcCancelOrderArgs {
                symbol: "ETHUSDT".into(),
                category: BybitCategory::Linear,
                order_id: Some("a4c1718e-fe53-4659-a118-1f6ecce04ad9".into()),
                order_link_id: None,
            }],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("order.cancel"));
        assert!(json.contains("ETHUSDT"));
    }

    #[test]
    fn test_bybit_rpc_cancel_order_request_deserialization() {
        let json = r#"
        {
            "reqId": "cancel-001",
            "header": {
                "X-BAPI-TIMESTAMP": "1711001595207"
            },
            "op": "order.cancel",
            "args": [
                {
                    "symbol": "ETHUSDT",
                    "category": "linear",
                    "orderId": "a4c1718e-fe53-4659-a118-1f6ecce04ad9"
                }
            ]
        }
        "#;

        let request: BybitRpcCancelOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.op, "order.cancel");
        assert_eq!(request.args[0].symbol, "ETHUSDT");
        assert_eq!(
            request.args[0].order_id,
            Some("a4c1718e-fe53-4659-a118-1f6ecce04ad9".into())
        );
    }

    #[test]
    fn test_bybit_rpc_cancel_order_response_deserialization() {
        let json = r#"
        {
            "reqId": "cancel-001",
            "retCode": 0,
            "retMsg": "OK",
            "op": "order.cancel",
            "data": {
                "orderId": "a4c1718e-fe53-4659-a118-1f6ecce04ad9",
                "orderLinkId": ""
            },
            "retExtInfo": {},
            "connId": "cnt5leec0hvan15eukcg-2t"
        }
        "#;

        let response: BybitRpcCancelOrderResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert!(response.data.is_some());
        let data = response.data.unwrap();
        assert_eq!(data.order_id, "a4c1718e-fe53-4659-a118-1f6ecce04ad9");
    }
}
