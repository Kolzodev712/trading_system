//! Bybit WebSocket RPC types

pub mod amend;
pub mod bulk;
pub mod cancel_all;
pub mod cancel_by_id;
pub mod open_order;

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BybitRpcOperation {
    #[serde(rename = "order.create")]
    CreateOrder,
    #[serde(rename = "order.cancel")]
    CancelOrder,
    #[serde(rename = "order.cancel-all")]
    CancelAllOrders,
    #[serde(rename = "order.amend")]
    AmendOrder,
    #[serde(rename = "order.create-batch")]
    BatchCreateOrder,
    #[serde(rename = "order.amend-batch")]
    BatchAmendOrder,
    #[serde(rename = "order.cancel-batch")]
    BatchCancelOrder,
}

impl BybitRpcOperation {
    pub fn as_str(&self) -> &'static str {
        match self {
            BybitRpcOperation::CreateOrder => "order.create",
            BybitRpcOperation::CancelOrder => "order.cancel",
            BybitRpcOperation::CancelAllOrders => "order.cancel-all",
            BybitRpcOperation::AmendOrder => "order.amend",
            BybitRpcOperation::BatchCreateOrder => "order.create-batch",
            BybitRpcOperation::BatchAmendOrder => "order.amend-batch",
            BybitRpcOperation::BatchCancelOrder => "order.cancel-batch",
        }
    }

    pub fn endpoint(&self) -> &'static str {
        self.as_str()
    }
}

impl TryFrom<&str> for BybitRpcOperation {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "order.create" => Ok(BybitRpcOperation::CreateOrder),
            "order.cancel" => Ok(BybitRpcOperation::CancelOrder),
            "order.cancel-all" => Ok(BybitRpcOperation::CancelAllOrders),
            "order.amend" => Ok(BybitRpcOperation::AmendOrder),
            "order.create-batch" => Ok(BybitRpcOperation::BatchCreateOrder),
            "order.amend-batch" => Ok(BybitRpcOperation::BatchAmendOrder),
            "order.cancel-batch" => Ok(BybitRpcOperation::BatchCancelOrder),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitRpcRequestHeader {
    #[serde(rename = "X-BAPI-TIMESTAMP")]
    pub x_bapi_timestamp: SmolStr,
    #[serde(
        rename = "X-BAPI-RECV-WINDOW",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub x_bapi_recv_window: Option<SmolStr>,
    #[serde(rename = "Referer", default, skip_serializing_if = "Option::is_none")]
    pub referer: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitRpcResponseHeader {
    #[serde(rename = "TraceId")]
    pub trace_id: SmolStr,
    #[serde(rename = "Timenow")]
    pub timenow: SmolStr,
    #[serde(rename = "X-Bapi-Limit")]
    pub x_bapi_limit: SmolStr,
    #[serde(rename = "X-Bapi-Limit-Status")]
    pub x_bapi_limit_status: SmolStr,
    #[serde(rename = "X-Bapi-Limit-Reset-Timestamp")]
    pub x_bapi_limit_reset_timestamp: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitRpcResponse<T> {
    #[serde(rename = "reqId", default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<SmolStr>,
    #[serde(rename = "retCode")]
    pub ret_code: i32,
    #[serde(
        rename = "retMsg",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub ret_msg: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub op: SmolStr,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(
        rename = "retExtInfo",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub ret_ext_info: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<BybitRpcResponseHeader>,
    #[serde(
        rename = "connId",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub conn_id: SmolStr,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bybit_rpc_request_header_serialization() {
        let header = BybitRpcRequestHeader {
            x_bapi_timestamp: "1711001595207".into(),
            x_bapi_recv_window: Some("8000".into()),
            referer: Some("bot-001".into()),
        };

        let json = serde_json::to_string(&header).unwrap();
        assert!(json.contains("X-BAPI-TIMESTAMP"));
        assert!(json.contains("1711001595207"));
    }

    #[test]
    fn test_bybit_rpc_request_header_deserialization() {
        let json = r#"
        {
            "X-BAPI-TIMESTAMP": "1711001595207",
            "X-BAPI-RECV-WINDOW": "8000",
            "Referer": "bot-001"
        }
        "#;

        let header: BybitRpcRequestHeader = serde_json::from_str(json).unwrap();
        assert_eq!(header.x_bapi_timestamp, "1711001595207");
        assert_eq!(header.x_bapi_recv_window, Some("8000".into()));
        assert_eq!(header.referer, Some("bot-001".into()));
    }

    #[test]
    fn test_bybit_rpc_response_header_deserialization() {
        let json = r#"
        {
            "TraceId": "0e32b551b3e17aae77651aadf6a5be80",
            "Timenow": "1740453408556",
            "X-Bapi-Limit": "150",
            "X-Bapi-Limit-Status": "147",
            "X-Bapi-Limit-Reset-Timestamp": "1740453408555"
        }
        "#;

        let header: BybitRpcResponseHeader = serde_json::from_str(json).unwrap();
        assert_eq!(header.trace_id, "0e32b551b3e17aae77651aadf6a5be80");
        assert_eq!(header.x_bapi_limit, "150");
        assert_eq!(header.x_bapi_limit_status, "147");
    }

    #[test]
    fn test_bybit_rpc_operation_as_str() {
        assert_eq!(BybitRpcOperation::CreateOrder.as_str(), "order.create");
        assert_eq!(BybitRpcOperation::CancelOrder.as_str(), "order.cancel");
        assert_eq!(
            BybitRpcOperation::CancelAllOrders.as_str(),
            "order.cancel-all"
        );
    }

    #[test]
    fn test_bybit_rpc_operation_try_from() {
        assert_eq!(
            BybitRpcOperation::try_from("order.create").unwrap(),
            BybitRpcOperation::CreateOrder
        );
        assert_eq!(
            BybitRpcOperation::try_from("order.cancel").unwrap(),
            BybitRpcOperation::CancelOrder
        );
        assert!(BybitRpcOperation::try_from("invalid").is_err());
    }

    #[test]
    fn test_bybit_rpc_operation_serialization() {
        let op = BybitRpcOperation::CreateOrder;
        let json = serde_json::to_string(&op).unwrap();
        assert_eq!(json, r#""order.create""#);
    }

    #[test]
    fn test_bybit_rpc_operation_deserialization() {
        let op: BybitRpcOperation = serde_json::from_str(r#""order.create""#).unwrap();
        assert_eq!(op, BybitRpcOperation::CreateOrder);
    }
}
