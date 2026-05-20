use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use trading_core::venue::{VenueId, VenueMode};

/// DEX engine configuration (chain-specific `config` is opaque JSON until Proposal 02).
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
    Solana,
}

impl DexEngineConfig {
    pub fn venue_mode(&self) -> VenueMode {
        VenueMode::Dex
    }
}
