use super::websocket::types::common::BinanceSymbol;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceConfig {
    pub symbol: BinanceSymbol,
    pub limit: u64,
}
