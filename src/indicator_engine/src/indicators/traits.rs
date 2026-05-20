use crate::data::price_point::PricePoint;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use thiserror::Error;

/// Trait that all indicators must implement
pub trait Indicator: Send + Sync {
    /// Unique identifier for this indicator instance
    fn id(&self) -> &str;

    /// Indicator name (e.g., "SMA", "EMA")
    fn name(&self) -> &str;

    /// Trading pair this indicator is for
    fn pair(&self) -> &str;

    /// Period/length of the indicator (e.g., 20 for SMA20)
    fn period(&self) -> usize;

    /// Update the indicator with a new price point
    fn update(&mut self, price_point: PricePoint) -> Result<(), IndicatorError>;

    /// Get the current indicator value
    fn value(&self) -> Option<Decimal>;

    /// Check if the indicator has enough data to calculate a value
    fn is_ready(&self) -> bool;

    /// Reset the indicator (clear all data)
    fn reset(&mut self);

    /// Get the indicator configuration
    fn config(&self) -> IndicatorConfig;
}

/// Indicator configuration
#[derive(Debug, Clone)]
pub struct IndicatorConfig {
    pub id: SmolStr,
    pub name: SmolStr,
    pub pair: SmolStr,
    pub period: usize,
    pub source: PriceSource, // Close, Open, High, Low, Volume, etc.
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceSource {
    Close,
    Open,
    High,
    Low,
    Volume,
    TypicalPrice,  // (High + Low + Close) / 3
    WeightedPrice, // (High + Low + 2*Close) / 4
}

#[derive(Debug, Error)]
pub enum IndicatorError {
    #[error("Insufficient data: need {needed} points, have {have}")]
    InsufficientData { needed: usize, have: usize },

    #[error("Invalid period: {period}")]
    InvalidPeriod { period: usize },

    #[error("Calculation error: {0}")]
    CalculationError(SmolStr),
}
