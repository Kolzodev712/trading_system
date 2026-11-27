use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};
use trading_core::{
    types::market_data::{StructuredOrderbook, StructuredPublicTrade},
    utils::{
        deserialize_option_smol_str, deserialize_smol_str, serialize_option_smol_str,
        serialize_smol_str,
    },
};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BybitSubscriptionRequest {
    #[serde(rename = "op")]
    pub op: SmolStr,
    #[serde(rename = "args")]
    pub args: Vec<SmolStr>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BybitSubscriptionResponse {
    #[serde(rename = "success")]
    pub success: bool,
    #[serde(rename = "ret_msg")]
    #[serde(
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    pub ret_msg: Option<SmolStr>,
    #[serde(rename = "conn_id")]
    #[serde(
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    pub conn_id: Option<SmolStr>,
    #[serde(rename = "op")]
    #[serde(
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    pub op: Option<SmolStr>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BybitMessageType {
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "delta")]
    Delta,
    #[serde(rename = "unknown")]
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum ParsedBybitMessage {
    SubscriptionResponse(BybitSubscriptionResponse),
    OrderBook {
        orderbook: StructuredOrderbook,
    },
    /// One Bybit `publicTrade` frame can batch several trades; all are forwarded.
    Trade {
        trades: Vec<StructuredPublicTrade>,
    },
    Raw(
        #[serde(
            serialize_with = "serialize_smol_str",
            deserialize_with = "deserialize_smol_str"
        )]
        SmolStr,
    ),
}

impl ToSmolStr for ParsedBybitMessage {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            ParsedBybitMessage::SubscriptionResponse(_) => SmolStr::new("SubscriptionResponse"),
            ParsedBybitMessage::OrderBook { .. } => SmolStr::new("OrderBook"),
            ParsedBybitMessage::Trade { .. } => SmolStr::new("Trade"),
            ParsedBybitMessage::Raw(smol_str) => smol_str.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_subscription_request() {
        let json = json!({
            "op": "subscribe",
            "args": ["orderbook.1.BTCUSDT"]
        });

        let req: BybitSubscriptionRequest = serde_json::from_value(json).unwrap();

        assert_eq!(req.op, "subscribe");
        assert_eq!(req.args.len(), 1);
        assert_eq!(req.args[0], "orderbook.1.BTCUSDT");
    }

    #[test]
    fn test_deserialize_subscription_response() {
        let json = json!({
            "success": true,
            "ret_msg": "subscribe",
            "conn_id": "abc123",
            "op": "subscribe"
        });

        let resp: BybitSubscriptionResponse = serde_json::from_value(json).unwrap();

        assert!(resp.success);
        assert_eq!(resp.ret_msg, Some(SmolStr::new("subscribe")));
        assert_eq!(resp.conn_id, Some(SmolStr::new("abc123")));
    }

    #[test]
    fn test_deserialize_message_type() {
        let snapshot: BybitMessageType = serde_json::from_str("\"snapshot\"").unwrap();
        let delta: BybitMessageType = serde_json::from_str("\"delta\"").unwrap();

        assert_eq!(snapshot, BybitMessageType::Snapshot);
        assert_eq!(delta, BybitMessageType::Delta);
    }
}
