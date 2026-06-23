//! Trading State Manager
//!
//! Manages trading-specific state (positions, active orders) derived from ExchangeStateManager.
//! Does NOT call exchange directly - relies on ExchangeStateManager via channel subscriptions.

use crate::cex::state::reconciler::{PositionReconciler, ReconciliationResult};
use crate::cex::state::subscriber::ExchangeStateSubscriber;
use crate::cex::state::TradingState;
use exchange_engine::state::manager::ExchangeStateManager;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::{
    error::EngineError, types::common::Pair, venue::cex::channels::CexVenueUpdateReceiver,
};

/// Manages trading-specific state (positions, active orders)
///
/// This state is derived from ExchangeStateManager via channel subscriptions.
/// Trading Engine does NOT call exchange directly - that's Exchange Engine's responsibility.
pub struct TradingStateManager {
    /// In-memory trading state (positions, active orders)
    state: Arc<RwLock<TradingState>>,

    /// Subscriber to ExchangeStateManager updates
    subscriber: Arc<tokio::sync::Mutex<ExchangeStateSubscriber>>,

    /// Reconciler for TradingState vs ExchangeStateManager reconciliation
    reconciler: PositionReconciler,
}

impl TradingStateManager {
    /// Create a new TradingStateManager
    ///
    /// # Arguments
    /// * `exchange_update_rx` - Receiver for ExchangeStateManager updates
    /// * `exchange_state_manager` - ExchangeStateManager to reconcile with
    pub fn new(
        exchange_update_rx: CexVenueUpdateReceiver,
        exchange_state_manager: Arc<ExchangeStateManager>,
    ) -> Self {
        let state = Arc::new(RwLock::new(TradingState::new()));
        let subscriber = Arc::new(tokio::sync::Mutex::new(ExchangeStateSubscriber::new(
            exchange_update_rx,
            state.clone(),
        )));
        let reconciler = PositionReconciler::new(exchange_state_manager);

        Self {
            state,
            subscriber,
            reconciler,
        }
    }

    /// Start the subscriber to listen for ExchangeStateManager updates
    ///
    /// This should be spawned in a background task.
    pub async fn start_subscriber(&self) -> Result<(), EngineError> {
        let mut subscriber = self.subscriber.lock().await;
        subscriber.run().await
    }

    /// Get the trading state (for strategies/levels)
    pub fn get_state(&self) -> Arc<RwLock<TradingState>> {
        self.state.clone()
    }

    /// Reconcile TradingState with ExchangeStateManager
    ///
    /// This reconciles positions and orders, but does NOT call exchange directly.
    /// It uses ExchangeStateManager's state (which is the single point of contact with exchange).
    ///
    /// # Arguments
    /// * `pair` - Trading pair to reconcile
    /// * `current_price` - Optional current market price for position updates
    pub async fn reconcile(
        &self,
        pair: &Pair,
        current_price: Option<rust_decimal::Decimal>,
    ) -> Result<ReconciliationResult, EngineError> {
        let mut state = self.state.write().await;
        self.reconciler
            .reconcile(&mut state, pair, current_price)
            .await
    }

    /// Add an order to trading state (called when we place an order)
    ///
    /// This tracks orders that we placed (not all orders from exchange).
    pub async fn add_order(&self, order: trading_core::types::exchange::Order) {
        let mut state = self.state.write().await;
        state.add_order(order);
    }

    /// Update order status (called from subscriber when ExchangeStateManager updates)
    pub async fn update_order(&self, order: trading_core::types::exchange::Order) {
        let mut state = self.state.write().await;
        // Only update if this is our order
        if state.active_orders.contains_key(&order.order_id) {
            state.add_order(order); // add_order handles updates too
        }
    }

    /// Remove order (called when order is cancelled/filled)
    pub async fn remove_order(&self, order_id: &trading_core::types::common::OrderId) {
        let mut state = self.state.write().await;
        state.remove_order(order_id);
    }
}
