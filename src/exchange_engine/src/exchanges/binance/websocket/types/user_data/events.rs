use super::{
    execution_report::ExecutionReport,
    outbound_account_position::{BalanceUpdate, OutboundAccountPosition},
};

use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[derive(Debug, PartialEq, Eq, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
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

impl<'de> Deserialize<'de> for ParsedBinanceMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize the raw JSON value
        let raw: Value = Value::deserialize(deserializer)?;
        // If it's a wrapper with an "event" field, drill in one level
        let v = if let Some(inner) = raw.get("event") {
            inner.clone()
        } else {
            raw
        };

        // Now we can find the "e" field on that inner object
        let event_type = v.get("e").and_then(|e| e.as_str()).unwrap_or_default();

        match event_type {
            "outboundAccountPosition" => {
                let pos = serde_json::from_value(v)
                    .map(ParsedBinanceMessage::OutboundAccountPosition)
                    .map_err(de::Error::custom)?;
                Ok(pos)
            }
            "balanceUpdate" => {
                let upd = serde_json::from_value(v)
                    .map(ParsedBinanceMessage::BalanceUpdate)
                    .map_err(de::Error::custom)?;
                Ok(upd)
            }
            "executionReport" => {
                let rep = serde_json::from_value(v)
                    .map(ParsedBinanceMessage::ExecutionReport)
                    .map_err(de::Error::custom)?;
                Ok(rep)
            }
            _ => {
                // Unknown event: keep whatever inner JSON we found
                let raw_str = serde_json::to_string(&v).unwrap_or_else(|_| "<invalid json>".into());
                Ok(ParsedBinanceMessage::Raw(raw_str.into()))
            }
        }
    }
}

impl ToSmolStr for ParsedBinanceMessage {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            ParsedBinanceMessage::OutboundAccountPosition(_) => {
                SmolStr::new("outboundAccountPosition")
            }
            ParsedBinanceMessage::BalanceUpdate(_) => SmolStr::new("balanceUpdate"),
            ParsedBinanceMessage::ExecutionReport(_) => SmolStr::new("executionReport"),
            ParsedBinanceMessage::Raw(s) => s.clone(),
        }
    }
}

impl From<&ParsedBinanceMessage> for BinanceWsEventType {
    fn from(msg: &ParsedBinanceMessage) -> Self {
        match msg {
            ParsedBinanceMessage::OutboundAccountPosition(_) => {
                BinanceWsEventType::OutboundAccountPosition
            }
            ParsedBinanceMessage::BalanceUpdate(_) => BinanceWsEventType::BalanceUpdate,
            ParsedBinanceMessage::ExecutionReport(_) => BinanceWsEventType::ExecutionReport,
            ParsedBinanceMessage::Raw(_) => BinanceWsEventType::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use smol_str::SmolStr;

    #[test]
    fn test_deserialize_binance_ws_message() {
        let json = r#"{"id":"test-id","method":"userDataStream.subscribe"}"#;
        let msg: BinanceWSMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, SmolStr::new("test-id"));
        assert_eq!(msg.method, SmolStr::new("userDataStream.subscribe"));
    }

    #[test]
    fn test_deserialize_parsed_outbound_account_position() {
        // minimal stub of the event wrapper; your real OutboundAccountPosition
        // struct will fill in the fields
        let json = r#"
        {
            "event": {
                "e": "outboundAccountPosition",
                "u": 1620000000000,
                "B": []
            }
        }
        "#;
        let parsed: ParsedBinanceMessage = serde_json::from_str(json).unwrap();
        match parsed {
            ParsedBinanceMessage::OutboundAccountPosition(_) => { /* ok */ }
            other => panic!("expected OutboundAccountPosition, got {:?}", other),
        }
    }

    #[test]
    fn test_deserialize_parsed_balance_update() {
        let json = r#"
        {
            "e":"balanceUpdate",
            "E":1620000000001,
            "a":"BTC",
            "d":"0.123",
            "T":1620000000002
        }
        "#;
        let parsed: ParsedBinanceMessage = serde_json::from_str(json).unwrap();
        match parsed {
            ParsedBinanceMessage::BalanceUpdate(_) => { /* ok */ }
            other => panic!("expected BalanceUpdate, got {:?}", other),
        }
    }

    #[test]
    fn test_deserialize_parsed_execution_report() {
        let json = r#"
        {
            "e":"executionReport",
            "E":1620000000003,
            "s":"BTCUSDT",
            "c":"myClientOrderId",
            "S":"BUY",
            "o":"LIMIT",
            "f":"GTC",
            "q":"0.01",
            "p":"50000.00",
            "F":"0",
            "g":-1,
            "C":null,
            "x":"NEW",
            "X":"NEW",
            "r":"NONE",
            "i":1,
            "l":"0.00",
            "z":"0.00",
            "L":"0.00",
            "n":null,
            "T":1620000000004,
            "t":-1,
            "I":1620000000005,
            "w":false,
            "m":false,
            "M":false
        }
        "#;
        let parsed: ParsedBinanceMessage = serde_json::from_str(json).unwrap();
        match parsed {
            ParsedBinanceMessage::ExecutionReport(_) => { /* ok */ }
            other => panic!("expected ExecutionReport, got {:?}", other),
        }
    }

    #[test]
    fn test_deserialize_parsed_raw_fallback() {
        let json = r#"{"foo":"bar","baz":123}"#;
        let parsed: ParsedBinanceMessage = serde_json::from_str(json).unwrap();
        if let ParsedBinanceMessage::Raw(s) = parsed {
            let got_val: Value = serde_json::from_str(&s).unwrap();
            let want_val: Value = serde_json::from_str(json).unwrap();
            assert_eq!(got_val, want_val);
        } else {
            panic!("expected Raw, got {:?}", parsed);
        }
    }
}
