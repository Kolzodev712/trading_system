use serde::{Deserialize, Serialize};

use super::rest::types::BinanceSymbol;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceConfig {
    pub symbol: BinanceSymbol,
    pub api_key: String,
    pub secret_key: String,
    pub recv_window: Option<u64>,
    pub limit: Option<u64>,
    pub timestamp: Option<u64>,
}
