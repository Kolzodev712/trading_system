//! Update request types for LiveConfig service
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

use super::{ConfigPath, ConfigValue, LevelField, LevelValue, RiskLimitType};

/// Request to update a level value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelUpdateRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub level_id: SmolStr,
    pub field: LevelField,
    pub value: LevelValue,
}

/// Request to update bot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdateRequest {
    pub path: ConfigPath,
    pub value: ConfigValue,
}

/// Request to update risk limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimitUpdateRequest {
    pub limit_type: RiskLimitType,
    pub value: Decimal,
}

/// Response to an update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResponse {
    pub success: bool,
    // Keep message as String for error messages (display purposes)
    pub message: String,
    pub timestamp: u64,
}

impl UpdateResponse {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            timestamp: chrono::Utc::now().timestamp() as u64,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            timestamp: chrono::Utc::now().timestamp() as u64,
        }
    }
}
