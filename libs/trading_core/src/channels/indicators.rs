//! Channel types for indicator updates
//!
//! Uses broadcast channels for fan-out to multiple consumers (Trading Engine, TUI, etc.)
use rust_decimal::Decimal;
use smol_str::SmolStr;
use tokio::sync::broadcast;

/// Indicator update message
#[derive(Debug, Clone)]
pub struct IndicatorUpdate {
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

    /// Whether this indicator is composite (built from multiple inputs)
    pub is_composite: bool,
}

/// Indicator update channel sender (broadcast for fan-out)
pub type IndicatorUpdateSender = broadcast::Sender<IndicatorUpdate>;

/// Indicator update channel receiver (broadcast for fan-out)
pub type IndicatorUpdateReceiver = broadcast::Receiver<IndicatorUpdate>;

/// Create a new indicator update broadcast channel
///
/// # Arguments
/// * `capacity` - Channel capacity (number of messages to buffer). Use 1000 for high-frequency updates.
pub fn indicator_update_channel(
    capacity: usize,
) -> (IndicatorUpdateSender, IndicatorUpdateReceiver) {
    broadcast::channel(capacity)
}
