use core_logic::{
    data::{base_data::Pair, private_data::Balance},
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
pub struct Login {
    #[serde(
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    result: Option<SmolStr>,
    id: u64,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinanceWsEventType {
    #[serde(rename = "session.logon")]
    Login,
    #[serde(rename = "account.status")]
    Balance,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum ParsedBinanceMessage {
    Login(Login),
    Balance(Balance),
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
            ParsedBinanceMessage::Login(_) => SmolStr::new("LoginResponse"),
            ParsedBinanceMessage::Balance(_) => SmolStr::new("Balances"),
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

impl From<Pair> for BinanceSymbol {
    fn from(pair: Pair) -> Self {
        BinanceSymbol {
            base: pair.base,
            quote: pair.quote,
        }
    }
}
