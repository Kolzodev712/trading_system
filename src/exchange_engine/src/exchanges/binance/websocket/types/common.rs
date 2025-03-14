use super::{
    balance::{BalanceUpdate, OutboundAccountPosition},
    orders::ExecutionReport,
};
use core_logic::utils::{
    deserialize_smol_str, deserialize_vec_smol_str, serialize_smol_str, serialize_vec_smol_str,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinanceWSMessage {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub method: SmolStr,
    #[serde(
        serialize_with = "serialize_vec_smol_str",
        deserialize_with = "deserialize_vec_smol_str"
    )]
    pub params: Vec<SmolStr>,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListStatus {
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "g")]
    pub order_list_id: u64,
    #[serde(rename = "c")]
    pub contingency_type: String,
    #[serde(rename = "l")]
    pub list_status_type: String,
    #[serde(rename = "L")]
    pub list_order_status: String,
    #[serde(rename = "r")]
    pub list_reject_reason: Option<String>,
    #[serde(rename = "C")]
    pub client_order_id: String,
    #[serde(rename = "T")]
    pub transaction_time: u64,
    #[serde(rename = "O")]
    pub orders: Vec<ListStatusOrder>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListStatusOrder {
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "i")]
    pub order_id: u64,
    #[serde(rename = "c")]
    pub client_order_id: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ListenKeyExpired {
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub listen_key: SmolStr,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventStreamTerminated {
    #[serde(rename = "E")]
    pub event_time: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLockUpdate {
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    #[serde(rename = "a")]
    pub asset: SmolStr,
    #[serde(rename = "d")]
    pub delta: Decimal,
    #[serde(rename = "T")]
    pub transaction_time: u64,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum BinanceWsEventType {
    OutboundAccountPosition,
    BalanceUpdate,
    ExecutionReport,
    ListStatus,
    ListenKeyExpired,
    EventStreamTerminated,
    ExternalLockUpdate,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum ParsedBinanceMessage {
    OutboundAccountPosition(OutboundAccountPosition),
    BalanceUpdate(BalanceUpdate),
    ExecutionReport(ExecutionReport),
    ListStatus(ListStatus),
    ListenKeyExpired(ListenKeyExpired),
    EventStreamTerminated(EventStreamTerminated),
    ExternalLockUpdate(ExternalLockUpdate),
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
            ParsedBinanceMessage::ListStatus(_) => SmolStr::new("listStatus"),
            ParsedBinanceMessage::ListenKeyExpired(_) => SmolStr::new("listenKeyExpired"),
            ParsedBinanceMessage::EventStreamTerminated(_) => SmolStr::new("eventStreamTerminated"),
            ParsedBinanceMessage::ExternalLockUpdate(_) => SmolStr::new("externalLockUpdate"),
            ParsedBinanceMessage::Raw(smol_str) => smol_str.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    // Test deserialization for the ListStatus message
    #[test]
    fn test_deserialize_list_status() {
        let json = r#"
        {
            "e": "listStatus",
            "E": 1564035303637,
            "s": "ETHBTC",
            "g": 2,
            "c": "OCO",
            "l": "EXEC_STARTED",
            "L": "EXECUTING",
            "r": "NONE",
            "C": "F4QN4G8DlFATFlIUQ0cjdD",
            "T": 1564035303625,
            "O": [
                {
                    "s": "ETHBTC",
                    "i": 17,
                    "c": "AJYsMjErWJesZvqlJCTUgL"
                },
                {
                    "s": "ETHBTC",
                    "i": 18,
                    "c": "bfYPSQdLoqAJeNrOr9adzq"
                }
            ]
        }
        "#;

        let list_status: ListStatus = serde_json::from_str(json).unwrap();
        assert_eq!(list_status.symbol, "ETHBTC");
        assert_eq!(list_status.order_list_id, 2);
        assert_eq!(list_status.orders.len(), 2);
        assert_eq!(list_status.orders[0].symbol, "ETHBTC");
        assert_eq!(list_status.orders[0].order_id, 17);
    }

    // Test deserialization for the ListenKeyExpired message
    #[test]
    fn test_deserialize_listen_key_expired() {
        let json = r#"
        {
            "e": "listenKeyExpired",
            "E": 1699596037418,
            "listenKey": "OfYGbUzi3PraNagEkdKuFwUHn48brFsItTdsuiIXrucEvD0rhRXZ7I6URWfE8YE8"
        }
        "#;

        let listen_key_expired: ListenKeyExpired = serde_json::from_str(json).unwrap();
        assert_eq!(
            listen_key_expired.listen_key,
            "OfYGbUzi3PraNagEkdKuFwUHn48brFsItTdsuiIXrucEvD0rhRXZ7I6URWfE8YE8"
        );
    }

    // Test deserialization for the EventStreamTerminated message
    #[test]
    fn test_deserialize_event_stream_terminated() {
        let json = r#"
        {
            "e": "eventStreamTerminated",
            "E": 1728973001334
        }
        "#;

        let event_stream_terminated: EventStreamTerminated = serde_json::from_str(json).unwrap();
        assert_eq!(event_stream_terminated.event_time, 1728973001334);
    }

    // Test deserialization for the ExternalLockUpdate message
    #[test]
    fn test_deserialize_external_lock_update() {
        let json = r#"
        {
            "e": "externalLockUpdate",
            "E": 1581557507324,
            "a": "NEO",
            "d": "10.00000000",
            "T": 1581557507268
        }
        "#;

        let external_lock_update: ExternalLockUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(external_lock_update.asset, "NEO");
        assert_eq!(external_lock_update.delta, Decimal::new(100_000_000_0, 8));
    }
}
