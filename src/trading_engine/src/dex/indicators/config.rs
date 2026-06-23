//! `config/dex/indicators.json` — pool indicators and quote price bindings.

use std::path::Path;

use rust_decimal::Decimal;
use serde::Deserialize;
use smol_str::SmolStr;
use std::collections::HashMap;

use super::types::{DexIndicatorKind, DexPricingOffsetType};

/// Declarative indicator published for a DeepBook pool.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DexIndicatorDef {
    pub id: String,
    pub pool: String,
    pub kind: DexIndicatorKind,
}

/// Binds a quote id to an indicator ref and offset (CEX `level_pricing` analogue).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DexQuotePricingRule {
    pub quote_id: String,
    pub indicator_ref: String,
    pub offset_type: DexPricingOffsetType,
    pub offset_value: Decimal,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct DexIndicatorsFile {
    #[serde(default)]
    pub indicators: Vec<DexIndicatorDef>,
    #[serde(default)]
    pub quote_pricing: Vec<DexQuotePricingRule>,
}

impl DexIndicatorsFile {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("read {}: {e}", path.as_ref().display()))?;
        serde_json::from_str(&content).map_err(|e| format!("parse dex indicators config: {e}"))
    }
}

#[derive(Debug, Clone, Default)]
pub struct DexQuotePricingRegistry {
    rules: HashMap<SmolStr, DexQuotePricingRule>,
}

impl DexQuotePricingRegistry {
    pub fn from_rules(rules: Vec<DexQuotePricingRule>) -> Result<Self, String> {
        let mut registry = Self::default();
        for rule in rules {
            registry.insert(rule)?;
        }
        Ok(registry)
    }

    pub fn insert(&mut self, rule: DexQuotePricingRule) -> Result<(), String> {
        let id = SmolStr::new(&rule.quote_id);
        if self.rules.contains_key(&id) {
            return Err(format!(
                "duplicate quote_pricing for quote_id: {}",
                rule.quote_id
            ));
        }
        self.rules.insert(id, rule);
        Ok(())
    }

    pub fn get(&self, quote_id: &str) -> Option<&DexQuotePricingRule> {
        self.rules.get(&SmolStr::new(quote_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn deserializes_indicators_and_quote_pricing() {
        let json = r#"{
            "indicators": [
                { "id": "mid_sui", "pool": "SUI_DBUSDC", "kind": "Mid" }
            ],
            "quote_pricing": [
                {
                    "quote_id": "bid_1",
                    "indicator_ref": "mid_sui",
                    "offset_type": "PercentBelow",
                    "offset_value": "0.5"
                }
            ]
        }"#;
        let file: DexIndicatorsFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.indicators.len(), 1);
        assert_eq!(file.quote_pricing[0].offset_value, dec!(0.5));
    }
}
