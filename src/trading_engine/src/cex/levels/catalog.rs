//! File-canonical levels catalog: templates, pair-specific instances, tombstones.
use super::config::LevelConfig;
use super::identity::{expanded_level_id, LevelIdentity};
use super::level::{Level, LevelState};
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
    #[serde(default)]
    pub venue: Option<Exchange>,
    #[serde(default)]
    pub instrument: Option<InstrumentType>,
    #[serde(default)]
    pub account: Option<String>,
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
    #[serde(default)]
    pub filled_qty: rust_decimal::Decimal,
    #[serde(default)]
    pub state: Option<LevelState>,
    #[serde(default)]
    pub repost_count: usize,
    #[serde(default)]
    pub last_filled_at: Option<u64>,
    #[serde(default)]
    pub fill_count: usize,
    #[serde(default)]
    pub inflight_client_order_id: Option<String>,
    #[serde(default)]
    pub filled_by_order: HashMap<String, rust_decimal::Decimal>,
}

impl LevelsCatalog {
    pub fn load_from_path(path: &Path) -> Result<Self, EngineError> {
        let content = fs::read_to_string(path).map_err(|e| {
            EngineError::CustomError(smol_str::format_smolstr!("Failed to read levels file: {e}"))
        })?;
        serde_json::from_str(&content).map_err(|e| {
            EngineError::CustomError(smol_str::format_smolstr!(
                "Failed to parse levels file: {e}"
            ))
        })
    }

