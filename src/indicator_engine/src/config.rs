//! Load system-wide indicators from `config/indicators.json`.
//!
//! Schema is aligned with the Python `indicator_config_generator` package (`kind` discriminator).

use crate::indicators::composite::{CompositeIndicatorConfig, IndicatorExpression};
use crate::indicators::traits::{IndicatorConfig, PriceSource};
use crate::level_pricing::LevelPricingRule;
use serde::Deserialize;
use smol_str::SmolStr;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct IndicatorsFile {
    pub version: u32,
    pub indicators: Vec<IndicatorEntry>,
    /// Level template id → indicator ref + offset (not stored in levels.json).
    #[serde(default)]
    pub level_pricing: Vec<LevelPricingRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IndicatorEntry {
    Simple {
        id: String,
        name: String,
        pair: String,
        period: usize,
        source: PriceSource,
    },
    Composite {
        id: String,
        name: String,
        pair: String,
        expression: IndicatorExpression,
        dependencies: Vec<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("validation error: {0}")]
    Validation(String),
}

pub fn load_indicators_file(path: &Path) -> Result<IndicatorsFile, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let file: IndicatorsFile = serde_json::from_str(&content)?;
    validate_file(&file)?;
    Ok(file)
}

fn validate_file(file: &IndicatorsFile) -> Result<(), ConfigError> {
    let mut ids = std::collections::HashSet::new();
    let mut simple_ids = std::collections::HashSet::new();

    for entry in &file.indicators {
        let id = entry.id();
        if !ids.insert(id.clone()) {
            return Err(ConfigError::Validation(format!(
                "duplicate indicator id: {id}"
            )));
        }
        if entry.is_simple() {
            simple_ids.insert(id);
        }
    }

    for entry in &file.indicators {
        if let IndicatorEntry::Composite {
            id,
            pair,
            dependencies,
            ..
        } = entry
        {
            for dep in dependencies {
                if !simple_ids.contains(&SmolStr::new(dep)) {
                    return Err(ConfigError::Validation(format!(
                        "composite {id}: dependency {dep} is not a simple indicator in this file"
                    )));
                }
                let suffix = format!("_{pair}");
                if !dep.ends_with(&suffix) {
                    return Err(ConfigError::Validation(format!(
                        "composite {id}: dependency {dep} pair mismatch (expected *_{pair})"
                    )));
                }
            }
        }
    }

    for rule in &file.level_pricing {
        if rule.level_id.is_empty() {
            return Err(ConfigError::Validation(
                "level_pricing: level_id must not be empty".to_string(),
            ));
        }
        if rule.indicator_ref.is_empty() {
            return Err(ConfigError::Validation(format!(
                "level_pricing: indicator_ref must not be empty for level {}",
                rule.level_id
            )));
        }
    }

    Ok(())
}

impl IndicatorEntry {
    pub fn id(&self) -> SmolStr {
        SmolStr::new(match self {
            IndicatorEntry::Simple { id, .. } | IndicatorEntry::Composite { id, .. } => id,
        })
    }

    pub fn is_simple(&self) -> bool {
        matches!(self, IndicatorEntry::Simple { .. })
    }

    pub fn into_simple_config(self) -> Option<IndicatorConfig> {
        match self {
            IndicatorEntry::Simple {
                id,
                name,
                pair,
                period,
                source,
            } => Some(IndicatorConfig {
                id: SmolStr::new(id),
                name: SmolStr::new(name),
                pair: SmolStr::new(pair),
                period,
                source,
            }),
            IndicatorEntry::Composite { .. } => None,
        }
    }

    pub fn into_composite_config(self) -> Option<CompositeIndicatorConfig> {
        match self {
            IndicatorEntry::Composite {
                id,
                name,
                pair,
                expression,
                dependencies,
            } => Some(CompositeIndicatorConfig {
                id: SmolStr::new(id),
                name: SmolStr::new(name),
                pair: SmolStr::new(pair),
                expression,
                dependencies: dependencies.into_iter().map(SmolStr::new).collect(),
            }),
            IndicatorEntry::Simple { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_default_fixture() {
        let path = fixture_path();
        if !path.exists() {
            return;
        }
        let file = load_indicators_file(&path).expect("fixture should parse");
        assert!(!file.indicators.is_empty());
        assert_eq!(file.level_pricing.len(), 2);
    }

    fn fixture_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/indicators.json")
    }

    #[test]
    fn fixture_has_expected_pairs_and_templates() {
        let path = fixture_path();
        if !path.exists() {
            return;
        }
        let file = load_indicators_file(&path).unwrap();
        let pairs: std::collections::HashSet<_> = file
            .indicators
            .iter()
            .map(|e| match e {
                IndicatorEntry::Simple { pair, .. } => pair.as_str(),
                IndicatorEntry::Composite { pair, .. } => pair.as_str(),
            })
            .collect();
        assert!(pairs.contains("BTCUSDT"));
        assert!(pairs.contains("ETHUSDT"));
    }
}
