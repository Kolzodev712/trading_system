//! Level → indicator pricing bindings (stored in indicators config / Mongo, not in levels.json).

use rust_decimal::Decimal;
use serde::Deserialize;
use smol_str::SmolStr;
use std::collections::HashMap;

/// How to offset price from the bound indicator value (aligned with levels `PriceOffsetType` JSON).
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum PricingOffsetType {
    Absolute,
    Percent,
    PercentBelow,
    PercentAbove,
}

/// Binds a level template id (e.g. `buy_level_1`) to an indicator ref (e.g. `sma_20`) and offset.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct LevelPricingRule {
    pub level_id: String,
    pub indicator_ref: String,
    pub offset_type: PricingOffsetType,
    pub offset_value: Decimal,
}

#[derive(Debug, Clone, Default)]
pub struct LevelPricingRegistry {
    rules: HashMap<SmolStr, LevelPricingRule>,
}

impl LevelPricingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_rules(rules: Vec<LevelPricingRule>) -> Result<Self, String> {
        let mut registry = Self::new();
        for rule in rules {
            registry.insert(rule)?;
        }
        Ok(registry)
    }

    pub fn insert(&mut self, rule: LevelPricingRule) -> Result<(), String> {
        let id = SmolStr::new(&rule.level_id);
        if self.rules.contains_key(&id) {
            return Err(format!(
                "duplicate level_pricing for level_id: {}",
                rule.level_id
            ));
        }
        self.rules.insert(id, rule);
        Ok(())
    }

    pub fn merge(&mut self, other: LevelPricingRegistry) {
        for (k, v) in other.rules {
            self.rules.insert(k, v);
        }
    }

    pub fn get(&self, level_template_id: &str) -> Option<&LevelPricingRule> {
        self.rules.get(&SmolStr::new(level_template_id))
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
