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
    pub async fn load_from_config(&mut self, path: &Path) -> Result<(), EngineError> {
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

        for level_config in config.levels {
            let level = Level::new(level_config);
            self.levels.insert(level.config.id.clone(), level);
        }

        self.config_path = Some(path.to_path_buf());
        Ok(())
    }

    /// Load levels and risk limits from configuration file
    pub async fn load_from_config_with_risk_limits(
        &mut self,
        path: &Path,
    ) -> Result<super::risk::RiskLimits, EngineError> {
        let content = fs::read_to_string(path).await.map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to read config file: {}", e))
        })?;

        #[derive(serde::Deserialize)]
        struct LevelsConfigFile {
            levels: Vec<LevelConfig>,
            #[serde(default)]
            risk_limits: super::risk::RiskLimits,
        }

        let config: LevelsConfigFile = serde_json::from_str(&content).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to parse config: {}", e))
        })?;

        for level_config in config.levels {
            let level = Level::new(level_config);
            self.levels.insert(level.config.id.clone(), level);
        }

        self.config_path = Some(path.to_path_buf());
        Ok(config.risk_limits)
    }

    /// Add a new level
    pub async fn add_level(&mut self, config: LevelConfig) -> Result<(), EngineError> {
        if self.levels.contains_key(&config.id) {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Level with ID {} already exists",
                config.id
            )));
        }

        let level = Level::new(config.clone());
        self.levels.insert(level.config.id.clone(), level);

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
        risk_limits: &super::risk::RiskLimits,
    ) -> Result<(), EngineError> {
        if let Some(persistence) = &self.persistence {
            let levels: Vec<LevelConfig> = self.levels.values().map(|l| l.config.clone()).collect();
            persistence
                .save_levels(&levels, risk_limits)
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
    pub async fn load_from_mongo(
        &mut self,
    ) -> Result<Option<super::risk::RiskLimits>, EngineError> {
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
                            // Create level and add to manager
                            let level = Level::new(level_config.clone());
                            self.levels.insert(level.config.id.clone(), level);
                            loaded_count += 1;
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
                let risk_limits = super::risk::RiskLimits {
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
