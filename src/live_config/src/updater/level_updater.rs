//! Level updater - updates level values in LevelManager
use crate::types::{LevelField, LevelValue};
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::error::EngineError;
use trading_engine::levels::LevelManager;

pub struct LevelUpdater {
    pub(crate) level_manager: Arc<RwLock<LevelManager>>,
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Level not found: {0}")]
    LevelNotFound(String),

    #[error("Invalid field: {0}")]
    InvalidField(String),

    #[error("Invalid value: {0}")]
    InvalidValue(String),

    #[error("Engine error: {0}")]
    EngineError(#[from] EngineError),
}

impl LevelUpdater {
    pub fn new(level_manager: Arc<RwLock<LevelManager>>) -> Self {
        Self { level_manager }
    }

    /// Update a specific field in a level
    pub async fn update_field(
        &self,
        level_id: &str,
        field: LevelField,
        value: LevelValue,
    ) -> Result<(), UpdateError> {
        let mut level_manager = self.level_manager.write().await;
        let level_id_smol = SmolStr::new(level_id);

        let level = level_manager
            .get_level_mut(&level_id_smol)
            .ok_or_else(|| UpdateError::LevelNotFound(level_id.to_string()))?;

        // Update the field based on type
        match field {
            LevelField::Enabled => {
                let enabled = value.as_bool().ok_or_else(|| {
                    UpdateError::InvalidValue("Enabled requires boolean".to_string())
                })?;
                level.config.enabled = enabled;

                // If enabling, change state from Paused to Created
                if enabled && matches!(level.state, trading_engine::levels::LevelState::Paused) {
                    level.state = trading_engine::levels::LevelState::Created;
                }
            }
            LevelField::Price => {
                // Note: This is a simplified update - in practice, you'd need to handle
                // PriceConfig enum (Fixed, Indicator, BestBid, etc.)
                // For now, we'll log a warning that this needs full implementation
                tracing::warn!(
                    "Price field update requires full PriceConfig handling - not yet implemented"
                );
                return Err(UpdateError::InvalidField(
                    "Price update requires full implementation".to_string(),
                ));
            }
            LevelField::Quantity => {
                // Note: Similar to Price, QuantityConfig is an enum
                tracing::warn!("Quantity field update requires full QuantityConfig handling - not yet implemented");
                return Err(UpdateError::InvalidField(
                    "Quantity update requires full implementation".to_string(),
                ));
            }
            LevelField::StopLoss => {
                if let Some(decimal) = value.as_decimal() {
                    level.config.risk.daily_loss_limit = Some(decimal);
                } else {
                    return Err(UpdateError::InvalidValue(
                        "StopLoss requires decimal".to_string(),
                    ));
                }
            }
            LevelField::TakeProfit => {
                // Take profit would need to be added to LevelConfig if not already present
                tracing::warn!("TakeProfit field not yet supported in LevelConfig");
                return Err(UpdateError::InvalidField(
                    "TakeProfit not yet implemented".to_string(),
                ));
            }
            LevelField::RiskLimit => {
                if let Some(decimal) = value.as_decimal() {
                    level.config.risk.daily_loss_limit = Some(decimal);
                } else {
                    return Err(UpdateError::InvalidValue(
                        "RiskLimit requires decimal".to_string(),
                    ));
                }
            }
            LevelField::MaxPositionSize => {
                if let Some(decimal) = value.as_decimal() {
                    level.config.risk.max_position = Some(decimal);
                } else {
                    return Err(UpdateError::InvalidValue(
                        "MaxPositionSize requires decimal".to_string(),
                    ));
                }
            }
            LevelField::RepostOnFill => {
                let repost = value.as_bool().ok_or_else(|| {
                    UpdateError::InvalidValue("RepostOnFill requires boolean".to_string())
                })?;
                level.config.lifecycle.auto_repost = repost;
            }
            LevelField::RepostOnCancel => {
                let repost = value.as_bool().ok_or_else(|| {
                    UpdateError::InvalidValue("RepostOnCancel requires boolean".to_string())
                })?;
                level.config.lifecycle.auto_repost_on_cancel = repost;
            }
        }

        // Persist the updated level config (LevelManager will handle this if persistence is enabled)
        // The persistence is already integrated in LevelManager methods

        Ok(())
    }
}
