//! Level manager - manages level lifecycle and state transitions
use super::catalog::{LevelInstanceRecord, LevelsCatalog};
use super::identity::LevelIdentity;
use super::{Level, LevelConfig, LevelState};
use crate::risk::RiskLimits;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use trading_core::error::EngineError;
use trading_core::types::common::{Exchange, InstrumentType, OrderId, Pair};

/// Manages level lifecycle and state transitions
pub struct LevelManager {
    levels: HashMap<SmolStr, Level>,
    templates: Vec<LevelConfig>,
    tombstones: HashSet<String>,
    risk_limits: RiskLimits,
    config_path: Option<PathBuf>,
    persistence: Option<Arc<super::persistence::LevelPersistence>>,
    venue: Exchange,
    instrument: InstrumentType,
    account: SmolStr,
    external_order_ids: HashSet<OrderId>,
}

impl LevelManager {
    /// Create a new level manager
    pub fn new() -> Self {
        Self {
            levels: HashMap::new(),
            templates: Vec::new(),
            tombstones: HashSet::new(),
            risk_limits: RiskLimits::default(),
            config_path: None,
            persistence: None,
            venue: Exchange::Paper,
            instrument: InstrumentType::Spot,
            account: SmolStr::new("test"),
            external_order_ids: HashSet::new(),
        }
    }

    pub fn with_identity_scope(
        mut self,
        venue: Exchange,
        instrument: InstrumentType,
        account: impl Into<SmolStr>,
    ) -> Self {
        self.venue = venue;
        self.instrument = instrument;
        self.account = account.into();
        self
    }

    /// Set persistence for level manager
    pub fn with_persistence(
        mut self,
        persistence: Arc<super::persistence::LevelPersistence>,
    ) -> Self {
        self.persistence = Some(persistence);
        self
    }

    fn identity_for(&self, template_id: impl Into<SmolStr>, pair: Pair) -> LevelIdentity {
        LevelIdentity::new(
            self.venue.clone(),
            self.instrument.clone(),
            self.account.clone(),
            pair,
            template_id,
        )
    }

    fn catalog_snapshot(&self) -> LevelsCatalog {
        let instances = self
            .levels
            .iter()
            .map(|(id, level)| LevelInstanceRecord {
                id: id.to_string(),
                template_id: level.identity.template_id.to_string(),
                pair: level.pair.clone(),
                config: level.config.clone(),
                owned_client_ids: level
                    .owned_client_ids
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                owned_exchange_ids: level
                    .active_orders
                    .iter()
                    .map(|oid| oid.as_str().to_string())
                    .collect(),
                filled_qty: level.filled_qty,
                state: Some(level.state.clone()),
                repost_count: level.repost_count,
                last_filled_at: level.last_filled_at,
                fill_count: level.fill_count,
                inflight_client_order_id: level
                    .inflight_client_order_id
                    .as_ref()
                    .map(|id| id.to_string()),
                filled_by_order: level
                    .filled_by_order
                    .iter()
                    .map(|(id, qty)| (id.to_string(), *qty))
                    .collect(),
            })
            .collect();
        let mut tombstones: Vec<String> = self.tombstones.iter().cloned().collect();
        tombstones.sort();
        LevelsCatalog {
            levels: self.templates.clone(),
            instances,
            tombstones,
            risk_limits: self.risk_limits.clone(),
            venue: Some(self.venue.clone()),
            instrument: Some(self.instrument.clone()),
            account: Some(self.account.to_string()),
        }
    }

    fn persist_canonical(&self) -> Result<(), EngineError> {
        let catalog = self.catalog_snapshot();
        if let Some(path) = &self.config_path {
            catalog.save_atomic(path)?;
        }
        Ok(())
    }

    fn mirror_mongo(&self, configured_pairs: &[Pair]) {
        if self.persistence.is_none() {
            return;
        }
        let catalog = self.catalog_snapshot();
        let persistence = self.persistence.clone();
        let pairs = configured_pairs.to_vec();
        tokio::spawn(async move {
            let Some(persistence) = persistence else {
                return;
            };
            if let Err(e) = persistence
                .save_levels(&catalog.levels, &catalog.risk_limits, &pairs)
                .await
            {
                tracing::warn!("Mongo mirror of levels catalog failed (file is canonical): {e}");
            }
        });
    }

    /// Load levels from configuration file
    /// Expands pair-agnostic level configs to create Level instances for each pair
    pub async fn load_from_config(
        &mut self,
        path: &Path,
        pairs: &[Pair],
    ) -> Result<(), EngineError> {
        self.load_from_config_with_risk_limits(path, pairs)
            .await
            .map(|_| ())
    }

    /// Load levels and risk limits from the file catalog.
    /// Mongo is never used as the restart source of truth.
    pub async fn load_from_config_with_risk_limits(
        &mut self,
        path: &Path,
        pairs: &[Pair],
    ) -> Result<RiskLimits, EngineError> {
        let catalog = LevelsCatalog::load_from_path(path)?;
        catalog.ensure_scope(&self.venue, &self.instrument, self.account.as_str())?;
        self.templates = catalog.levels.clone();
        self.tombstones = catalog.tombstone_set();
        self.risk_limits = catalog.risk_limits.clone();
        self.levels = catalog.materialize(
            pairs,
            self.venue.clone(),
            self.instrument.clone(),
            self.account.as_str(),
        )?;
        self.tombstones
            .retain(|id| !self.levels.contains_key(id.as_str()));
        self.config_path = Some(path.to_path_buf());
        Ok(self.risk_limits.clone())
    }

