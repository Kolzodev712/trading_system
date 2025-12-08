//! Channel types for exchange updates
//!
//! Uses broadcast channels for fan-out to multiple consumers (Trading Engine, Risk Engine, TUI, etc.)
use crate::types::exchange::{Balance, Order, Position, Trade};
use tokio::sync::broadcast;

/// Exchange update message (individual updates, not batches)
#[derive(Debug, Clone)]
pub enum ExchangeUpdate {
    /// Order update
    Order(Order),
    /// Balance update
    Balance(Balance),
    /// Position update
    Position(Position),
    /// Trade update
    Trade(Trade),
}

/// Exchange update channel sender (broadcast for fan-out)
pub type ExchangeUpdateSender = broadcast::Sender<ExchangeUpdate>;

/// Exchange update channel receiver (broadcast for fan-out)
pub type ExchangeUpdateReceiver = broadcast::Receiver<ExchangeUpdate>;

/// Create a new exchange update broadcast channel
///
/// # Arguments
/// * `capacity` - Channel capacity (number of messages to buffer). Use 1000 for high-frequency updates.
pub fn exchange_update_channel(capacity: usize) -> (ExchangeUpdateSender, ExchangeUpdateReceiver) {
    broadcast::channel(capacity)
}
