use super::rest::types::BinanceSymbol;
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceConfig {
    pub symbol: BinanceSymbol,
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
    pub recv_window: Option<u64>,
    pub limit: Option<u64>,
    pub timestamp: Option<u64>,
}
