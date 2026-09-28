//! k-paper integration config (wire format from bot.json / runner).
//!
//! Paper exchange exposes `/ws`, `/ws/market`, and `/ws/trade`. The exchange engine
//! must use **`/ws/trade`** (or `/ws`) so orders and execution/ack messages are on the same connection.

use serde::{Deserialize, Serialize};
use trading_core::types::common::{InstrumentType, Pair};

/// Paper exchange (k-paper) configuration.
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct PaperConfig {
    /// WebSocket URL for trading (e.g. "ws://127.0.0.1:19999/ws/trade"). Use /ws/trade or /ws so executions/acks are received.
    #[serde(default = "default_paper_url")]
    pub paper_exchange_url: String,
    /// Pairs to trade (e.g. BTC-USDT)
    pub pairs: Vec<Pair>,
    /// Instrument type (default Spot)
    #[serde(default)]
    pub instrument_type: InstrumentType,
    /// Explicit linear leverage for Exchange futures. Required; no silent default.
    #[serde(default)]
    pub leverage: Option<u32>,
}

fn default_paper_url() -> String {
    "ws://127.0.0.1:19999/ws/trade".to_string()
}
