//! Channel types for exchange updates
use crate::types::exchange::{Balance, Order, Position, Trade};
use tokio::sync::mpsc;

/// Exchange update message
#[derive(Debug, Clone)]
pub struct ExchangeUpdate {
    /// Updated orders
    pub orders: Vec<Order>,
    /// Updated balances
    pub balances: Vec<Balance>,
    /// Updated positions
    pub positions: Vec<Position>,
    /// New trades
    pub trades: Vec<Trade>,
}

/// Exchange update channel sender
pub type ExchangeUpdateSender = mpsc::UnboundedSender<ExchangeUpdate>;

/// Exchange update channel receiver
pub type ExchangeUpdateReceiver = mpsc::UnboundedReceiver<ExchangeUpdate>;

/// Create a new exchange update channel pair
pub fn exchange_update_channel() -> (ExchangeUpdateSender, ExchangeUpdateReceiver) {
    mpsc::unbounded_channel()
}
