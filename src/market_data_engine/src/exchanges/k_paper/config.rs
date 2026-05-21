//! Paper (k-paper) market data config.
//!
//! Paper exchange exposes `/ws/market` (orderbook + public_trade only). The market data
//! engine must use that path. You can pass the same URL as the exchange engine (e.g. with
//! `/ws/trade`); we derive the market endpoint when connecting.

use serde::{Deserialize, Serialize};
use trading_core::types::common::Pair;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperConfig {
    /// WebSocket URL (exchange URL with any path, e.g. "ws://127.0.0.1:19999/ws/trade").
    /// We connect to the same host with path /ws/market.
    pub url: String,
    /// Pairs to forward from the k-paper market feed (must match `bot.json` exchange pairs).
    /// When non-empty, orderbook/trade messages for other instruments are dropped in `PaperClient`.
    pub pairs: Vec<Pair>,
}

/// Derive market WebSocket URL from configured URL (exchange uses /ws/trade; we use /ws/market).
pub fn to_market_ws_url(configured: &str) -> String {
    if let Ok(mut u) = url::Url::parse(configured) {
        u.set_path("/ws/market");
        return u.to_string();
    }
    let s = configured.trim_end_matches('/');
    let base = s
        .strip_suffix("/ws/trade")
        .or_else(|| s.strip_suffix("/ws/market"))
        .or_else(|| s.strip_suffix("/ws"))
        .unwrap_or(s);
    format!("{}/ws/market", base)
}
