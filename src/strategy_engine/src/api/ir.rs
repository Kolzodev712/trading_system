//! Strategy Intermediate Representation (IR)
//!
//! Defines the IR format that Python strategies compile to.
//! This IR is then interpreted by the Rust runtime.
//!
//! Key design decisions:
//! - IR is JSON-serializable (for storage and hot-reloading)
//! - IR is language-agnostic (Python compiles to it, but could be other languages)
//! - IR supports both strategy logic AND indicator definitions
//! - IR separates structure (compile-time) from parameters (runtime-updateable)

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Strategy Intermediate Representation
/// This is what Python strategies compile to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyIR {
    /// Unique identifier for this strategy
    pub id: SmolStr,

    /// Strategy name
    pub name: SmolStr,

    /// Trading pair this strategy operates on
    pub pair: SmolStr,

    /// Indicators required by this strategy
    /// Can reference system-wide indicators OR define new ones
    pub indicators: StrategyIndicators,

    /// Strategy logic (conditions, order generation)
    pub logic: StrategyLogic,

    /// Runtime-updateable parameters
    pub parameters: StrategyParameters,

    /// Metadata
    pub metadata: StrategyMetadata,
}

/// Indicator requirements for a strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyIndicators {
    /// Indicator IDs that reference system-wide indicators
    pub system_indicator_ids: Vec<SmolStr>,

    /// Strategy-specific indicator definitions
    pub strategy_indicators: Vec<StrategyIndicatorDef>,
}

/// Strategy-specific indicator definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyIndicatorDef {
    // TODO: Define structure for strategy-defined indicators
    // Should include indicator config (SMA, EMA, composite, etc.)
}

/// Strategy execution logic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyLogic {
    // TODO: Define IR representation of strategy logic
    // This will include:
    // - Conditional expressions
    // - Order generation rules
    // - Signal/trigger definitions
    // - State management
}

/// Runtime-updateable parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParameters {
    // TODO: Define parameter structure
    // Examples: order_amount, threshold, stop_loss, etc.
    // These can be updated at runtime via live_config service
}

/// Strategy metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyMetadata {
    /// Strategy version
    pub version: SmolStr,

    /// Created timestamp
    pub created_at: u64,

    /// Updated timestamp
    pub updated_at: u64,

    /// Whether strategy is active
    pub active: bool,
}

impl StrategyIR {
    /// Validate IR structure
    pub fn validate(&self) -> Result<(), IRValidationError> {
        // TODO: Validate IR structure
        // - Check all indicator references exist
        // - Validate logic syntax
        // - Ensure parameters are properly defined
        todo!()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IRValidationError {
    // TODO: Define validation errors
}
