//! Bybit WebSocket RPC cancel all orders types
//! See: <https://bybit-exchange.github.io/docs/v5/websocket/trade/guideline>

use super::{BybitRpcRequestHeader, BybitRpcResponse};
use crate::exchanges::bybit::common_types::BybitCategory;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcCancelAllOrdersRequest {
    #[serde(rename = "reqId", default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<SmolStr>,
    pub header: BybitRpcRequestHeader,
    pub op: SmolStr,
    pub args: Vec<BybitRpcCancelAllOrdersArgs>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcCancelAllOrdersArgs {
    pub category: BybitCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SmolStr>,
    #[serde(rename = "baseCoin", default, skip_serializing_if = "Option::is_none")]
    pub base_coin: Option<SmolStr>,
    #[serde(
        rename = "settleCoin",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub settle_coin: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcCancelAllOrdersData {
    pub list: Vec<BybitRpcCancelAllOrderItem>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRpcCancelAllOrderItem {
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

pub type BybitRpcCancelAllOrdersResponse = BybitRpcResponse<BybitRpcCancelAllOrdersData>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bybit_rpc_cancel_all_orders_request_serialization() {
        let request = BybitRpcCancelAllOrdersRequest {
            req_id: Some("cancel-all-001".into()),
            header: BybitRpcRequestHeader {
                x_bapi_timestamp: "1711001595207".into(),
                x_bapi_recv_window: None,
                referer: None,
            },
            op: "order.cancel-all".into(),
            args: vec![BybitRpcCancelAllOrdersArgs {
                category: BybitCategory::Linear,
                symbol: Some("ETHUSDT".into()),
                base_coin: None,
                settle_coin: None,
            }],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("order.cancel-all"));
        assert!(json.contains("linear"));
    }

    #[test]
    fn test_bybit_rpc_cancel_all_orders_request_deserialization() {
        let json = r#"
        {
            "reqId": "cancel-all-001",
            "header": {
                "X-BAPI-TIMESTAMP": "1711001595207"
            },
            "op": "order.cancel-all",
            "args": [
                {
                    "category": "linear",
                    "symbol": "ETHUSDT"
                }
            ]
        }
        "#;

        let request: BybitRpcCancelAllOrdersRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.op, "order.cancel-all");
        assert_eq!(request.args[0].category, BybitCategory::Linear);
        assert_eq!(request.args[0].symbol, Some("ETHUSDT".into()));
    }

    #[test]
    fn test_bybit_rpc_cancel_all_orders_response_deserialization() {
        let json = r#"
        {
            "reqId": "cancel-all-001",
            "retCode": 0,
            "retMsg": "OK",
            "op": "order.cancel-all",
            "data": {
                "list": [
                    {
                        "orderId": "order-1",
                        "orderLinkId": "link-1"
                    },
                    {
                        "orderId": "order-2",
                        "orderLinkId": ""
                    }
                ]
            },
            "retExtInfo": {},
            "connId": "cnt5leec0hvan15eukcg-2t"
        }
        "#;

        let response: BybitRpcCancelAllOrdersResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert!(response.data.is_some());
        let data = response.data.unwrap();
        assert_eq!(data.list.len(), 2);
        assert_eq!(data.list[0].order_id, "order-1");
    }
}
