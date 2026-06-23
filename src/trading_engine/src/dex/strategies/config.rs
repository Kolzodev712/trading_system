//! DEX strategy configuration (`config/dex/strategies.json`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::types::{DexMarket, DexStrategyId, DexStrategyKind};

fn default_true() -> bool {
    true
}

fn default_eval_cooldown_secs() -> u64 {
    5
}

fn default_failure_backoff_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexStrategyDefaults {
    #[serde(default = "default_eval_cooldown_secs")]
    pub eval_cooldown_secs: u64,
    #[serde(default = "default_failure_backoff_secs")]
    pub failure_backoff_secs: u64,
}

impl Default for DexStrategyDefaults {
    fn default() -> Self {
        Self {
            eval_cooldown_secs: default_eval_cooldown_secs(),
            failure_backoff_secs: default_failure_backoff_secs(),
        }
    }
}

/// One configured DEX strategy instance (venue-agnostic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexStrategyEntryConfig {
    pub id: DexStrategyId,
    #[serde(rename = "type")]
    pub strategy_type: DexStrategyKind,
    pub market: DexMarket,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexStrategiesFile {
    pub strategies: Vec<DexStrategyEntryConfig>,
    #[serde(default)]
    pub defaults: DexStrategyDefaults,
}

impl DexStrategiesFile {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("read {}: {e}", path.as_ref().display()))?;
        serde_json::from_str(&content).map_err(|e| format!("parse dex strategies config: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_observe_mid_strategy() {
        let json = r#"{
            "strategies": [{
                "id": "observe_mid_sui_dbusdc",
                "type": "ObserveMid",
                "market": "SUI_DBUSDC",
                "enabled": true
            }]
        }"#;
        let file: DexStrategiesFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.strategies.len(), 1);
        assert_eq!(
            file.strategies[0].strategy_type,
            DexStrategyKind::ObserveMid
        );
        assert_eq!(file.strategies[0].id.as_str(), "observe_mid_sui_dbusdc");
        assert_eq!(file.strategies[0].market.as_str(), "SUI_DBUSDC");
    }
}
