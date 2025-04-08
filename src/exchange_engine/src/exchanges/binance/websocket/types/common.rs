use super::{
    balance::{BalanceUpdate, OutboundAccountPosition},
    orders::ExecutionReport,
};
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinanceWSMessage {
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

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum BinanceWsEventType {
    OutboundAccountPosition,
    BalanceUpdate,
    ExecutionReport,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum ParsedBinanceMessage {
    OutboundAccountPosition(OutboundAccountPosition),
    BalanceUpdate(BalanceUpdate),
    ExecutionReport(ExecutionReport),
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    Raw(SmolStr),
}

impl ToSmolStr for ParsedBinanceMessage {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            ParsedBinanceMessage::OutboundAccountPosition(_) => {
                SmolStr::new("outboundAccountPosition")
            }
            ParsedBinanceMessage::BalanceUpdate(_) => SmolStr::new("balanceUpdate"),
            ParsedBinanceMessage::ExecutionReport(_) => SmolStr::new("executionReport"),
            ParsedBinanceMessage::Raw(smol_str) => smol_str.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
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
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub algo: SmolStr,
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
                "signature": "BASE64_SIGNATURE",
                "algo": "ED25519"
            }
        }
        "#;

        let logon: SessionLogonMessage = serde_json::from_str(json).unwrap();

        assert_eq!(logon.id, "3b42f841-b8ff-4b1d-84d4-4a65efddc0da");
        assert_eq!(logon.method, "session.logon");

        assert_eq!(logon.params.api_key, "MY_API_KEY");
        assert_eq!(logon.params.timestamp, 1700000000000);
        assert_eq!(logon.params.signature, "BASE64_SIGNATURE");
        assert_eq!(logon.params.algo, "ED25519");
    }
}
