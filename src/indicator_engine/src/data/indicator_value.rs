use rust_decimal::Decimal;
use smol_str::SmolStr;

/// An indicator value with metadata
#[derive(Debug, Clone, PartialEq)]
pub struct IndicatorValue {
    /// Indicator instance ID
    pub indicator_id: SmolStr,

    /// Indicator name (e.g., "SMA", "EMA")
    pub indicator_name: SmolStr,

    /// Trading pair
    pub pair: SmolStr,

    /// Calculated value
    pub value: Decimal,

    /// Timestamp when the value was calculated
    pub timestamp: u64,

    /// Period/length of the indicator
    pub period: usize,
}