    /// Add a new level for a specific pair. Returns the expanded instance id.
    pub async fn add_level(
        &mut self,
        config: LevelConfig,
        pair: Pair,
    ) -> Result<SmolStr, EngineError> {
        let identity = self.identity_for(config.id.clone(), pair.clone());
        let level_id = identity.instance_id.clone();

        if self.levels.contains_key(&level_id) {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Level with ID {} already exists for pair {}",
                config.id,
                pair.convert_to_smolstr()
            )));
        }

        let cleared_tombstone = self.tombstones.remove(level_id.as_str());
        let level = Level::from_identity(config, identity);
        self.levels.insert(level_id.clone(), level);
        if let Err(err) = self.persist_canonical() {
            self.levels.remove(&level_id);
            if cleared_tombstone {
                self.tombstones.insert(level_id.to_string());
            }
            return Err(err);
        }
        self.mirror_mongo(&[pair]);
        Ok(level_id)
    }

    /// Remove a level instance. Tombstone so a template cannot recreate it on restart.
    pub async fn remove_level(&mut self, level_id: &SmolStr) -> Result<(), EngineError> {
        if self.tombstones.contains(level_id.as_str()) && !self.levels.contains_key(level_id) {
            return Ok(());
        }
        let Some(mut level) = self.levels.remove(level_id) else {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {} not found",
                level_id
            )));
        };
        level.generation = level.generation.saturating_add(1);
        self.tombstones.insert(level_id.to_string());
        if let Err(err) = self.persist_canonical() {
            self.tombstones.remove(level_id.as_str());
            self.levels.insert(level_id.clone(), level);
            return Err(err);
        }
        self.mirror_mongo(&[level.pair.clone()]);
        Ok(())
    }

    /// Enable a level
    pub async fn enable_level(&mut self, level_id: &SmolStr) -> Result<(), EngineError> {
        let (pair, prev_enabled, prev_state) = {
            let level = self.levels.get_mut(level_id).ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
            })?;
            let prev_enabled = level.config.enabled;
            let prev_state = level.state.clone();
            if matches!(level.state, LevelState::Deleting) {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "Level {level_id} is deleting"
                )));
            }
            level.config.enabled = true;
            if matches!(level.state, LevelState::Paused) {
                level.state = LevelState::Created;
            }
            (level.pair.clone(), prev_enabled, prev_state)
        };
        if let Err(err) = self.persist_canonical() {
            if let Some(level) = self.levels.get_mut(level_id) {
                level.config.enabled = prev_enabled;
                level.state = prev_state;
            }
            return Err(err);
        }
        self.mirror_mongo(&[pair]);
        Ok(())
    }

    /// Disable/pause a level
    pub async fn disable_level(&mut self, level_id: &SmolStr) -> Result<(), EngineError> {
        let (pair, prev_enabled, prev_state, prev_generation) = {
            let level = self.levels.get_mut(level_id).ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
            })?;
            let prev_enabled = level.config.enabled;
            let prev_state = level.state.clone();
            let prev_generation = level.generation;
            if matches!(level.state, LevelState::Deleting) {
                return Ok(());
            }
            level.config.enabled = false;
            level.state = LevelState::Paused;
            level.generation = level.generation.saturating_add(1);
            (
                level.pair.clone(),
                prev_enabled,
                prev_state,
                prev_generation,
            )
        };
        if let Err(err) = self.persist_canonical() {
            if let Some(level) = self.levels.get_mut(level_id) {
                level.config.enabled = prev_enabled;
                level.state = prev_state;
                level.generation = prev_generation;
            }
            return Err(err);
        }
        self.mirror_mongo(&[pair]);
        Ok(())
    }

    /// Update level configuration and persist
    pub async fn update_level_config(
        &mut self,
        level_id: &SmolStr,
        config: LevelConfig,
    ) -> Result<(), EngineError> {
        let (pair, prev_config) = {
            let level = self.levels.get_mut(level_id).ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
            })?;
            let prev_config = std::mem::replace(&mut level.config, config);
            (level.pair.clone(), prev_config)
        };
        if let Err(err) = self.persist_canonical() {
            if let Some(level) = self.levels.get_mut(level_id) {
                level.config = prev_config;
            }
            return Err(err);
        }
        self.mirror_mongo(&[pair]);
        Ok(())
    }

    /// Latch submitting before persist-before-send. Returns the generation to recheck.
    pub fn begin_submit(&mut self, level_id: &SmolStr) -> Result<u64, EngineError> {
        let level = self.levels.get_mut(level_id).ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
        })?;
        if !level.config.enabled || matches!(level.state, LevelState::Deleting) {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {level_id} is disabled"
            )));
        }
        if !matches!(level.state, LevelState::Created) || !level.active_orders.is_empty() {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {level_id} is not ready to submit"
            )));
        }
        level.state = LevelState::Submitting;
        Ok(level.generation)
    }

    pub fn submit_still_allowed(&self, level_id: &SmolStr, generation: u64) -> bool {
        self.levels.get(level_id).is_some_and(|level| {
            level.config.enabled
                && level.generation == generation
                && matches!(level.state, LevelState::Submitting)
                && !matches!(level.state, LevelState::Paused | LevelState::Removed)
        })
    }

    pub fn abort_submit(&mut self, level_id: &SmolStr, generation: u64) {
        if let Some(level) = self.levels.get_mut(level_id) {
            if level.generation == generation && matches!(level.state, LevelState::Submitting) {
                level.state = LevelState::Created;
            }
        }
    }

    pub fn record_inflight_client_id(
        &mut self,
        level_id: &SmolStr,
        client_order_id: &SmolStr,
    ) -> Result<(), EngineError> {
        let (pair, prev_inflight, prev_owned) = {
            let level = self.levels.get_mut(level_id).ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
            })?;
            let prev_inflight = level.inflight_client_order_id.clone();
            let prev_owned = level.owned_client_ids.clone();
            level.inflight_client_order_id = Some(client_order_id.clone());
            if !level
                .owned_client_ids
                .iter()
                .any(|id| id == client_order_id)
            {
                level.owned_client_ids.push(client_order_id.clone());
            }
            (level.pair.clone(), prev_inflight, prev_owned)
        };
        if let Err(err) = self.persist_canonical() {
            if let Some(level) = self.levels.get_mut(level_id) {
                level.inflight_client_order_id = prev_inflight;
                level.owned_client_ids = prev_owned;
            }
            return Err(err);
        }
        self.mirror_mongo(&[pair]);
        Ok(())
    }

    pub fn clear_inflight_client_id(&mut self, level_id: &SmolStr) -> Result<(), EngineError> {
        let Some(level) = self.levels.get_mut(level_id) else {
            return Ok(());
        };
        if level.inflight_client_order_id.is_none() {
            return Ok(());
        }
        let prev = level.inflight_client_order_id.take();
        let pair = level.pair.clone();
        if let Err(err) = self.persist_canonical() {
            if let Some(level) = self.levels.get_mut(level_id) {
                level.inflight_client_order_id = prev;
            }
            return Err(err);
        }
        self.mirror_mongo(&[pair]);
        Ok(())
    }

    pub fn persist_runtime_state(&self) -> Result<(), EngineError> {
        self.persist_canonical()
    }

    pub fn confirm_posted(
        &mut self,
        level_id: &SmolStr,
        client_order_id: &SmolStr,
        order_id: &OrderId,
    ) -> Result<(), EngineError> {
        self.attach_owned_order(level_id, client_order_id, order_id, true)
    }

    /// Record durable ownership. `keep_posted` is false when disable won the send race.
    pub fn attach_owned_order(
        &mut self,
        level_id: &SmolStr,
        client_order_id: &SmolStr,
        order_id: &OrderId,
        keep_posted: bool,
    ) -> Result<(), EngineError> {
        let pair = {
            let level = self.levels.get_mut(level_id).ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
            })?;
            if !level
                .owned_client_ids
                .iter()
                .any(|id| id == client_order_id)
            {
                level.owned_client_ids.push(client_order_id.clone());
            }
            if !level.active_orders.iter().any(|id| id == order_id) {
                level.active_orders.push(order_id.clone());
            }
            level.inflight_client_order_id = None;
            if keep_posted {
                level.state = LevelState::Posted;
                level.last_posted_at = Some(trading_core::utils::current_timestamp());
            }
            level.pair.clone()
        };
        self.persist_canonical()?;
        self.mirror_mongo(&[pair]);
        Ok(())
    }

    pub fn mark_external_order(&mut self, order_id: OrderId) {
        self.external_order_ids.insert(order_id);
    }

    pub fn is_external_order(&self, order_id: &OrderId) -> bool {
        self.external_order_ids.contains(order_id)
    }

    pub fn owns_client_id(&self, client_order_id: &str) -> Option<SmolStr> {
        self.levels.iter().find_map(|(id, level)| {
            level
                .owned_client_ids
                .iter()
                .any(|owned| owned.as_str() == client_order_id)
                .then(|| id.clone())
        })
    }

    pub fn owns_exchange_id(&self, order_id: &OrderId) -> Option<SmolStr> {
        self.levels.iter().find_map(|(id, level)| {
            level
                .active_orders
                .iter()
                .any(|owned| owned == order_id)
                .then(|| id.clone())
        })
    }

    /// Get level by ID
    pub fn get_level(&self, level_id: &SmolStr) -> Option<&Level> {
        self.levels.get(level_id)
    }

    /// Get mutable level by ID
    pub fn get_level_mut(&mut self, level_id: &SmolStr) -> Option<&mut Level> {
        self.levels.get_mut(level_id)
    }

    /// Get all levels
    pub fn get_all_levels(&self) -> Vec<&Level> {
        self.levels.values().collect()
    }

    /// Get levels ready to post (Created state, enabled)
    pub fn get_levels_ready_to_post(&self) -> Vec<&Level> {
        self.levels
            .values()
            .filter(|level| level.is_ready_to_post())
            .collect()
    }

    /// Update level state
    pub fn update_level_state(
        &mut self,
        level_id: &SmolStr,
        state: LevelState,
    ) -> Result<(), EngineError> {
        if let Some(level) = self.levels.get_mut(level_id) {
            level.state = state;
            Ok(())
        } else {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {} not found",
                level_id
            )))
        }
    }

    /// Handle order filled for a level
    pub async fn handle_order_filled(
        &mut self,
        level_id: &SmolStr,
        order_id: &OrderId,
        filled_qty: rust_decimal::Decimal,
    ) -> Result<(), EngineError> {
        let pair = {
            let level = self.levels.get_mut(level_id).ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
            })?;
            let delta = level.apply_reported_fill(order_id, filled_qty, true);
            if delta > rust_decimal::Decimal::ZERO {
                level.last_filled_at = Some(trading_core::utils::current_timestamp());
            }
            if level.config.enabled
                && !matches!(level.state, LevelState::Paused | LevelState::Deleting)
            {
                level.state = LevelState::Filled;
            }
            level.pair.clone()
        };
        self.persist_canonical()?;
        self.mirror_mongo(&[pair]);
        Ok(())
    }

    /// Handle order cancelled for a level
    pub async fn handle_order_cancelled(
        &mut self,
        level_id: &SmolStr,
        order_id: &OrderId,
        filled_qty: rust_decimal::Decimal,
    ) -> Result<(), EngineError> {
        let pair = {
            let level = self.levels.get_mut(level_id).ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
            })?;
            level.apply_reported_fill(order_id, filled_qty, false);
            if level.config.enabled
                && !matches!(level.state, LevelState::Paused | LevelState::Deleting)
            {
                level.state = LevelState::Cancelled;
            }
            level.pair.clone()
        };
        self.persist_canonical()?;
        self.mirror_mongo(&[pair]);
        Ok(())
    }

    /// Check if level should repost
    pub fn should_repost(&self, level_id: &SmolStr) -> Result<bool, EngineError> {
        let level = self.levels.get(level_id).ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
        })?;

        if !level.config.enabled
            || matches!(
                level.state,
                LevelState::Paused | LevelState::Deleting | LevelState::Removed
            )
        {
            return Ok(false);
        }
        if let Some(max_reposts) = level.config.lifecycle.max_reposts {
            if level.repost_count >= max_reposts {
                return Ok(false);
            }
        }
        if let Some(delay_secs) = level.config.lifecycle.repost_delay_secs {
            if let Some(last_filled) = level.last_filled_at {
                let now = trading_core::utils::current_timestamp();
                if now.saturating_sub(last_filled) < delay_secs.saturating_mul(1000) {
                    return Ok(false);
                }
            }
        }
        if level
            .config
            .lifecycle
            .repost_conditions
            .iter()
            .any(|c| matches!(c, super::config::RepostCondition::PositionLimitReached))
        {
            if let Some(max_position) = level
                .config
                .risk
                .max_position
                .filter(|v| *v > rust_decimal::Decimal::ZERO)
            {
                if level.filled_qty >= max_position {
                    return Ok(false);
                }
            }
        }

        match level.state {
            LevelState::Filled => Ok(level.config.lifecycle.auto_repost),
            LevelState::Cancelled => Ok(level.config.lifecycle.auto_repost_on_cancel),
            _ => Ok(false),
        }
    }

    /// Latch delete before any cancel. Keeps the instance until REST verifies owned IDs are gone.
    /// Tombstone is written only in `complete_delete` after verification.
    pub fn begin_delete(&mut self, level_id: &SmolStr) -> Result<Vec<OrderId>, EngineError> {
        if self.tombstones.contains(level_id.as_str()) && !self.levels.contains_key(level_id) {
            return Ok(Vec::new());
        }
        if self
            .levels
            .get(level_id)
            .is_some_and(|level| matches!(level.state, LevelState::Deleting))
        {
            return Ok(self
                .levels
                .get(level_id)
                .map(|level| level.active_orders.clone())
                .unwrap_or_default());
        }
        let (owned, prev_enabled, prev_state, prev_generation) = {
            let level = self.levels.get_mut(level_id).ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
            })?;
            let prev_enabled = level.config.enabled;
            let prev_state = level.state.clone();
            let prev_generation = level.generation;
            level.config.enabled = false;
            level.state = LevelState::Deleting;
            level.generation = level.generation.saturating_add(1);
            (
                level.active_orders.clone(),
                prev_enabled,
                prev_state,
                prev_generation,
            )
        };
        if let Err(err) = self.persist_canonical() {
            if let Some(level) = self.levels.get_mut(level_id) {
                level.config.enabled = prev_enabled;
                level.state = prev_state;
                level.generation = prev_generation;
            }
            return Err(err);
        }
        Ok(owned)
    }

    pub fn complete_delete(&mut self, level_id: &SmolStr) -> Result<(), EngineError> {
        if self.tombstones.contains(level_id.as_str()) && !self.levels.contains_key(level_id) {
            return Ok(());
        }
        if let Some(level) = self.levels.get(level_id) {
            if level.inflight_client_order_id.is_some() {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "delete latched; pending submission unresolved"
                )));
            }
            if !level.active_orders.is_empty() {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "delete latched; owned orders not REST-verified gone"
                )));
            }
        }
        let Some(level) = self.levels.remove(level_id) else {
            self.tombstones.insert(level_id.to_string());
            self.persist_canonical()?;
            return Ok(());
        };
        self.tombstones.insert(level_id.to_string());
        if let Err(err) = self.persist_canonical() {
            self.levels.insert(level_id.clone(), level);
            self.tombstones.remove(level_id.as_str());
            return Err(err);
        }
        Ok(())
    }

    /// REST proof that these IDs are gone: drop them from `active_orders` without waiting for WS.
    pub fn retire_verified_order_ids(
        &mut self,
        level_id: &SmolStr,
        order_ids: &[OrderId],
    ) -> Result<(), EngineError> {
        let pair = {
            let level = self.levels.get_mut(level_id).ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
            })?;
            for order_id in order_ids {
                level.active_orders.retain(|id| id != order_id);
            }
            level.pair.clone()
        };
        self.persist_canonical()?;
        self.mirror_mongo(&[pair]);
        Ok(())
    }

    /// Drop owned IDs on Deleting levels that REST no longer shows as working.
    pub fn retire_absent_owned_orders(
        &mut self,
        rest_working: &std::collections::HashSet<OrderId>,
    ) -> Result<(), EngineError> {
        let mut changed = false;
        let mut pairs = Vec::new();
        for level in self.levels.values_mut() {
            if !matches!(level.state, LevelState::Deleting) {
                continue;
            }
            let before = level.active_orders.len();
            level.active_orders.retain(|id| rest_working.contains(id));
            if level.active_orders.len() != before {
                changed = true;
                pairs.push(level.pair.clone());
            }
        }
        if changed {
            self.persist_canonical()?;
            for pair in pairs {
                self.mirror_mongo(&[pair]);
            }
        }
        Ok(())
    }

    pub fn complete_ready_deletes(&mut self) -> Result<(), EngineError> {
        let ready: Vec<SmolStr> = self
            .levels
            .iter()
            .filter(|(_, level)| {
                matches!(level.state, LevelState::Deleting)
                    && level.inflight_client_order_id.is_none()
                    && level.active_orders.is_empty()
            })
            .map(|(id, _)| id.clone())
            .collect();
        for level_id in ready {
            self.complete_delete(&level_id)?;
        }
        Ok(())
    }

    pub fn owned_order_ids(&self) -> Vec<OrderId> {
        self.levels
            .values()
            .flat_map(|level| level.active_orders.iter().cloned())
            .collect()
    }

    /// Save all levels and risk limits to MongoDB (mirror only).
    pub async fn save_all_levels(
        &self,
        risk_limits: &RiskLimits,
        configured_pairs: &[Pair],
    ) -> Result<(), EngineError> {
        if let Some(persistence) = &self.persistence {
            persistence
                .save_levels(&self.templates, risk_limits, configured_pairs)
                .await
                .map_err(|e| {
                    EngineError::CustomError(anyhow::anyhow!(
                        "Failed to save levels to MongoDB: {}",
                        e
                    ))
                })?;
        }
        Ok(())
    }

    /// Mongo load is not used at restart. File catalog is canonical.
    pub async fn load_from_mongo(
        &mut self,
        _pairs: &[Pair],
    ) -> Result<Option<RiskLimits>, EngineError> {
        tracing::warn!(
            "load_from_mongo is not applied at restart; levels file is the canonical source"
        );
        Ok(None)
    }
}

