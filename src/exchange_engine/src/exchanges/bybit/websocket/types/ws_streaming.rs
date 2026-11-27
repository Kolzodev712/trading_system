//! WebSocket streaming request and response types

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::{
    types::exchange::{Balance, Order, Position, Trade},
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BybitTopic {
    Position,
    Execution,
    Order,
    Wallet,
}

impl BybitTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            BybitTopic::Position => "position",
            BybitTopic::Execution => "execution",
            BybitTopic::Order => "order",
            BybitTopic::Wallet => "wallet",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BybitWebSocketOp {
    #[serde(rename = "auth")]
    Auth,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "subscribe")]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitAuthRequest {
    #[serde(rename = "req_id")]
    pub req_id: Option<SmolStr>,
    pub op: BybitWebSocketOp,
    pub args: [String; 3],
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitPingRequest {
    #[serde(rename = "req_id")]
    pub req_id: Option<SmolStr>,
    pub op: BybitWebSocketOp,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitSubscribeRequest {
    #[serde(rename = "req_id")]
    pub req_id: Option<SmolStr>,
    pub op: BybitWebSocketOp,
    pub args: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitAuthResponse {
    pub success: bool,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub ret_msg: SmolStr,
    pub op: BybitWebSocketOp,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub conn_id: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitPongResponse {
    pub success: bool,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub ret_msg: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub conn_id: SmolStr,
    #[serde(rename = "req_id")]
    pub req_id: Option<SmolStr>,
    pub op: BybitWebSocketOp,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitSubscribeResponse {
    pub success: bool,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub ret_msg: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub conn_id: SmolStr,
    #[serde(rename = "req_id")]
    pub req_id: Option<SmolStr>,
    pub op: BybitWebSocketOp,
}

#[derive(Debug, Clone)]
pub enum BybitStreamingMessage {
    // Private topics are batched by Bybit: one frame may carry several items.
    Position(Vec<Position>),
    Execution(Vec<Trade>),
    Order(Vec<Order>),
    Wallet(Vec<Balance>),
    AuthResponse(BybitAuthResponse),
    PongResponse(BybitPongResponse),
    SubscribeResponse(BybitSubscribeResponse),
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_bybit_websocket_op_deserialization() {
        assert_eq!(
            serde_json::from_str::<BybitWebSocketOp>(r#""auth""#).unwrap(),
            BybitWebSocketOp::Auth
        );
        assert_eq!(
            serde_json::from_str::<BybitWebSocketOp>(r#""ping""#).unwrap(),
            BybitWebSocketOp::Ping
        );
        assert_eq!(
            serde_json::from_str::<BybitWebSocketOp>(r#""pong""#).unwrap(),
            BybitWebSocketOp::Pong
        );
        assert_eq!(
            serde_json::from_str::<BybitWebSocketOp>(r#""subscribe""#).unwrap(),
            BybitWebSocketOp::Subscribe
        );
        assert_eq!(
            serde_json::from_str::<BybitWebSocketOp>(r#""unsubscribe""#).unwrap(),
            BybitWebSocketOp::Unsubscribe
        );
    }

    #[test]
    fn test_bybit_auth_request_serialization() {
        let req = BybitAuthRequest {
            req_id: Some("test-123".into()),
            op: BybitWebSocketOp::Auth,
            args: [
                "api_key".to_string(),
                "1234567890".to_string(),
                "signature".to_string(),
            ],
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("auth"));
        assert!(json.contains("test-123"));
        assert!(json.contains("api_key"));
    }

    #[test]
    fn test_bybit_auth_request_deserialization() {
        let json = r#"
        {
            "req_id": "test-123",
            "op": "auth",
            "args": ["api_key", "1234567890", "signature"]
        }
        "#;

        let req: BybitAuthRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.req_id, Some("test-123".into()));
        assert_eq!(req.op, BybitWebSocketOp::Auth);
        assert_eq!(req.args[0], "api_key");
        assert_eq!(req.args[1], "1234567890");
        assert_eq!(req.args[2], "signature");
    }

    #[test]
    fn test_bybit_ping_request_serialization() {
        let req = BybitPingRequest {
            req_id: Some("ping-123".into()),
            op: BybitWebSocketOp::Ping,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("ping"));
        assert!(json.contains("ping-123"));
    }

    #[test]
    fn test_bybit_ping_request_deserialization() {
        let json = r#"
        {
            "req_id": "ping-123",
            "op": "ping"
        }
        "#;

        let req: BybitPingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.req_id, Some("ping-123".into()));
        assert_eq!(req.op, BybitWebSocketOp::Ping);
    }

    #[test]
    fn test_bybit_subscribe_request_serialization() {
        let req = BybitSubscribeRequest {
            req_id: Some("sub-123".into()),
            op: BybitWebSocketOp::Subscribe,
            args: vec!["position".into(), "execution".into()],
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("subscribe"));
        assert!(json.contains("position"));
        assert!(json.contains("execution"));
    }

    #[test]
    fn test_bybit_subscribe_request_deserialization() {
        let json = r#"
        {
            "req_id": "sub-123",
            "op": "subscribe",
            "args": ["position", "execution"]
        }
        "#;

        let req: BybitSubscribeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.req_id, Some("sub-123".into()));
        assert_eq!(req.op, BybitWebSocketOp::Subscribe);
        assert_eq!(req.args.len(), 2);
        assert_eq!(req.args[0], "position");
        assert_eq!(req.args[1], "execution");
    }

    #[test]
    fn test_bybit_auth_response_deserialization() {
        let json = r#"
        {
            "success": true,
            "ret_msg": "",
            "op": "auth",
            "conn_id": "conn-123"
        }
        "#;

        let resp: BybitAuthResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert_eq!(resp.ret_msg, "");
        assert_eq!(resp.op, BybitWebSocketOp::Auth);
        assert_eq!(resp.conn_id, "conn-123");
    }

    #[test]
    fn test_bybit_pong_response_deserialization() {
        let json = r#"
        {
            "success": true,
            "ret_msg": "pong",
            "conn_id": "conn-123",
            "req_id": "ping-123",
            "op": "pong"
        }
        "#;

        let resp: BybitPongResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert_eq!(resp.ret_msg, "pong");
        assert_eq!(resp.conn_id, "conn-123");
        assert_eq!(resp.req_id, Some("ping-123".into()));
        assert_eq!(resp.op, BybitWebSocketOp::Pong);
    }

    #[test]
    fn test_bybit_subscribe_response_deserialization() {
        let json = r#"
        {
            "success": true,
            "ret_msg": "subscribe",
            "conn_id": "conn-123",
            "req_id": "sub-123",
            "op": "subscribe"
        }
        "#;

        let resp: BybitSubscribeResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert_eq!(resp.ret_msg, "subscribe");
        assert_eq!(resp.conn_id, "conn-123");
        assert_eq!(resp.req_id, Some("sub-123".into()));
        assert_eq!(resp.op, BybitWebSocketOp::Subscribe);
    }

    #[test]
    fn test_bybit_auth_response_failure() {
        let json = r#"
        {
            "success": false,
            "ret_msg": "Invalid signature",
            "op": "auth",
            "conn_id": ""
        }
        "#;

        let resp: BybitAuthResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.success);
        assert_eq!(resp.ret_msg, "Invalid signature");
    }

    #[test]
    fn test_bybit_topic_enum() {
        assert_eq!(
            serde_json::from_str::<BybitTopic>(r#""position""#).unwrap(),
            BybitTopic::Position
        );
        assert_eq!(
            serde_json::from_str::<BybitTopic>(r#""execution""#).unwrap(),
            BybitTopic::Execution
        );
        assert_eq!(
            serde_json::from_str::<BybitTopic>(r#""order""#).unwrap(),
            BybitTopic::Order
        );
        assert_eq!(
            serde_json::from_str::<BybitTopic>(r#""wallet""#).unwrap(),
            BybitTopic::Wallet
        );
    }

    #[test]
    fn test_bybit_topic_as_str() {
        assert_eq!(BybitTopic::Position.as_str(), "position");
        assert_eq!(BybitTopic::Execution.as_str(), "execution");
        assert_eq!(BybitTopic::Order.as_str(), "order");
        assert_eq!(BybitTopic::Wallet.as_str(), "wallet");
    }
}
