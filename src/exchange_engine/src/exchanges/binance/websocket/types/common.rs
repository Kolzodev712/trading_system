use super::{rpc::BinanceRpcResponse, user_data::events::ParsedBinanceMessage};
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BinanceInbound {
    Event(ParsedBinanceMessage),
    Rpc(BinanceRpcResponse),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLogonMessage {
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
    pub params: SessionLogonParams,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLogonParams {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub api_key: SmolStr,
    pub timestamp: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub signature: SmolStr,
}

impl Default for SessionLogonParams {
    fn default() -> Self {
        SessionLogonParams {
            api_key: SmolStr::new(""),
            timestamp: 0,
            signature: SmolStr::new(""),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SesionStatusMessage {
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
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SesionLogoutMessage {
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
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTime {
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
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTimeResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    pub status: i64,
    pub result: TimeResponse,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeResponse {
    server_time: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceRateLimit {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub rate_limit_type: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub interval: SmolStr,
    pub interval_num: i64,
    pub limit: i64,
    pub count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_deserialize_session_logon_message() {
        let json = r#"
        {
            "id": "3b42f841-b8ff-4b1d-84d4-4a65efddc0da",
            "method": "session.logon",
            "params": {
                "apiKey": "MY_API_KEY",
                "timestamp": 1700000000000,
                "signature": "BASE64_SIGNATURE"
            }
        }
        "#;

        let logon: SessionLogonMessage = serde_json::from_str(json).unwrap();

        assert_eq!(logon.id, "3b42f841-b8ff-4b1d-84d4-4a65efddc0da");
        assert_eq!(logon.method, "session.logon");

        assert_eq!(logon.params.api_key, "MY_API_KEY");
        assert_eq!(logon.params.timestamp, 1700000000000);
        assert_eq!(logon.params.signature, "BASE64_SIGNATURE");
    }

    #[test]
    fn test_serialize_session_logon_message() {
        let message = SessionLogonMessage {
            id: SmolStr::new("abc123"),
            method: SmolStr::new("session.logon"),
            params: SessionLogonParams {
                api_key: SmolStr::new("KEY123"),
                timestamp: 1700000000000,
                signature: SmolStr::new("SIGNATURE123"),
            },
        };

        let json = serde_json::to_string(&message).unwrap();
        let expected = r#"{"id":"abc123","method":"session.logon","params":{"apiKey":"KEY123","timestamp":1700000000000,"signature":"SIGNATURE123"}}"#;

        assert_eq!(json, expected);
    }

    #[test]
    fn test_serialize_session_status_message() {
        let msg = SesionStatusMessage {
            id: SmolStr::new("id123"),
            method: SmolStr::new("session.status"),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let expected = r#"{"id":"id123","method":"session.status"}"#;
        assert_eq!(json, expected);
    }

    #[test]
    fn test_deserialize_session_status_message() {
        let json = r#"
        {
            "id": "status_id",
            "method": "session.status"
        }
        "#;

        let msg: SesionStatusMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "status_id");
        assert_eq!(msg.method, "session.status");
    }

    #[test]
    fn test_serialize_session_logout_message() {
        let msg = SesionLogoutMessage {
            id: SmolStr::new("logout_id"),
            method: SmolStr::new("session.logout"),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let expected = r#"{"id":"logout_id","method":"session.logout"}"#;
        assert_eq!(json, expected);
    }

    #[test]
    fn test_serialize_server_time() {
        let time = ServerTime {
            id: SmolStr::new("time_id"),
            method: SmolStr::new("server.time"),
        };

        let json = serde_json::to_string(&time).unwrap();
        let expected = r#"{"id":"time_id","method":"server.time"}"#;
        assert_eq!(json, expected);
    }

    #[test]
    fn test_serialize_server_time_response() {
        let resp = ServerTimeResponse {
            id: SmolStr::new("res_id"),
            status: 0,
            result: TimeResponse {
                server_time: 123456789,
            },
        };

        let json = serde_json::to_string(&resp).unwrap();
        let expected = r#"{"id":"res_id","status":0,"result":{"serverTime":123456789}}"#;
        assert_eq!(json, expected);
    }

    #[test]
    fn test_deserialize_server_time_response() {
        let json = r#"
        {
            "id": "res_id",
            "status": 1,
            "result": {
                "serverTime": 987654321
            }
        }
        "#;

        let msg: ServerTimeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "res_id");
        assert_eq!(msg.status, 1);
        assert_eq!(msg.result.server_time, 987654321);
    }

    #[test]
    fn test_serialize_binance_rate_limit() {
        let rate_limit = BinanceRateLimit {
            rate_limit_type: SmolStr::new("REQUEST_WEIGHT"),
            interval: SmolStr::new("MINUTE"),
            interval_num: 1,
            limit: 1200,
            count: 5,
        };

        let json = serde_json::to_string(&rate_limit).unwrap();
        let expected = r#"{"rateLimitType":"REQUEST_WEIGHT","interval":"MINUTE","intervalNum":1,"limit":1200,"count":5}"#;
        assert_eq!(json, expected);
    }

    #[test]
    fn test_deserialize_binance_rate_limit() {
        let json = r#"
        {
            "rateLimitType": "ORDERS",
            "interval": "SECOND",
            "intervalNum": 10,
            "limit": 50,
            "count": 2
        }
        "#;

        let rate_limit: BinanceRateLimit = serde_json::from_str(json).unwrap();
        assert_eq!(rate_limit.rate_limit_type, "ORDERS");
        assert_eq!(rate_limit.interval, "SECOND");
        assert_eq!(rate_limit.interval_num, 10);
        assert_eq!(rate_limit.limit, 50);
        assert_eq!(rate_limit.count, 2);
    }
}
