//! DeepBook pool metadata and helpers.

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::types::common::Pair;

/// Pool metadata from DeepBook indexer `GET /get_pools`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeepBookPoolMeta {
    pub pool_id: SmolStr,
    pub pool_name: SmolStr,
    pub base_asset_id: SmolStr,
    pub base_asset_decimals: u32,
    pub base_asset_symbol: SmolStr,
    pub base_asset_name: SmolStr,
    pub quote_asset_id: SmolStr,
    pub quote_asset_decimals: u32,
    pub quote_asset_symbol: SmolStr,
    pub quote_asset_name: SmolStr,
    pub min_size: u64,
    pub lot_size: u64,
    pub tick_size: u64,
}

impl DeepBookPoolMeta {
    /// Display pair for orderbooks / UI (`SUI/DBUSDC`).
    pub fn pair(&self) -> Pair {
        pool_name_to_pair(&self.pool_name)
    }
}

/// Convert indexer pool name (`SUI_DBUSDC`) to internal `Pair` (`SUI/DBUSDC`).
pub fn pool_name_to_pair(pool_name: &str) -> Pair {
    match pool_name.split_once('_') {
        Some((base, quote)) => Pair::new(base, quote),
        None => Pair::new(pool_name, "UNKNOWN"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_name_to_pair_splits_on_first_underscore() {
        let pair = pool_name_to_pair("SUI_DBUSDC");
        assert_eq!(pair.base.as_str(), "SUI");
        assert_eq!(pair.quote.as_str(), "DBUSDC");
    }
}
