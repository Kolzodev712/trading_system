use super::rest::types::common::BinanceSymbol;
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceConfig {
    pub symbol: BinanceSymbol,
    pub rest: BinanceCredentials,
    pub ws: BinanceCredentials,
    pub limit: Option<u64>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCredentials {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub api_key: SmolStr,

    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub secret_key: SmolStr,
}
