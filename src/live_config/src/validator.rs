//! Configuration validator - validates changes before applying
//! Phase 2 implementation

use crate::types::{ConfigPath, ConfigValue, LevelField, LevelValue, RiskLimitType};
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_engine::levels::{LevelManager, RiskManager};

pub struct ConfigValidator {
    level_manager: Arc<RwLock<LevelManager>>,
    #[allow(dead_code)]
    pub(crate) risk_manager: Arc<RwLock<RiskManager>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Level not found: {0}")]
    LevelNotFound(String),

    #[error("Invalid field type: {0}")]
    InvalidFieldType(String),

    #[error("Invalid value for field: {0}")]
    InvalidValue(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Config path error: {0}")]
    ConfigPathError(String),
}

impl ConfigValidator {
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        risk_manager: Arc<RwLock<RiskManager>>,
    ) -> Self {
        Self {
            level_manager,
            risk_manager,
        }
    }

    /// Validate a level update request
    pub async fn validate_level_update(
        &self,
        level_id: &str,
        field: &LevelField,
        value: &LevelValue,
    ) -> Result<(), ValidationError> {
        // Check if level exists
        let level_manager = self.level_manager.read().await;
        let level_id_smol = SmolStr::new(level_id);
        if level_manager.get_level(&level_id_smol).is_none() {
            return Err(ValidationError::LevelNotFound(level_id.to_string())); // Keep String for error message
        }
        drop(level_manager);

        // Validate field-value compatibility
        match field {
            LevelField::Price
            | LevelField::Quantity
            | LevelField::StopLoss
            | LevelField::TakeProfit
            | LevelField::RiskLimit
            | LevelField::MaxPositionSize => {
                if value.as_decimal().is_none() {
                    return Err(ValidationError::InvalidValue(format!(
                        "Field {:?} requires a decimal value",
                        field
                    )));
                }
            }
            LevelField::Enabled | LevelField::RepostOnFill | LevelField::RepostOnCancel => {
                if value.as_bool().is_none() {
                    return Err(ValidationError::InvalidValue(format!(
                        "Field {:?} requires a boolean value",
                        field
                    )));
                }
            }
        }

        // Additional business rule validations can be added here
        // e.g., check if level has active orders, etc.

        Ok(())
    }

    /// Validate a risk limit update request
    pub fn validate_risk_limit_update(
        &self,
        _limit_type: &RiskLimitType,
        value: &Decimal,
    ) -> Result<(), ValidationError> {
        // Basic validation - value must be positive
        if *value <= Decimal::ZERO {
            return Err(ValidationError::InvalidValue(
                "Risk limit must be greater than zero".to_string(),
            ));
        }

        // Additional validations can be added here
        // e.g., check current positions against new limit

        Ok(())
    }

    /// Validate a bot config update request
    pub fn validate_config_update(
        &self,
        path: &ConfigPath,
        _value: &ConfigValue,
    ) -> Result<(), ValidationError> {
        // Validate path format
        path.validate()
            .map_err(|e| ValidationError::ConfigPathError(e.to_string()))?;

        // Additional path-specific validations can be added here

        Ok(())
    }
}
