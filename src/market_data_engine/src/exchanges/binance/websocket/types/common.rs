use core_logic::{
    data::public_data::{Orderbook, Trade},
    utils::{
        deserialize_option_smol_str, deserialize_smol_str, deserialize_vec_smol_str,
        serialize_option_smol_str, serialize_smol_str, serialize_vec_smol_str,
    },
};
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};
use std::primitive::str;

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

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct InitialResponse {
    #[serde(
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    result: Option<SmolStr>,
    id: u64,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinanceWsEventType {
    #[serde(rename = "initialResponse")]
    InitialResponse,
    #[serde(rename = "depthUpdate")]
    DepthUpdate,
    #[serde(rename = "trade")]
    Trade,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum ParsedBinanceMessage {
    InitialResponse(InitialResponse),
    OrderBook(Orderbook),
    Trade(Trade),
    Raw(
        #[serde(
            serialize_with = "serialize_smol_str",
            deserialize_with = "deserialize_smol_str"
        )]
        SmolStr,
    ),
}

impl ToSmolStr for ParsedBinanceMessage {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            ParsedBinanceMessage::InitialResponse(_) => SmolStr::new("InitialResponse"),
            ParsedBinanceMessage::OrderBook(_) => SmolStr::new("OrderBook"),
            ParsedBinanceMessage::Trade(_) => SmolStr::new("Trade"),
            ParsedBinanceMessage::Raw(smol_str) => smol_str.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceSymbol {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub base: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub quote: SmolStr,
}
