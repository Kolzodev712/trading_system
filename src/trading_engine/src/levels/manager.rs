//! Level manager - manages level lifecycle and state transitions
use super::{Level, LevelConfig, LevelState};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use trading_core::error::EngineError;
use trading_core::types::common::OrderId;

/// Manages level lifecycle and state transitions
pub struct LevelManager {
    levels: HashMap<SmolStr, Level>,
    config_path: Option<PathBuf>,
    persistence: Option<Arc<super::persistence::LevelPersistence>>,
}

impl LevelManager {
    /// Create a new level manager
    pub fn new() -> Self {
        Self {
            levels: HashMap::new(),
            config_path: None,
            persistence: None,
        }
    }

    /// Set persistence for level manager
    pub fn with_persistence(
        mut self,
        persistence: Arc<super::persistence::LevelPersistence>,
    ) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// Load levels from configuration file
    /// Expands pair-agnostic level configs to create Level instances for each pair
    pub async fn load_from_config(
        &mut self,
        path: &Path,
        pairs: &[trading_core::types::common::Pair],
    ) -> Result<(), EngineError> {
        if pairs.is_empty() {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "No pairs provided - cannot expand levels"
            )));
        }

        let content = fs::read_to_string(path).await.map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to read config file: {}", e))
        })?;

        #[derive(serde::Deserialize)]
        struct LevelsConfig {
            levels: Vec<LevelConfig>,
        }

        let config: LevelsConfig = serde_json::from_str(&content).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to parse config: {}", e))
        })?;

        // Expand each level config for each pair
        for level_config in config.levels {
            for pair in pairs {
                // Generate pair-specific level ID: "buy_level_1" -> "buy_level_1_ETHUSDC"
                let pair_str = pair.convert_to_smolstr();
                let level_id = SmolStr::new(&format!("{}_{}", level_config.id, pair_str));

                // Create level instance for this pair
                let level = Level::new(level_config.clone(), pair.clone());
                self.levels.insert(level_id, level);
            }
        }

        self.config_path = Some(path.to_path_buf());
        Ok(())
    }

    /// Load levels and risk limits from configuration file
    /// Expands pair-agnostic level configs to create Level instances for each pair
    pub async fn load_from_config_with_risk_limits(
        &mut self,
        path: &Path,
        pairs: &[trading_core::types::common::Pair],
    ) -> Result<crate::risk::RiskLimits, EngineError> {
        if pairs.is_empty() {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "No pairs provided - cannot expand levels"
            )));
        }

        let content = fs::read_to_string(path).await.map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to read config file: {}", e))
        })?;

        #[derive(serde::Deserialize)]
        struct LevelsConfigFile {
            levels: Vec<LevelConfig>,
            #[serde(default)]
            risk_limits: crate::risk::RiskLimits,
        }

        let config: LevelsConfigFile = serde_json::from_str(&content).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to parse config: {}", e))
        })?;

        // Expand each level config for each pair
        for level_config in config.levels {
            for pair in pairs {
                // Generate pair-specific level ID: "buy_level_1" -> "buy_level_1_ETHUSDC"
                let pair_str = pair.convert_to_smolstr();
                let level_id = SmolStr::new(&format!("{}_{}", level_config.id, pair_str));

                // Create level instance for this pair
                let level = Level::new(level_config.clone(), pair.clone());
                self.levels.insert(level_id, level);
            }
        }

        self.config_path = Some(path.to_path_buf());
        Ok(config.risk_limits)
    }

    /// Add a new level for a specific pair
    pub async fn add_level(
        &mut self,
        config: LevelConfig,
        pair: trading_core::types::common::Pair,
    ) -> Result<(), EngineError> {
        // Generate pair-specific level ID
        let pair_str = pair.convert_to_smolstr();
        let level_id = SmolStr::new(&format!("{}_{}", config.id, pair_str));

        if self.levels.contains_key(&level_id) {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Level with ID {} already exists for pair {}",
                config.id,
                pair_str
            )));
        }

        let level = Level::new(config.clone(), pair);
        self.levels.insert(level_id, level);

        // Persist to MongoDB if enabled
        if let Some(persistence) = &self.persistence {
            if let Err(e) = persistence.update_level(&config).await {
                tracing::warn!("Failed to persist level to MongoDB: {}", e);
            }
        }

        Ok(())
    }

    /// Remove a level
    pub async fn remove_level(&mut self, level_id: &SmolStr) -> Result<(), EngineError> {
        if let Some(mut level) = self.levels.remove(level_id) {
            level.state = LevelState::Removed;

            // Persist deletion to MongoDB if enabled
            if let Some(persistence) = &self.persistence {
                if let Err(e) = persistence.delete_level(level_id.as_str()).await {
                    tracing::warn!("Failed to persist level deletion to MongoDB: {}", e);
                }
            }

            Ok(())
        } else {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {} not found",
                level_id
            )))
        }
    }

    /// Enable a level
    pub async fn enable_level(&mut self, level_id: &SmolStr) -> Result<(), EngineError> {
        if let Some(level) = self.levels.get_mut(level_id) {
            level.config.enabled = true;
            if matches!(level.state, LevelState::Paused) {
                level.state = LevelState::Created;
            }

            // Persist to MongoDB if enabled
            if let Some(persistence) = &self.persistence {
                if let Err(e) = persistence.update_level(&level.config).await {
                    tracing::warn!("Failed to persist level update to MongoDB: {}", e);
                }
            }

            Ok(())
        } else {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {} not found",
                level_id
            )))
        }
    }

    /// Disable/pause a level
    pub async fn disable_level(&mut self, level_id: &SmolStr) -> Result<(), EngineError> {
        if let Some(level) = self.levels.get_mut(level_id) {
            level.config.enabled = false;
            level.state = LevelState::Paused;

            // Persist to MongoDB if enabled
            if let Some(persistence) = &self.persistence {
                if let Err(e) = persistence.update_level(&level.config).await {
                    tracing::warn!("Failed to persist level update to MongoDB: {}", e);
                }
            }

            Ok(())
        } else {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {} not found",
                level_id
            )))
        }
    }

    /// Update level configuration and persist
    pub async fn update_level_config(
        &mut self,
        level_id: &SmolStr,
        config: LevelConfig,
    ) -> Result<(), EngineError> {
        if let Some(level) = self.levels.get_mut(level_id) {
            // Clone config for persistence before moving
            let config_for_persistence = config.clone();
            level.config = config;

            // Persist to MongoDB if enabled
            if let Some(persistence) = &self.persistence {
                if let Err(e) = persistence.update_level(&config_for_persistence).await {
                    tracing::warn!("Failed to persist level update to MongoDB: {}", e);
                }
            }

            Ok(())
        } else {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {} not found",
                level_id
            )))
        }
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
    ) -> Result<(), EngineError> {
        if let Some(level) = self.levels.get_mut(level_id) {
            // Remove order from active orders
            level.active_orders.retain(|id| id != order_id);
            level.state = LevelState::Filled;
            level.last_filled_at = Some(trading_core::utils::current_timestamp());
            Ok(())
        } else {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {} not found",
                level_id
            )))
        }
    }

    /// Handle order cancelled for a level
    pub async fn handle_order_cancelled(
        &mut self,
        level_id: &SmolStr,
        order_id: &OrderId,
    ) -> Result<(), EngineError> {
        if let Some(level) = self.levels.get_mut(level_id) {
            // Remove order from active orders
            level.active_orders.retain(|id| id != order_id);
            level.state = LevelState::Cancelled;
            Ok(())
        } else {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "Level {} not found",
                level_id
            )))
        }
    }

    /// Check if level should repost
    pub fn should_repost(&self, level_id: &SmolStr) -> Result<bool, EngineError> {
        let level = self.levels.get(level_id).ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!("Level {} not found", level_id))
        })?;

        if !level.config.enabled {
            return Ok(false);
        }

        match level.state {
            LevelState::Filled => Ok(level.config.lifecycle.auto_repost),
            LevelState::Cancelled => Ok(level.config.lifecycle.auto_repost_on_cancel),
            _ => Ok(false),
        }
    }

    /// Save all levels and risk limits to MongoDB
    pub async fn save_all_levels(
        &self,
        risk_limits: &crate::risk::RiskLimits,
        configured_pairs: &[trading_core::types::common::Pair],
    ) -> Result<(), EngineError> {
        if let Some(persistence) = &self.persistence {
            // Extract unique level configs (deduplicate by config ID since we have multiple instances per pair)
            let mut seen_configs = std::collections::HashSet::new();
            let levels: Vec<LevelConfig> = self
                .levels
                .values()
                .filter_map(|l| {
                    // Only include each unique config once (remove pair suffix from ID)
                    let base_id = l.config.id.as_str();
                    // Extract base ID by removing pair suffix (format: "level_id_PAIR")
                    let base_id = if let Some(underscore_pos) = base_id.rfind('_') {
                        // Check if the part after last underscore looks like a pair (e.g., "ETHUSDC")
                        let potential_pair = &base_id[underscore_pos + 1..];
                        if potential_pair.len() >= 4
                            && potential_pair
                                .chars()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                        {
                            &base_id[..underscore_pos]
                        } else {
                            base_id
                        }
                    } else {
                        base_id
                    };

                    if seen_configs.insert(base_id.to_string()) {
                        Some(l.config.clone())
                    } else {
                        None
                    }
                })
                .collect();

            persistence
                .save_levels(&levels, risk_limits, configured_pairs)
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

    /// Load levels from MongoDB (if available)
    /// Expands pair-agnostic level configs to create Level instances for each pair
    pub async fn load_from_mongo(
        &mut self,
        pairs: &[trading_core::types::common::Pair],
    ) -> Result<Option<crate::risk::RiskLimits>, EngineError> {
        if pairs.is_empty() {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "No pairs provided - cannot expand levels"
            )));
        }
        if let Some(persistence) = &self.persistence {
            if let Some(bot_config) = persistence.load_levels().await.map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!(
                    "Failed to load levels from MongoDB: {}",
                    e
                ))
            })? {
                // Convert document levels to LevelConfig and add them
                let mut loaded_count = 0;
                let mut error_count = 0;

                for level_doc in &bot_config.levels {
                    match persistence.document_to_level_config(level_doc) {
                        Ok(level_config) => {
                            // Expand level config for each pair
                            for pair in pairs {
                                let pair_str = pair.convert_to_smolstr();
                                let level_id =
                                    SmolStr::new(&format!("{}_{}", level_config.id, pair_str));
                                let level = Level::new(level_config.clone(), pair.clone());
                                self.levels.insert(level_id, level);
                                loaded_count += 1;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to convert level document {} to LevelConfig: {}",
                                level_doc.id,
                                e
                            );
                            error_count += 1;
                        }
                    }
                }

                tracing::info!(
                    "Loaded {} levels from MongoDB ({} successful, {} errors)",
                    bot_config.levels.len(),
                    loaded_count,
                    error_count
                );

                // Convert risk limits
                let risk_limits = crate::risk::RiskLimits {
                    total_max_position: bot_config.risk_limits.total_max_position,
                    max_total_orders: bot_config.risk_limits.max_total_orders,
                    daily_loss_limit_all_pairs: bot_config.risk_limits.daily_loss_limit_all_pairs,
                    daily_loss_limit_per_pair: bot_config
                        .risk_limits
                        .daily_loss_limit_per_pair
                        .into_iter()
                        .map(|(k, v)| (SmolStr::new(&k), v))
                        .collect(),
                };
                return Ok(Some(risk_limits));
            }
        }
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
    use crate::levels::config::{
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
        let result = manager.add_level(config.clone(), pair).await;
        assert!(result.is_ok());

        let level = manager.get_level(&SmolStr::new("test_level"));
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
        assert!(manager.get_level(&SmolStr::new("test_level")).is_none());
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
            .handle_order_filled(&level_id, &order_id)
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
            .handle_order_cancelled(&level_id, &order_id)
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
}
