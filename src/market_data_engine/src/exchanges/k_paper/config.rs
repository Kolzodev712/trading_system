//! Paper (k-paper) market data config.
//!
//! Paper exchange exposes `/ws/market` (orderbook + public_trade only). The market data
//! engine must use that URL so this connection does not receive executions or acks.

use serde::{Deserialize, Serialize};
use trading_core::types::common::Pair;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperConfig {
    /// WebSocket URL for market data (e.g. "ws://127.0.0.1:19999/ws/market").
    pub url: String,
    pub pairs: Vec<Pair>,
}
