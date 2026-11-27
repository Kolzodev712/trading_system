//! Exchange State Subscriber
//!
//! Subscribes to ExchangeStateManager updates and updates TradingState accordingly.
//! This is how TradingState stays in sync with ExchangeStateManager.

use super::operator_flatten::{
    settle_operator_flatten_on_order, settle_operator_flatten_when_local_flat,
};
use super::{SubmissionJournal, TradingState};
use crate::cex::state::ledger::ExecutionLedger;
use std::sync::atomic::{AtomicBool, Ordering};
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
    submission_journal: Arc<Mutex<Option<Arc<SubmissionJournal>>>>,
    /// Set when the lossy broadcast dropped venue updates (possibly fills); the
    /// owner's event loop polls it and reconciles against ExchangeStateManager.
    needs_reconcile: Arc<AtomicBool>,
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
            submission_journal: Arc::new(Mutex::new(None)),
            needs_reconcile: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Share the reconcile-request flag with the owner (see `TradingStateManager`).
    pub fn with_needs_reconcile_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.needs_reconcile = flag;
        self
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

    pub fn with_submission_journal_slot(
        mut self,
        slot: Arc<Mutex<Option<Arc<SubmissionJournal>>>>,
    ) -> Self {
        self.submission_journal = slot;
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
                        "Exchange update channel lagged, skipped {} messages; requesting reconcile",
                        skipped
                    );
                    trading_core::metrics::record_broadcast_lag_cex(skipped);
                    // Skipped messages may include fills/cancels that TradingState never saw.
                    // Flag a reconcile against ExchangeStateManager (which saw them all).
                    self.needs_reconcile.store(true, Ordering::Release);
                }
            }
        }
        Ok(())
    }

    /// Handle order update from ExchangeStateManager
    ///
    /// Only updates TradingState if this is an order we placed (tracked in active_orders).
    /// Terminal statuses (Filled, Canceled, Rejected, Expired) remove the order from active_orders.
    pub(crate) async fn handle_order_update(&self, order: Order) {
        let journal = self
            .submission_journal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
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
        if let Some(journal) = journal {
            if let Err(err) = settle_operator_flatten_on_order(&journal, &mut state, &order) {
                warn!("operator flatten settle on order failed: {err}");
                state.halt_new_exposure(format!("operator flatten settle on order failed: {err}"));
            }
            if let Err(err) = settle_operator_flatten_when_local_flat(&journal, &mut state) {
                warn!("operator flatten settle on flat failed: {err}");
                state.halt_new_exposure(format!("operator flatten settle on flat failed: {err}"));
            }
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
        let journal = self
            .submission_journal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
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
        if let Some(journal) = journal {
            if let Err(err) = settle_operator_flatten_when_local_flat(&journal, &mut state) {
                warn!("operator flatten settle on position update failed: {err}");
                state.halt_new_exposure(format!(
                    "operator flatten settle on position update failed: {err}"
                ));
            }
        }
    }

    /// Handle trade update from ExchangeStateManager
    ///
    /// Economic fills go to the execution ledger for every instrument type.
    /// Spot position quantity changes once per unique trade, using the execution price.
    /// Futures position is REST/WS truth.
    pub(crate) async fn handle_trade_update(&self, trade: Trade) {
        let ledger_new = self.record_ledger_trade(&trade).await;

        if matches!(
            self.instrument_type,
            InstrumentType::Futures | InstrumentType::Derivitives
        ) {
            debug!(
                "Ignoring trade-derived position in futures mode (ESM position is authoritative)"
            );
            return;
        }
        if !ledger_new {
            return;
        }
        let mut state = self.trading_state.write().await;
        if state.apply_unique_trade(&trade) {
            debug!(
                "Updated position from unique trade: {} {} @ {}",
                trade.pair, trade.quantity, trade.price
            );
        }
    }

    async fn record_ledger_trade(&self, trade: &Trade) -> bool {
        let ledger = self
            .execution_ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let Some(ledger) = ledger else {
            return true;
        };
        match ledger.record_trade(trade) {
            Ok(newly) => newly,
            Err(err) => {
                warn!("execution ledger write failed: {err}");
                let mut state = self.trading_state.write().await;
                state.halt_new_exposure(format!("execution ledger write failed: {err}"));
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::types::common::{Instrument, OrderId, Pair, Side, Symbol};
    use trading_core::venue::cex::channels::cex_venue_update_channel;

    fn order(filled: Decimal) -> Order {
        Order {
            order_id: OrderId::new("ord-a".into()),
            client_order_id: Some(smol_str::SmolStr::new("cid-a")),
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(1000, 0),
            amount: Decimal::new(1, 2),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::PartiallyFilled,
            filled_amount: filled,
        }
    }

    fn trade(id: &str, qty: Decimal, price: Decimal) -> Trade {
        Trade {
            trade_id: id.into(),
            order_id: OrderId::new("ord-a".into()),
            pair: Pair::new("BTC", "USDT"),
            price,
            quantity: qty,
            side: Side::Buy,
            timestamp: 1,
            fee: Decimal::ZERO,
            fee_currency: Symbol::from("USDT"),
        }
    }

    #[tokio::test]
    async fn order_plus_trade_repeats_and_two_partials() {
        let (_tx, rx) = cex_venue_update_channel(8);
        let state = Arc::new(RwLock::new(TradingState::new()));
        let sub = ExchangeStateSubscriber::new(rx, state.clone());
        let pair = Pair::new("BTC", "USDT");

        sub.handle_order_update(order(Decimal::new(1, 3))).await;
        sub.handle_trade_update(trade("t1", Decimal::new(1, 3), Decimal::new(1000, 0)))
            .await;
        sub.handle_order_update(order(Decimal::new(1, 3))).await;
        sub.handle_trade_update(trade("t1", Decimal::new(1, 3), Decimal::new(1000, 0)))
            .await;
        {
            let st = state.read().await;
            assert_eq!(st.position_qty(&pair), Decimal::new(1, 3));
            assert!(!st.blocks_new_exposure(&pair));
        }

        sub.handle_order_update(order(Decimal::new(2, 3))).await;
        sub.handle_trade_update(trade("t2", Decimal::new(1, 3), Decimal::new(1020, 0)))
            .await;
        let st = state.read().await;
        assert_eq!(st.position_qty(&pair), Decimal::new(2, 3));
        assert_eq!(
            st.get_position(&pair).unwrap().avg_price,
            Decimal::new(1010, 0)
        );
    }
}
