//! Channel types for indicator updates
use rust_decimal::Decimal;
use tokio::sync::mpsc;

/// Indicator update message
#[derive(Debug, Clone)]
pub struct IndicatorUpdate {
    /// Indicator instance ID
    pub indicator_id: String,
    /// Indicator name (e.g., "SMA", "EMA")
    pub indicator_name: String,
    /// Trading pair
    pub pair: String,
    /// Calculated value
    pub value: Decimal,
    /// Timestamp when the value was calculated
    pub timestamp: u64,
    /// Period/length of the indicator
    pub period: usize,
}

/// Indicator update channel sender
pub type IndicatorUpdateSender = mpsc::UnboundedSender<IndicatorUpdate>;

/// Indicator update channel receiver
pub type IndicatorUpdateReceiver = mpsc::UnboundedReceiver<IndicatorUpdate>;

/// Create a new indicator update channel pair
pub fn indicator_update_channel() -> (IndicatorUpdateSender, IndicatorUpdateReceiver) {
    mpsc::unbounded_channel()
}
