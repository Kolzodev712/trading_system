//! File-canonical levels catalog: templates, pair-specific instances, tombstones.
use super::config::LevelConfig;
use super::identity::{expanded_level_id, LevelIdentity};
use super::level::Level;
use crate::risk::RiskLimits;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use trading_core::error::EngineError;
use trading_core::types::common::{Exchange, InstrumentType, Pair};

/// On-disk catalog. `levels` are templates (pair-agnostic). Instances and tombstones
/// are pair-specific. Missing instances/tombstones keeps the legacy expand-all-pairs load.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LevelsCatalog {
    #[serde(default)]
    pub levels: Vec<LevelConfig>,
    #[serde(default)]
    pub instances: Vec<LevelInstanceRecord>,
    #[serde(default)]
    pub tombstones: Vec<String>,
    #[serde(default)]
    pub risk_limits: RiskLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelInstanceRecord {
    pub id: String,
    pub template_id: String,
    pub pair: Pair,
    pub config: LevelConfig,
    #[serde(default)]
    pub owned_client_ids: Vec<String>,
    #[serde(default)]
    pub owned_exchange_ids: Vec<String>,
}

impl LevelsCatalog {
    pub fn load_from_path(path: &Path) -> Result<Self, EngineError> {
        let content = fs::read_to_string(path).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to read levels file: {e}"))
        })?;
        serde_json::from_str(&content).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to parse levels file: {e}"))
        })
    }

    /// Atomic replace: temp file, fsync, rename.
    pub fn save_atomic(&self, path: &Path) -> Result<(), EngineError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("Failed to create levels dir: {e}"))
            })?;
        }
        let tmp = path.with_extension("json.tmp");
        let payload = serde_json::to_vec_pretty(self).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to serialize levels: {e}"))
        })?;
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp)
                .map_err(|e| {
                    EngineError::CustomError(anyhow::anyhow!("Failed to open levels temp: {e}"))
                })?;
            file.write_all(&payload).map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("Failed to write levels temp: {e}"))
            })?;
            file.write_all(b"\n").map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("Failed to write levels temp: {e}"))
            })?;
            file.sync_all().map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("Failed to fsync levels temp: {e}"))
            })?;
        }
        fs::rename(&tmp, path).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to replace levels file: {e}"))
        })?;
        Ok(())
    }

    pub fn tombstone_set(&self) -> HashSet<String> {
        self.tombstones.iter().cloned().collect()
    }

    pub fn materialize(
        &self,
        pairs: &[Pair],
        venue: Exchange,
        instrument: InstrumentType,
        account: &str,
    ) -> Result<HashMap<SmolStr, Level>, EngineError> {
        if pairs.is_empty() {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "No pairs provided - cannot expand levels"
            )));
        }
        let tombstones = self.tombstone_set();
        let mut out: HashMap<SmolStr, Level> = HashMap::new();

        for record in &self.instances {
            if tombstones.contains(&record.id) {
                continue;
            }
            let identity = LevelIdentity::new(
                venue.clone(),
                instrument.clone(),
                SmolStr::new(account),
                record.pair.clone(),
                SmolStr::new(&record.template_id),
            );
            if identity.instance_id.as_str() != record.id {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "instance id {} does not match template {} on {}",
                    record.id,
                    record.template_id,
                    record.pair
                )));
            }
            let mut level = Level::from_identity(record.config.clone(), identity);
            level.owned_client_ids = record.owned_client_ids.iter().map(SmolStr::new).collect();
            level.active_orders = record
                .owned_exchange_ids
                .iter()
                .map(|id| trading_core::types::common::OrderId::new(SmolStr::new(id)))
                .collect();
            if !level.active_orders.is_empty() {
                level.state = super::level::LevelState::Posted;
            }
            out.insert(level.identity.instance_id.clone(), level);
        }

        for template in &self.levels {
            for pair in pairs {
                let instance_id = expanded_level_id(template.id.as_str(), pair);
                if tombstones.contains(instance_id.as_str()) {
                    continue;
                }
                if out.contains_key(&instance_id) {
                    continue;
                }
                let identity = LevelIdentity::new(
                    venue.clone(),
                    instrument.clone(),
                    SmolStr::new(account),
                    pair.clone(),
                    template.id.clone(),
                );
                out.insert(
                    identity.instance_id.clone(),
                    Level::from_identity(template.clone(), identity),
                );
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cex::levels::config::{
        LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
    };
    use rust_decimal_macros::dec;
    use trading_core::types::common::Side;

    fn template(id: &str) -> LevelConfig {
        LevelConfig {
            id: SmolStr::new(id),
            name: None,
            side: Side::Buy,
            order_type: OrderTypeConfig::Limit,
            quantity: QuantityConfig::Fixed(dec!(0.001)),
            price: PriceConfig::Fixed(dec!(50000)),
            risk: LevelRiskConfig::default(),
            lifecycle: LifecycleConfig::default(),
            enabled: true,
            group: None,
        }
    }

    #[test]
    fn delete_one_pair_leaves_other_template_expansion() {
        let btc = Pair::new("BTC", "USDT");
        let eth = Pair::new("ETH", "USDT");
        let catalog = LevelsCatalog {
            levels: vec![template("buy_level_1")],
            instances: vec![],
            tombstones: vec!["buy_level_1_BTCUSDT".into()],
            risk_limits: RiskLimits::default(),
        };
        let levels = catalog
            .materialize(&[btc, eth], Exchange::Paper, InstrumentType::Spot, "test")
            .unwrap();
        assert!(!levels.contains_key(&SmolStr::new("buy_level_1_BTCUSDT")));
        assert!(levels.contains_key(&SmolStr::new("buy_level_1_ETHUSDT")));
    }

    #[test]
    fn instance_overrides_template_for_that_pair_only() {
        let btc = Pair::new("BTC", "USDT");
        let eth = Pair::new("ETH", "USDT");
        let mut cfg = template("buy_level_1");
        cfg.enabled = false;
        let catalog = LevelsCatalog {
            levels: vec![template("buy_level_1")],
            instances: vec![LevelInstanceRecord {
                id: "buy_level_1_BTCUSDT".into(),
                template_id: "buy_level_1".into(),
                pair: btc.clone(),
                config: cfg,
                owned_client_ids: vec![],
                owned_exchange_ids: vec![],
            }],
            tombstones: vec![],
            risk_limits: RiskLimits::default(),
        };
        let levels = catalog
            .materialize(&[btc, eth], Exchange::Paper, InstrumentType::Spot, "test")
            .unwrap();
        assert!(!levels[&SmolStr::new("buy_level_1_BTCUSDT")].config.enabled);
        assert!(levels[&SmolStr::new("buy_level_1_ETHUSDT")].config.enabled);
    }

    #[test]
    fn save_atomic_roundtrip_preserves_tombstones() {
        let dir =
            std::env::temp_dir().join(format!("levels_catalog_atomic_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("levels.json");
        let catalog = LevelsCatalog {
            levels: vec![template("buy_level_1")],
            instances: vec![],
            tombstones: vec!["buy_level_1_BTCUSDT".into()],
            risk_limits: RiskLimits::default(),
        };
        catalog.save_atomic(&path).unwrap();
        let loaded = LevelsCatalog::load_from_path(&path).unwrap();
        assert_eq!(loaded.tombstones, catalog.tombstones);
        assert_eq!(loaded.levels.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
