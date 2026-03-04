//! Paper (k-paper) market data config.
//! Same shape as used by exchange_engine for Paper; only url + pairs needed for MD.

use serde::{Deserialize, Serialize};
use trading_core::types::common::Pair;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperConfig {
    pub url: String,
    pub pairs: Vec<Pair>,
}