impl Default for LevelManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cex::levels::config::{
        LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
    };
    use rust_decimal_macros::dec;
    use smol_str::SmolStr;
    use trading_core::types::common::{OrderId, Pair, Side};

    fn create_test_level_config(id: &str) -> LevelConfig {
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

    #[tokio::test]
    async fn test_level_manager_new() {
        let manager = LevelManager::new();
        assert_eq!(manager.get_all_levels().len(), 0);
    }

    #[tokio::test]
    async fn test_level_manager_add_level() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");

        let pair = Pair::new("BTC", "USDT");
        let instance_id = manager.add_level(config.clone(), pair).await.unwrap();
        assert_eq!(instance_id.as_str(), "test_level_BTCUSDT");

        let level_id = SmolStr::new("test_level_BTCUSDT");
        let level = manager.get_level(&level_id);
        assert!(level.is_some());
        assert_eq!(level.unwrap().config.id, SmolStr::new("test_level"));
    }

    #[tokio::test]
    async fn test_level_manager_add_duplicate_level() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");

        let pair = Pair::new("BTC", "USDT");
        manager
            .add_level(config.clone(), pair.clone())
            .await
            .unwrap();
        let result = manager.add_level(config, pair).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_level_manager_remove_level() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");
        let pair = Pair::new("BTC", "USDT");

        manager.add_level(config, pair).await.unwrap();
        let result = manager
            .remove_level(&SmolStr::new("test_level_BTCUSDT"))
            .await;

        assert!(result.is_ok());
        assert!(manager
            .get_level(&SmolStr::new("test_level_BTCUSDT"))
            .is_none());
        let again = manager
            .remove_level(&SmolStr::new("test_level_BTCUSDT"))
            .await;
        assert!(again.is_ok());
    }

    #[tokio::test]
    async fn test_level_manager_remove_nonexistent_level() {
        let mut manager = LevelManager::new();
        let result = manager.remove_level(&SmolStr::new("nonexistent")).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_level_manager_enable_level() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");
        let pair = Pair::new("BTC", "USDT");
        let level_id = SmolStr::new("test_level_BTCUSDT");

        manager.add_level(config, pair).await.unwrap();

        // Disable first
        manager.disable_level(&level_id).await.unwrap();
        let level = manager.get_level(&level_id).unwrap();
        assert!(!level.config.enabled);

        // Then enable
        manager.enable_level(&level_id).await.unwrap();
        let level = manager.get_level(&level_id).unwrap();
        assert!(level.config.enabled);
    }

    #[tokio::test]
    async fn test_level_manager_disable_level() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");
        let pair = Pair::new("BTC", "USDT");
        let level_id = SmolStr::new("test_level_BTCUSDT");

        manager.add_level(config, pair).await.unwrap();
        manager.disable_level(&level_id).await.unwrap();

        let level = manager.get_level(&level_id).unwrap();
        assert!(!level.config.enabled);
        assert_eq!(level.state, LevelState::Paused);
    }

    #[tokio::test]
    async fn test_level_manager_get_levels_ready_to_post() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");
        let pair = Pair::new("BTC", "USDT");

        manager.add_level(config, pair).await.unwrap();

        let ready = manager.get_levels_ready_to_post();
        assert_eq!(ready.len(), 1);
    }

    #[tokio::test]
    async fn test_level_manager_update_level_state() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");
        let pair = Pair::new("BTC", "USDT");
        let level_id = SmolStr::new("test_level_BTCUSDT");

        manager.add_level(config, pair).await.unwrap();
        manager
            .update_level_state(&level_id, LevelState::Posted)
            .unwrap();

        let level = manager.get_level(&level_id).unwrap();
        assert_eq!(level.state, LevelState::Posted);
    }

    #[tokio::test]
    async fn test_level_manager_handle_order_filled() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");
        let pair = Pair::new("BTC", "USDT");
        let level_id = SmolStr::new("test_level_BTCUSDT");

        manager.add_level(config, pair).await.unwrap();

        let order_id = OrderId::new("order123".into());
        manager
            .handle_order_filled(&level_id, &order_id, dec!(0.001))
            .await
            .unwrap();

        let level = manager.get_level(&level_id).unwrap();
        assert_eq!(level.state, LevelState::Filled);
        assert!(level.last_filled_at.is_some());
    }

    #[tokio::test]
    async fn test_level_manager_handle_order_cancelled() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");
        let pair = Pair::new("BTC", "USDT");
        let level_id = SmolStr::new("test_level_BTCUSDT");

        manager.add_level(config, pair).await.unwrap();

        let order_id = OrderId::new("order123".into());
        manager
            .handle_order_cancelled(&level_id, &order_id, dec!(0))
            .await
            .unwrap();

        let level = manager.get_level(&level_id).unwrap();
        assert_eq!(level.state, LevelState::Cancelled);
    }

    #[tokio::test]
    async fn test_level_manager_should_repost() {
        let mut manager = LevelManager::new();
        let mut config = create_test_level_config("test_level");
        config.lifecycle.auto_repost = true;
        config.lifecycle.auto_repost_on_cancel = false;

        let pair = Pair::new("BTC", "USDT");
        let level_id = SmolStr::new("test_level_BTCUSDT");
        manager.add_level(config, pair).await.unwrap();

        // Set to filled state
        manager
            .update_level_state(&level_id, LevelState::Filled)
            .unwrap();
        assert!(manager.should_repost(&level_id).unwrap());

        // Set to cancelled state
        manager
            .update_level_state(&level_id, LevelState::Cancelled)
            .unwrap();
        assert!(!manager.should_repost(&level_id).unwrap());
    }

    #[tokio::test]
    async fn test_level_manager_update_level_config() {
        let mut manager = LevelManager::new();
        let config = create_test_level_config("test_level");

        let pair = Pair::new("BTC", "USDT");
        let level_id = SmolStr::new("test_level_BTCUSDT");
        manager.add_level(config.clone(), pair).await.unwrap();

        let mut new_config = config;
        new_config.enabled = false;
        manager
            .update_level_config(&level_id, new_config)
            .await
            .unwrap();

        let level = manager.get_level(&level_id).unwrap();
        assert!(!level.config.enabled);
    }

    fn unique_temp_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "levels_gate1_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp levels dir");
        dir
    }

    async fn reload(path: &Path, pairs: &[Pair], venue: Exchange, account: &str) -> LevelManager {
        let mut manager = LevelManager::new().with_identity_scope(
            venue,
            InstrumentType::Spot,
            SmolStr::new(account),
        );
        manager.load_from_config(path, pairs).await.unwrap();
        manager
    }

    #[tokio::test]
    async fn create_edit_disable_delete_survive_restart_and_multi_pair_template() {
        let dir = unique_temp_dir();
        let path = dir.join("levels.json");
        let btc = Pair::new("BTC", "USDT");
        let eth = Pair::new("ETH", "USDT");
        let pairs = [btc.clone(), eth.clone()];

        LevelsCatalog {
            levels: vec![create_test_level_config("buy_level_1")],
            instances: vec![],
            tombstones: vec![],
            risk_limits: RiskLimits::default(),
            ..Default::default()
        }
        .save_atomic(&path)
        .unwrap();

        let mut manager = reload(&path, &pairs, Exchange::Paper, "acct-a").await;
        assert_eq!(manager.get_all_levels().len(), 2);
        let btc_id = SmolStr::new("buy_level_1_BTCUSDT");
        let eth_id = SmolStr::new("buy_level_1_ETHUSDT");
        assert_eq!(
            manager
                .get_level(&btc_id)
                .unwrap()
                .identity
                .account
                .as_str(),
            "acct-a"
        );
        assert_eq!(
            manager.get_level(&btc_id).unwrap().identity.instance_id,
            btc_id
        );

        let created = manager
            .add_level(create_test_level_config("extra_level"), btc.clone())
            .await
            .unwrap();
        assert_eq!(created.as_str(), "extra_level_BTCUSDT");

        let mut edited = create_test_level_config("buy_level_1");
        edited.name = Some(SmolStr::new("eth only"));
        manager.update_level_config(&eth_id, edited).await.unwrap();
        manager.disable_level(&eth_id).await.unwrap();
        manager.remove_level(&btc_id).await.unwrap();

        let manager = reload(&path, &pairs, Exchange::Paper, "acct-a").await;
        assert!(manager.get_level(&btc_id).is_none());
        let eth_level = manager.get_level(&eth_id).unwrap();
        assert!(!eth_level.config.enabled);
        assert!(!eth_level.is_ready_to_post());
        assert_eq!(eth_level.config.name.as_deref(), Some("eth only"));
        let extra = manager
            .get_level(&SmolStr::new("extra_level_BTCUSDT"))
            .unwrap();
        assert_eq!(extra.config.id.as_str(), "extra_level");
        let mut mongo_probe = reload(&path, &pairs, Exchange::Paper, "acct-a").await;
        assert!(mongo_probe.load_from_mongo(&pairs).await.unwrap().is_none());
        assert!(mongo_probe.get_level(&btc_id).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn persist_failure_is_reported_and_rolls_back_create() {
        let dir = unique_temp_dir();
        let blocker = dir.join("not_a_directory");
        std::fs::write(&blocker, b"x").unwrap();
        let mut manager = LevelManager::new();
        manager.config_path = Some(blocker.join("levels.json"));
        let err = manager
            .add_level(
                create_test_level_config("buy_level_1"),
                Pair::new("BTC", "USDT"),
            )
            .await;
        assert!(err.is_err());
        assert!(manager.get_all_levels().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn catalog_scope_mismatch_is_rejected_on_load() {
        let dir = unique_temp_dir();
        let path = dir.join("levels.json");
        LevelsCatalog {
            levels: vec![create_test_level_config("buy_level_1")],
            venue: Some(Exchange::Paper),
            instrument: Some(InstrumentType::Spot),
            account: Some("acct-a".into()),
            ..Default::default()
        }
        .save_atomic(&path)
        .unwrap();
        let mut manager = LevelManager::new().with_identity_scope(
            Exchange::Bybit,
            InstrumentType::Spot,
            SmolStr::new("acct-a"),
        );
        let err = manager
            .load_from_config(&path, &[Pair::new("BTC", "USDT")])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("venue"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn restart_after_terminal_order_does_not_restore_posted() {
        let dir = unique_temp_dir();
        let path = dir.join("levels.json");
        let pair = Pair::new("BTC", "USDT");
        let mut manager = LevelManager::new().with_identity_scope(
            Exchange::Paper,
            InstrumentType::Spot,
            SmolStr::new("acct-a"),
        );
        manager.config_path = Some(path.clone());
        let level_id = manager
            .add_level(create_test_level_config("buy_level_1"), pair.clone())
            .await
            .unwrap();
        manager
            .confirm_posted(
                &level_id,
                &SmolStr::new("cid-1"),
                &OrderId::new("ex-1".into()),
            )
            .unwrap();
        assert_eq!(
            manager.get_level(&level_id).unwrap().state,
            LevelState::Posted
        );
        manager
            .handle_order_filled(&level_id, &OrderId::new("ex-1".into()), dec!(0.001))
            .await
            .unwrap();
        let reloaded = reload(&path, &[pair], Exchange::Paper, "acct-a").await;
        let level = reloaded.get_level(&level_id).unwrap();
        assert_eq!(level.state, LevelState::Filled);
        assert!(level.active_orders.is_empty());
        assert_eq!(level.filled_qty, dec!(0.001));
        assert_eq!(level.fill_count, 1);
        assert!(level.last_filled_at.is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn disable_does_not_allow_repost() {
        let mut manager = LevelManager::new();
        let level_id = manager
            .add_level(
                create_test_level_config("buy_level_1"),
                Pair::new("BTC", "USDT"),
            )
            .await
            .unwrap();
        manager
            .update_level_state(&level_id, LevelState::Filled)
            .unwrap();
        manager.disable_level(&level_id).await.unwrap();
        assert!(!manager.should_repost(&level_id).unwrap());
    }

    #[tokio::test]
    async fn pending_delete_survives_restart_with_owned_ids() {
        let dir = unique_temp_dir();
        let path = dir.join("levels.json");
        let pair = Pair::new("BTC", "USDT");
        let mut manager = LevelManager::new().with_identity_scope(
            Exchange::Paper,
            InstrumentType::Spot,
            SmolStr::new("acct-a"),
        );
        manager.config_path = Some(path.clone());
        let level_id = manager
            .add_level(create_test_level_config("buy_level_1"), pair.clone())
            .await
            .unwrap();
        manager
            .confirm_posted(
                &level_id,
                &SmolStr::new("cid-1"),
                &OrderId::new("ex-1".into()),
            )
            .unwrap();
        let owned = manager.begin_delete(&level_id).unwrap();
        assert_eq!(owned[0].as_str(), "ex-1");
        assert_eq!(
            manager.get_level(&level_id).unwrap().state,
            LevelState::Deleting
        );
        let catalog = LevelsCatalog::load_from_path(&path).unwrap();
        assert!(!catalog.tombstones.iter().any(|id| id == level_id.as_str()));
        assert!(catalog
            .instances
            .iter()
            .any(|record| record.id == level_id.as_str()
                && record.owned_exchange_ids == ["ex-1".to_string()]));

        let reloaded = reload(
            &path,
            std::slice::from_ref(&pair),
            Exchange::Paper,
            "acct-a",
        )
        .await;
        let level = reloaded.get_level(&level_id).unwrap();
        assert_eq!(level.state, LevelState::Deleting);
        assert_eq!(level.active_orders[0].as_str(), "ex-1");
        let err = {
            let mut reloaded = reloaded;
            reloaded.complete_delete(&level_id).unwrap_err()
        };
        assert!(err.to_string().contains("owned orders"));

        let mut reloaded = reload(
            &path,
            std::slice::from_ref(&pair),
            Exchange::Paper,
            "acct-a",
        )
        .await;
        reloaded
            .handle_order_cancelled(&level_id, &OrderId::new("ex-1".into()), dec!(0))
            .await
            .unwrap();
        reloaded.complete_delete(&level_id).unwrap();
        let after = reload(&path, &[pair], Exchange::Paper, "acct-a").await;
        assert!(after.get_level(&level_id).is_none());
        let catalog = LevelsCatalog::load_from_path(&path).unwrap();
        assert!(catalog.tombstones.iter().any(|id| id == level_id.as_str()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn restart_restores_repost_count_and_last_filled_at() {
        let dir = unique_temp_dir();
        let path = dir.join("levels.json");
        let pair = Pair::new("BTC", "USDT");
        let mut config = create_test_level_config("buy_level_1");
        config.lifecycle.max_reposts = Some(2);
        config.lifecycle.repost_delay_secs = Some(60);
        let mut manager = LevelManager::new().with_identity_scope(
            Exchange::Paper,
            InstrumentType::Spot,
            SmolStr::new("acct-a"),
        );
        manager.config_path = Some(path.clone());
        let level_id = manager.add_level(config, pair.clone()).await.unwrap();
        manager
            .handle_order_filled(&level_id, &OrderId::new("ex-1".into()), dec!(0.001))
            .await
            .unwrap();
        {
            let level = manager.get_level_mut(&level_id).unwrap();
            level.repost_count = 2;
        }
        manager.persist_runtime_state().unwrap();

        let reloaded = reload(&path, &[pair], Exchange::Paper, "acct-a").await;
        let level = reloaded.get_level(&level_id).unwrap();
        assert_eq!(level.repost_count, 2);
        assert!(level.last_filled_at.is_some());
        assert!(!reloaded.should_repost(&level_id).unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn rest_verify_without_ws_completes_delete() {
        let dir = unique_temp_dir();
        let path = dir.join("levels.json");
        let pair = Pair::new("BTC", "USDT");
        let mut manager = LevelManager::new().with_identity_scope(
            Exchange::Paper,
            InstrumentType::Spot,
            SmolStr::new("acct-a"),
        );
        manager.config_path = Some(path.clone());
        let level_id = manager
            .add_level(create_test_level_config("buy_level_1"), pair.clone())
            .await
            .unwrap();
        manager
            .confirm_posted(
                &level_id,
                &SmolStr::new("cid-1"),
                &OrderId::new("ex-1".into()),
            )
            .unwrap();
        manager.begin_delete(&level_id).unwrap();
        manager
            .retire_verified_order_ids(&level_id, &[OrderId::new("ex-1".into())])
            .unwrap();
        manager.complete_delete(&level_id).unwrap();
        let reloaded = reload(&path, &[pair], Exchange::Paper, "acct-a").await;
        assert!(reloaded.get_level(&level_id).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn replayed_fill_does_not_inflate_totals() {
        let mut manager = LevelManager::new();
        let level_id = manager
            .add_level(
                create_test_level_config("buy_level_1"),
                Pair::new("BTC", "USDT"),
            )
            .await
            .unwrap();
        let order_id = OrderId::new("ex-1".into());
        manager
            .confirm_posted(&level_id, &SmolStr::new("cid-1"), &order_id)
            .unwrap();
        manager
            .handle_order_filled(&level_id, &order_id, dec!(0.001))
            .await
            .unwrap();
        manager
            .handle_order_filled(&level_id, &order_id, dec!(0.001))
            .await
            .unwrap();
        let level = manager.get_level(&level_id).unwrap();
        assert_eq!(level.filled_qty, dec!(0.001));
        assert_eq!(level.fill_count, 1);
    }

    #[tokio::test]
    async fn restart_after_rest_gone_keeps_deleting_not_posted() {
        let dir = unique_temp_dir();
        let path = dir.join("levels.json");
        let pair = Pair::new("BTC", "USDT");
        let mut manager = LevelManager::new().with_identity_scope(
            Exchange::Paper,
            InstrumentType::Spot,
            SmolStr::new("acct-a"),
        );
        manager.config_path = Some(path.clone());
        let level_id = manager
            .add_level(create_test_level_config("buy_level_1"), pair.clone())
            .await
            .unwrap();
        manager
            .confirm_posted(
                &level_id,
                &SmolStr::new("cid-1"),
                &OrderId::new("ex-1".into()),
            )
            .unwrap();
        manager.begin_delete(&level_id).unwrap();
        let mut reloaded = reload(&path, &[pair], Exchange::Paper, "acct-a").await;
        assert_eq!(
            reloaded.get_level(&level_id).unwrap().state,
            LevelState::Deleting
        );
        reloaded
            .retire_absent_owned_orders(&std::collections::HashSet::new())
            .unwrap();
        reloaded.complete_ready_deletes().unwrap();
        assert!(reloaded.get_level(&level_id).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
