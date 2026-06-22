//! DeepBook market data configuration.

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepBookMarketDataConfig {
    pub indexer_url: SmolStr,
    pub pools: Vec<SmolStr>,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_orderbook_depth")]
    pub orderbook_depth: u32,
    #[serde(default = "default_trades_limit")]
    pub trades_limit: u32,
}

fn default_poll_interval_ms() -> u64 {
    1000
}

fn default_orderbook_depth() -> u32 {
    50
}

fn default_trades_limit() -> u32 {
    20
}
