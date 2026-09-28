//! Exchange State Subscriber
//!
//! Subscribes to ExchangeStateManager updates and updates TradingState accordingly.
//! This is how TradingState stays in sync with ExchangeStateManager.

use crate::cex::state::ledger::ExecutionLedger;
use crate::cex::state::TradingState;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use tracing::{debug, warn};
use trading_core::{
    error::EngineError,
    types::common::{InstrumentType, OrderStatus},
    types::exchange::{Balance, Order, Trade},
    venue::cex::channels::CexVenueUpdate,
};

/// Subscribes to ExchangeStateManager updates and updates TradingState
pub struct ExchangeStateSubscriber {
    /// Receiver for ExchangeStateManager updates
    exchange_update_rx: trading_core::venue::cex::channels::CexVenueUpdateReceiver,

    /// Trading state to update
    trading_state: Arc<RwLock<TradingState>>,
    instrument_type: InstrumentType,
    execution_ledger: Arc<Mutex<Option<Arc<ExecutionLedger>>>>,
}

impl ExchangeStateSubscriber {
    /// Create a new ExchangeStateSubscriber
    ///
    /// # Arguments
    /// * `exchange_update_rx` - Receiver for ExchangeStateManager updates
    /// * `trading_state` - Trading state to update
    pub fn new(
        exchange_update_rx: trading_core::venue::cex::channels::CexVenueUpdateReceiver,
        trading_state: Arc<RwLock<TradingState>>,
    ) -> Self {
        Self {
            exchange_update_rx,
            trading_state,
            instrument_type: InstrumentType::Spot,
            execution_ledger: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_instrument_type(mut self, instrument_type: InstrumentType) -> Self {
        self.instrument_type = instrument_type;
        self
    }

    pub fn with_execution_ledger_slot(
        mut self,
        slot: Arc<Mutex<Option<Arc<ExecutionLedger>>>>,
    ) -> Self {
        self.execution_ledger = slot;
        self
    }

    /// Start listening for ExchangeStateManager updates
    ///
    /// This runs in a loop, updating TradingState based on ExchangeStateManager updates.
    pub async fn run(&mut self) -> Result<(), EngineError> {
        loop {
            match self.exchange_update_rx.recv().await {
                Ok(CexVenueUpdate::Order { order, .. }) => {
                    self.handle_order_update(order).await;
                }
                Ok(CexVenueUpdate::Balance { balance, .. }) => {
                    self.handle_balance_update(balance).await;
                }
                Ok(CexVenueUpdate::Trade { trade, .. }) => {
                    self.handle_trade_update(trade).await;
                }
                Ok(CexVenueUpdate::Position { position, .. }) => {
                    self.handle_position_update(position).await;
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
    /// Terminal statuses (Filled, Canceled, Rejected, Expired) remove the order from active_orders.
    async fn handle_order_update(&self, order: Order) {
        let mut state = self.trading_state.write().await;
        let tracked = state.active_orders.contains_key(&order.order_id);

        // Private stream is this account. Apply fills even if the place ack
        // has not been written to TSM yet (demo fills can beat add_order).
        state.clear_submission_for_order(&order);
        state.apply_fill(&order);

        let is_terminal = matches!(
            order.status,
            OrderStatus::Filled
                | OrderStatus::Canceled
                | OrderStatus::Rejected
                | OrderStatus::Expired
        );
        if is_terminal {
            state.remove_order(&order.order_id);
            debug!(
                "Removed order from trading state (terminal status {:?}): {}",
                order.status,
                order.order_id.as_str()
            );
        } else if tracked
            || matches!(
                order.status,
                OrderStatus::New | OrderStatus::PartiallyFilled
            )
        {
            state.add_order(order.clone());
            debug!(
                "Updated trading state with order: {}",
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

    async fn handle_position_update(&self, position: trading_core::types::exchange::Position) {
        if !matches!(
            self.instrument_type,
            InstrumentType::Futures | InstrumentType::Derivitives
        ) {
            debug!("Ignoring position update in spot mode");
            return;
        }
        let mut state = self.trading_state.write().await;
        state.apply_stream_position(
            position.instrument.pair.clone(),
            position.quantity,
            position.avg_price,
        );
        debug!(
            "Updated trading state from exchange position: {} qty={}",
            position.instrument.pair, position.quantity
        );
    }

    /// Handle trade update from ExchangeStateManager
    ///
    /// Economic fills go to the execution ledger for every instrument type.
    /// Spot still derives position from trades; futures position is REST/WS truth.
    async fn handle_trade_update(&self, trade: Trade) {
        self.record_ledger_trade(&trade).await;

        if matches!(
            self.instrument_type,
            InstrumentType::Futures | InstrumentType::Derivitives
        ) {
            debug!(
                "Ignoring trade-derived position in futures mode (ESM position is authoritative)"
            );
            return;
        }
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

    async fn record_ledger_trade(&self, trade: &Trade) {
        let ledger = self
            .execution_ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let Some(ledger) = ledger else {
            return;
        };
        if let Err(err) = ledger.record_trade(trade) {
            warn!("execution ledger write failed: {err}");
            let mut state = self.trading_state.write().await;
            state.halt_new_exposure(format!("execution ledger write failed: {err}"));
        }
    }
}
