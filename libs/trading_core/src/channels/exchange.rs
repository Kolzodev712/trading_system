//! Channel types for exchange updates
//!
//! Uses broadcast channels for fan-out to multiple consumers (Trading Engine, Risk Engine, TUI, etc.)
use crate::types::exchange::{Balance, Order, Position, Trade};
use tokio::sync::broadcast;
use trading_trace::ColdTraceContext;

/// Exchange update message (individual updates, not batches)
#[derive(Debug, Clone)]
pub enum ExchangeUpdate {
    /// Order update
    Order {
        order: Order,
        trace_context: Option<ColdTraceContext>,
    },
    /// Balance update
    Balance {
        balance: Balance,
        trace_context: Option<ColdTraceContext>,
    },
    /// Position update
    Position {
        position: Position,
        trace_context: Option<ColdTraceContext>,
    },
    /// Trade update
    Trade {
        trade: Trade,
        trace_context: Option<ColdTraceContext>,
    },
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
