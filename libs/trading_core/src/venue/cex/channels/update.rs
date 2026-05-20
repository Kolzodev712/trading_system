//! Channel types for exchange updates
//!
//! Uses broadcast channels for fan-out to multiple consumers (Trading Engine, Risk Engine, TUI, etc.)
use crate::venue::cex::types::exchange::{Balance, Order, Position, Trade};
use tokio::sync::broadcast;
use trading_trace::ColdTraceContext;

/// CEX venue update message (individual updates, not batches)
#[derive(Debug, Clone)]
pub enum CexVenueUpdate {
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

/// CEX venue update channel sender (broadcast for fan-out)
pub type CexVenueUpdateSender = broadcast::Sender<CexVenueUpdate>;

/// CEX venue update channel receiver (broadcast for fan-out)
pub type CexVenueUpdateReceiver = broadcast::Receiver<CexVenueUpdate>;

/// Create a new exchange update broadcast channel
///
/// # Arguments
/// * `capacity` - Channel capacity (number of messages to buffer). Use 1000 for high-frequency updates.
pub fn cex_venue_update_channel(capacity: usize) -> (CexVenueUpdateSender, CexVenueUpdateReceiver) {
    broadcast::channel(capacity)
}