    /// Atomic replace: temp file, fsync, rename.
    pub fn save_atomic(&self, path: &Path) -> Result<(), EngineError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to create levels dir: {e}"
                ))
            })?;
        }
        let tmp = path.with_extension("json.tmp");
        let payload = serde_json::to_vec_pretty(self).map_err(|e| {
            EngineError::CustomError(smol_str::format_smolstr!("Failed to serialize levels: {e}"))
        })?;
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp)
                .map_err(|e| {
                    EngineError::CustomError(smol_str::format_smolstr!(
                        "Failed to open levels temp: {e}"
                    ))
                })?;
            file.write_all(&payload).map_err(|e| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to write levels temp: {e}"
                ))
            })?;
            file.write_all(b"\n").map_err(|e| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to write levels temp: {e}"
                ))
            })?;
            file.sync_all().map_err(|e| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to fsync levels temp: {e}"
                ))
            })?;
        }
        fs::rename(&tmp, path).map_err(|e| {
            EngineError::CustomError(smol_str::format_smolstr!(
                "Failed to replace levels file: {e}"
            ))
        })?;
        Ok(())
    }

    pub fn tombstone_set(&self) -> HashSet<String> {
        self.tombstones.iter().cloned().collect()
    }

    /// Reject a catalog reused under a different venue/account/instrument.
    pub fn ensure_scope(
        &self,
        venue: &Exchange,
        instrument: &InstrumentType,
        account: &str,
    ) -> Result<(), EngineError> {
        if let Some(stored) = &self.venue {
            if stored != venue {
                return Err(EngineError::CustomError(smol_str::format_smolstr!(
                    "levels catalog venue {:?} does not match runtime {:?}",
                    stored,
                    venue
                )));
            }
        }
        if let Some(stored) = &self.instrument {
            if stored != instrument {
                return Err(EngineError::CustomError(smol_str::format_smolstr!(
                    "levels catalog instrument {:?} does not match runtime {:?}",
                    stored,
                    instrument
                )));
            }
        }
        if let Some(stored) = &self.account {
            if stored != account {
                return Err(EngineError::CustomError(smol_str::format_smolstr!(
                    "levels catalog account {stored} does not match runtime {account}"
                )));
            }
        }
        Ok(())
    }

    pub fn materialize(
        &self,
        pairs: &[Pair],
        venue: Exchange,
        instrument: InstrumentType,
        account: &str,
    ) -> Result<HashMap<SmolStr, Level>, EngineError> {
        if pairs.is_empty() {
            return Err(EngineError::CustomError(smol_str::format_smolstr!(
                "No pairs provided - cannot expand levels"
            )));
        }
        let tombstones = self.tombstone_set();
        let mut out: HashMap<SmolStr, Level> = HashMap::new();

        for record in &self.instances {
            let identity = LevelIdentity::new(
                venue.clone(),
                instrument.clone(),
                SmolStr::new(account),
                record.pair.clone(),
                SmolStr::new(&record.template_id),
            );
            if identity.instance_id.as_str() != record.id {
                return Err(EngineError::CustomError(smol_str::format_smolstr!(
                    "instance id {} does not match template {} on {}",
                    record.id,
                    record.template_id,
                    record.pair
                )));
            }
            let mut level = Level::from_identity(record.config.clone(), identity);
            level.owned_client_ids = record.owned_client_ids.iter().map(SmolStr::new).collect();
            if let Some(cid) = &record.inflight_client_order_id {
                let cid = SmolStr::new(cid);
                if !level.owned_client_ids.iter().any(|owned| owned == &cid) {
                    level.owned_client_ids.push(cid.clone());
                }
                level.inflight_client_order_id = Some(cid);
            }
            level.filled_qty = record.filled_qty;
            level.fill_count = record.fill_count;
            level.filled_by_order = record
                .filled_by_order
                .iter()
                .map(|(id, qty)| (SmolStr::new(id), *qty))
                .collect();
            level.repost_count = record.repost_count;
            level.last_filled_at = record.last_filled_at;
            level.active_orders = record
                .owned_exchange_ids
                .iter()
                .map(|id| trading_core::types::common::OrderId::new(SmolStr::new(id)))
                .collect();
            match record.state.clone() {
                Some(state) => level.state = state,
                None if !level.active_orders.is_empty() => {
                    level.state = LevelState::Posted;
                }
                None => {}
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
            ..Default::default()
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
                filled_qty: rust_decimal::Decimal::ZERO,
                state: None,
                repost_count: 0,
                last_filled_at: None,
                fill_count: 0,
                inflight_client_order_id: None,
                filled_by_order: HashMap::new(),
            }],
            tombstones: vec![],
            risk_limits: RiskLimits::default(),
            ..Default::default()
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
            ..Default::default()
        };
        catalog.save_atomic(&path).unwrap();
        let loaded = LevelsCatalog::load_from_path(&path).unwrap();
        assert_eq!(loaded.tombstones, catalog.tombstones);
        assert_eq!(loaded.levels.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reused_catalog_rejects_scope_mismatch() {
        let catalog = LevelsCatalog {
            venue: Some(Exchange::Paper),
            instrument: Some(InstrumentType::Spot),
            account: Some("acct-a".into()),
            ..Default::default()
        };
        catalog
            .ensure_scope(&Exchange::Paper, &InstrumentType::Spot, "acct-a")
            .unwrap();
        let err = catalog
            .ensure_scope(&Exchange::Bybit, &InstrumentType::Spot, "acct-a")
            .unwrap_err();
        assert!(err.to_string().contains("venue"));
        let err = catalog
            .ensure_scope(&Exchange::Paper, &InstrumentType::Futures, "acct-a")
            .unwrap_err();
        assert!(err.to_string().contains("instrument"));
        let err = catalog
            .ensure_scope(&Exchange::Paper, &InstrumentType::Spot, "acct-b")
            .unwrap_err();
        assert!(err.to_string().contains("account"));
    }

    #[test]
    fn pending_delete_instance_loads_despite_stale_tombstone() {
        let btc = Pair::new("BTC", "USDT");
        let mut cfg = template("buy_level_1");
        cfg.enabled = false;
        let catalog = LevelsCatalog {
            levels: vec![template("buy_level_1")],
            instances: vec![LevelInstanceRecord {
                id: "buy_level_1_BTCUSDT".into(),
                template_id: "buy_level_1".into(),
                pair: btc.clone(),
                config: cfg,
                owned_client_ids: vec!["cid-1".into()],
                owned_exchange_ids: vec!["ex-1".into()],
                filled_qty: rust_decimal::Decimal::ZERO,
                state: Some(LevelState::Deleting),
                repost_count: 0,
                last_filled_at: None,
                fill_count: 0,
                inflight_client_order_id: None,
                filled_by_order: HashMap::new(),
            }],
            tombstones: vec!["buy_level_1_BTCUSDT".into()],
            risk_limits: RiskLimits::default(),
            ..Default::default()
        };
        let levels = catalog
            .materialize(&[btc], Exchange::Paper, InstrumentType::Spot, "test")
            .unwrap();
        let level = &levels[&SmolStr::new("buy_level_1_BTCUSDT")];
        assert_eq!(level.state, LevelState::Deleting);
        assert_eq!(level.active_orders[0].as_str(), "ex-1");
    }

    #[test]
    fn materialize_restores_repost_lifecycle_fields() {
        let btc = Pair::new("BTC", "USDT");
        let catalog = LevelsCatalog {
            levels: vec![template("buy_level_1")],
            instances: vec![LevelInstanceRecord {
                id: "buy_level_1_BTCUSDT".into(),
                template_id: "buy_level_1".into(),
                pair: btc.clone(),
                config: template("buy_level_1"),
                owned_client_ids: vec![],
                owned_exchange_ids: vec![],
                filled_qty: rust_decimal::Decimal::ZERO,
                state: Some(LevelState::Filled),
                repost_count: 3,
                last_filled_at: Some(1_700_000_000_000),
                fill_count: 3,
                inflight_client_order_id: None,
                filled_by_order: HashMap::new(),
            }],
            tombstones: vec![],
            risk_limits: RiskLimits::default(),
            ..Default::default()
        };
        let levels = catalog
            .materialize(&[btc], Exchange::Paper, InstrumentType::Spot, "test")
            .unwrap();
        let level = &levels[&SmolStr::new("buy_level_1_BTCUSDT")];
        assert_eq!(level.state, LevelState::Filled);
        assert_eq!(level.repost_count, 3);
        assert_eq!(level.last_filled_at, Some(1_700_000_000_000));
        assert_eq!(level.fill_count, 3);
    }
}
