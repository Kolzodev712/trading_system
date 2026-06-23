use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use trading_core::venue::dex::DexError;
use trading_core::venue::{VenueId, VenueMode};

/// DEX engine configuration (chain-specific `config` is opaque JSON until Proposal 01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexEngineConfig {
    pub venue: VenueId,
    #[serde(default)]
    pub chain: DexChain,
    pub config: JsonValue,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DexChain {
    #[default]
    Sui,
    #[serde(other)]
    Unknown,
}

impl DexEngineConfig {
    pub fn venue_mode(&self) -> VenueMode {
        VenueMode::Dex
    }

    /// Parse typed DeepBook config when `venue` is `deepbook` and `chain` is `sui`.
    pub fn deepbook_config(&self) -> Result<crate::chains::sui::DeepBookConfig, DexError> {
        crate::chains::sui::DeepBookConfig::from_dex_engine(self)
    }
}
