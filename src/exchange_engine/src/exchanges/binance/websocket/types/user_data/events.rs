use super::{
    execution_report::ExecutionReport,
    outbound_account_position::{BalanceUpdate, OutboundAccountPosition},
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
