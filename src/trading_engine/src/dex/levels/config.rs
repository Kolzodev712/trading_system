//! DEX quote configuration (`config/dex_levels.json`).

use std::path::Path;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trading_core::types::common::Side;

use crate::dex::DexPriceConfig;

fn default_true() -> bool {
    true
}

fn default_repost_delay_secs() -> u64 {
    5
}

/// File-level defaults applied to each quote unless overridden.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexLevelsDefaults {
    #[serde(default)]
    pub pay_fees_in_deep: bool,
    #[serde(default = "default_true")]
    pub auto_repost: bool,
    #[serde(default = "default_repost_delay_secs")]
    pub repost_delay_secs: u64,
}

impl Default for DexLevelsDefaults {
    fn default() -> Self {
        Self {
            pay_fees_in_deep: false,
            auto_repost: true,
            repost_delay_secs: default_repost_delay_secs(),
        }
    }
}

/// Single quote on a DeepBook pool (fixed or indicator-derived price).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexQuoteConfig {
    pub id: String,
    /// DeepBook pool name (e.g. `SUI_DBUSDC`).
    pub pool: String,
    pub side: Side,
    pub price: DexPriceConfig,
    pub quantity: Decimal,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Override file default for repost-on-fill.
    #[serde(default)]
    pub auto_repost: Option<bool>,
    #[serde(default)]
    pub pay_fees_in_deep: Option<bool>,
}

impl DexQuoteConfig {
    pub fn auto_repost(&self, defaults: &DexLevelsDefaults) -> bool {
        self.auto_repost.unwrap_or(defaults.auto_repost)
    }

    pub fn pay_fees_in_deep(&self, defaults: &DexLevelsDefaults) -> bool {
        self.pay_fees_in_deep.unwrap_or(defaults.pay_fees_in_deep)
    }
}

/// Root config file for DEX levels mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexLevelsFile {
    pub quotes: Vec<DexQuoteConfig>,
    #[serde(default)]
    pub defaults: DexLevelsDefaults,
}

impl DexLevelsFile {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("read {}: {e}", path.as_ref().display()))?;
        serde_json::from_str(&content).map_err(|e| format!("parse dex levels config: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn deserializes_example_shape() {
        let json = r#"{
            "quotes": [{
                "id": "bid_1",
                "pool": "SUI_DBUSDC",
                "side": "Buy",
                "price": "0.70",
                "quantity": "1.0",
                "enabled": true
            }],
            "defaults": { "auto_repost": true, "repost_delay_secs": 5 }
        }"#;
        let file: DexLevelsFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.quotes.len(), 1);
        assert_eq!(file.quotes[0].price, DexPriceConfig::Fixed(dec!(0.70)));
        assert!(file.quotes[0].auto_repost(&file.defaults));
    }

    #[test]
    fn deserializes_binding_price() {
        let json = r#"{
            "quotes": [{
                "id": "bid_1",
                "pool": "SUI_DBUSDC",
                "side": "Buy",
                "price": "Binding",
                "quantity": "1.0",
                "enabled": true
            }],
            "defaults": {}
        }"#;
        let file: DexLevelsFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.quotes[0].price, DexPriceConfig::Binding);
    }
}
