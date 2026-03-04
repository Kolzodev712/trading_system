//! k-paper integration config (wire format from bot.json / runner).

use serde::{Deserialize, Serialize};
use trading_core::types::common::{InstrumentType, Pair};

/// Paper exchange (k-paper) configuration.
/// Lives in the integration module; top-level ExchangeConfig references this.
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct PaperConfig {
    /// WebSocket URL (e.g. "ws://127.0.0.1:19999/ws")
    #[serde(default = "default_paper_url")]
    pub paper_exchange_url: String,
    /// Pairs to trade (e.g. BTC-USDT)
    pub pairs: Vec<Pair>,
    /// Instrument type (default Spot)
    #[serde(default)]
    pub instrument_type: InstrumentType,
}

fn default_paper_url() -> String {
    "ws://127.0.0.1:19999/ws".to_string()
}
