//! Configuration path types for bot configuration updates
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use thiserror::Error;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

/// Path to a configuration value (dot-separated)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigPath {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub path: SmolStr, // e.g., "reconciliation.exchange_level_interval_ms"
}

impl ConfigPath {
    pub fn new(path: impl Into<SmolStr>) -> Self {
        Self { path: path.into() }
    }

    pub fn segments(&self) -> Vec<&str> {
        self.path.split('.').collect()
    }

    pub fn validate(&self) -> Result<(), ConfigPathError> {
        if self.path.is_empty() {
            return Err(ConfigPathError::EmptyPath);
        }

        // Validate path format (alphanumeric, dots, underscores)
        if !self
            .path
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '_')
        {
            return Err(ConfigPathError::InvalidCharacters);
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ConfigPathError {
    #[error("Empty path")]
    EmptyPath,

    #[error("Invalid characters in path")]
    InvalidCharacters,
}

/// Value types for configuration updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigValue {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    String(SmolStr),
    Number(f64),
    Boolean(bool),
    Decimal(rust_decimal::Decimal),
}
