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
        // Convert to SmolStr once before acquiring lock
        let level_id_smol = SmolStr::new(level_id);
        let level_id_for_error = level_id.to_string();

        let mut level_manager = self.level_manager.write().await;

        let level = level_manager
            .get_level_mut(&level_id_smol)
            .ok_or_else(|| UpdateError::LevelNotFound(level_id_for_error))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use smol_str::SmolStr;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use trading_core::types::common::{Pair, Side};
    use trading_engine::levels::config::{
        LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
    };
    use trading_engine::levels::{LevelConfig, LevelManager};

    fn create_test_level_config(id: &str) -> LevelConfig {
        LevelConfig {
            id: SmolStr::new(id),
            name: None,
            symbol: Pair::new("BTC", "USDT"),
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
    async fn test_level_updater_new() {
        let level_manager = Arc::new(RwLock::new(LevelManager::new()));
        let _updater = LevelUpdater::new(level_manager);
        // Should not panic
        assert!(true);
    }

    #[tokio::test]
    async fn test_level_updater_update_field_enabled() {
        let level_manager = Arc::new(RwLock::new(LevelManager::new()));
        let updater = LevelUpdater::new(level_manager.clone());

        // Add a level
        {
            let mut lm = level_manager.write().await;
            let config = create_test_level_config("test_level");
            lm.add_level(config).await.unwrap();
        }

        // Update enabled field
        let result = updater
            .update_field(
                "test_level",
                LevelField::Enabled,
                LevelValue::Boolean(false),
            )
            .await;
        assert!(result.is_ok());

        // Verify update
        let lm = level_manager.read().await;
        let level = lm.get_level(&SmolStr::new("test_level")).unwrap();
        assert!(!level.config.enabled);
    }

    #[tokio::test]
    async fn test_level_updater_update_field_not_found() {
        let level_manager = Arc::new(RwLock::new(LevelManager::new()));
        let updater = LevelUpdater::new(level_manager);

        let result = updater
            .update_field(
                "nonexistent",
                LevelField::Enabled,
                LevelValue::Boolean(true),
            )
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UpdateError::LevelNotFound(_)));
    }

    #[tokio::test]
    async fn test_level_updater_update_field_invalid_value() {
        let level_manager = Arc::new(RwLock::new(LevelManager::new()));
        let updater = LevelUpdater::new(level_manager.clone());

        // Add a level
        {
            let mut lm = level_manager.write().await;
            let config = create_test_level_config("test_level");
            lm.add_level(config).await.unwrap();
        }

        // Try to update with wrong type
        let result = updater
            .update_field(
                "test_level",
                LevelField::Enabled,
                LevelValue::Decimal(dec!(100)),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_level_updater_update_risk_limit() {
        let level_manager = Arc::new(RwLock::new(LevelManager::new()));
        let updater = LevelUpdater::new(level_manager.clone());

        // Add a level
        {
            let mut lm = level_manager.write().await;
            let config = create_test_level_config("test_level");
            lm.add_level(config).await.unwrap();
        }

        // Update risk limit
        let result = updater
            .update_field(
                "test_level",
                LevelField::RiskLimit,
                LevelValue::Decimal(dec!(1000)),
            )
            .await;
        assert!(result.is_ok());

        // Verify update
        let lm = level_manager.read().await;
        let level = lm.get_level(&SmolStr::new("test_level")).unwrap();
        assert_eq!(level.config.risk.daily_loss_limit, Some(dec!(1000)));
    }
}
