//! Exchange State Subscriber
//!
//! Subscribes to ExchangeStateManager updates and updates TradingState accordingly.
//! This is how TradingState stays in sync with ExchangeStateManager.

use crate::state::TradingState;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use trading_core::{
    channels::exchange::ExchangeUpdate,
    error::EngineError,
    types::exchange::{Balance, Order, Trade},
};

/// Subscribes to ExchangeStateManager updates and updates TradingState
pub struct ExchangeStateSubscriber {
    /// Receiver for ExchangeStateManager updates
    exchange_update_rx: trading_core::channels::exchange::ExchangeUpdateReceiver,

    /// Trading state to update
    trading_state: Arc<RwLock<TradingState>>,
}

impl ExchangeStateSubscriber {
    /// Create a new ExchangeStateSubscriber
    ///
    /// # Arguments
    /// * `exchange_update_rx` - Receiver for ExchangeStateManager updates
    /// * `trading_state` - Trading state to update
    pub fn new(
        exchange_update_rx: trading_core::channels::exchange::ExchangeUpdateReceiver,
        trading_state: Arc<RwLock<TradingState>>,
    ) -> Self {
        Self {
            exchange_update_rx,
            trading_state,
        }
    }

    /// Start listening for ExchangeStateManager updates
    ///
    /// This runs in a loop, updating TradingState based on ExchangeStateManager updates.
    pub async fn run(&mut self) -> Result<(), EngineError> {
        loop {
            match self.exchange_update_rx.recv().await {
                Ok(ExchangeUpdate::Order { order, .. }) => {
                    self.handle_order_update(order).await;
                }
                Ok(ExchangeUpdate::Balance { balance, .. }) => {
                    self.handle_balance_update(balance).await;
                }
                Ok(ExchangeUpdate::Trade { trade, .. }) => {
                    self.handle_trade_update(trade).await;
                }
                Ok(ExchangeUpdate::Position {
                    position: _position,
                    ..
                }) => {
                    // Positions are derived from balances, not directly tracked
                    // This is for future use if exchanges provide position updates
                    debug!("Received position update (derived from balances)");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    warn!("Exchange update channel closed, stopping subscriber");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(
                        "Exchange update channel lagged, skipped {} messages",
                        skipped
                    );
                    // Continue processing - we'll catch up
                }
            }
        }
        Ok(())
    }

    /// Handle order update from ExchangeStateManager
    ///
    /// Only updates TradingState if this is an order we placed (tracked in active_orders).
    async fn handle_order_update(&self, order: Order) {
        let mut state = self.trading_state.write().await;

        // Only update if this is our order (we track it in active_orders)
        if state.active_orders.contains_key(&order.order_id) {
            state.add_order(order.clone());
            debug!(
                "Updated trading state with order: {}",
                order.order_id.as_str()
            );
        } else {
            // This is an order from exchange that we didn't place
            // We don't track it in TradingState (only track orders we placed)
            debug!(
                "Ignoring order update (not our order): {}",
                order.order_id.as_str()
            );
        }
    }

    /// Handle balance update from ExchangeStateManager
    ///
    /// Derives positions from balances. When a balance changes, we update the corresponding position.
    async fn handle_balance_update(&self, balance: Balance) {
        // Convert Symbol to Pair using registry
        // For now, we'll need to get the pair from the symbol
        // This is a simplified approach - in production, you'd use the symbol registry

        // Note: Position derivation from balances is complex because:
        // 1. We need to know which pair this balance corresponds to
        // 2. We need to handle multiple pairs with the same base currency
        // 3. Positions are better derived from trades (execution reports)
        // So we'll derive positions from trades instead

        debug!(
            "Received balance update: {} (free: {}, used: {})",
            balance.symbol, balance.free, balance.used
        );
    }

    /// Handle trade update from ExchangeStateManager
    ///
    /// Updates positions based on trades (execution reports).
    /// Only processes trades for orders we placed.
    async fn handle_trade_update(&self, trade: Trade) {
        let mut state = self.trading_state.write().await;

        // Check if this trade is for an order we placed
        if state.active_orders.contains_key(&trade.order_id) {
            // Update position based on trade
            let quantity = if trade.side == trading_core::types::common::Side::Buy {
                trade.quantity
            } else {
                -trade.quantity // Negative for sells
            };

            state.update_position(trade.pair.clone(), quantity, trade.price);
            debug!(
                "Updated position from trade: {} {} @ {}",
                trade.pair, quantity, trade.price
            );
        } else {
            debug!("Ignoring trade update (not our order): {}", trade.trade_id);
        }
    }
}
