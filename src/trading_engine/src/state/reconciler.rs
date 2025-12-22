//! Trading State Reconciler
//!
//! Reconciles TradingState with ExchangeStateManager (not with exchange directly).
//! Trading Engine does NOT call exchange directly - that's Exchange Engine's responsibility.
//! This reconciler uses ExchangeStateManager's state (which is the single point of contact with exchange).

use crate::state::TradingState;
use exchange_engine::state::manager::ExchangeStateManager;
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::warn;
use trading_core::{error::EngineError, types::common::Pair};

/// Reconciles TradingState with ExchangeStateManager (not with exchange directly)
///
/// Trading Engine does NOT call exchange directly - that's Exchange Engine's responsibility.
/// This reconciler uses ExchangeStateManager's state (which is the single point of contact with exchange).
#[derive(Clone)]
pub struct PositionReconciler {
    /// Exchange state manager (single point of contact with exchange)
    exchange_state_manager: Arc<ExchangeStateManager>,
}

impl PositionReconciler {
    /// Create a new PositionReconciler
    ///
    /// # Arguments
    /// * `exchange_state_manager` - Exchange state manager (single point of contact with exchange)
    pub fn new(exchange_state_manager: Arc<ExchangeStateManager>) -> Self {
        Self {
            exchange_state_manager,
        }
    }

    /// Reconcile TradingState with ExchangeStateManager (not with exchange directly)
    ///
    /// Compares TradingState positions and active orders with ExchangeStateManager's view.
    /// Updates TradingState to resolve discrepancies.
    ///
    /// # Arguments
    /// * `trading_state` - Trading state to reconcile
    /// * `pair` - Trading pair to reconcile
    /// * `current_price` - Optional current market price for position updates
    pub async fn reconcile(
        &self,
        trading_state: &mut TradingState,
        pair: &Pair,
        _current_price: Option<Decimal>,
    ) -> Result<ReconciliationResult, EngineError> {
        tracing::debug!("Starting trading-level reconciliation for {}", pair);

        let exchange_state_arc = self.exchange_state_manager.get_state();
        let exchange_state = exchange_state_arc.read().await;

        // Reconcile positions
        // Get balance for base currency from ExchangeStateManager
        let base_currency = pair.base.as_str();
        let exchange_balance = exchange_state
            .get_balance(base_currency)
            .map(|b| b.total)
            .unwrap_or(Decimal::ZERO);

        let trading_position_qty = trading_state
            .get_position(pair)
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO);

        let discrepancy = exchange_balance - trading_position_qty;

        let mut reconciled = false;

        tracing::debug!(
            "Trading-level reconciliation for {}: TradingState={}, ExchangeStateManager={}, discrepancy={}",
            pair, trading_position_qty, exchange_balance, discrepancy
        );

        if discrepancy.abs() > Decimal::new(1, 6) {
            warn!(
                "TradingState position discrepancy for {}: internal={}, exchange={}, diff={}",
                pair, trading_position_qty, exchange_balance, discrepancy
            );
            // Update TradingState to match ExchangeStateManager
            // Calculate delta: exchange_balance - current_position
            let delta = exchange_balance - trading_position_qty;
            if delta != Decimal::ZERO {
                // Use a reasonable price for reconciliation (use current price if available, else 0)
                let reconcile_price = _current_price.unwrap_or(Decimal::ZERO);
                trading_state.update_position(
                    pair.clone(),
                    delta, // Delta, not absolute value
                    reconcile_price,
                );
            }
            reconciled = true;
            tracing::debug!(
                "TradingState position reconciled for {}: updated to {}",
                pair,
                exchange_balance
            );
        } else {
            tracing::debug!(
                "TradingState position for {} is consistent with ExchangeStateManager",
                pair
            );
        }

        // Reconcile active orders (ensure all active orders in TradingState are known by ExchangeStateManager)
        let exchange_orders = exchange_state.get_orders_for_pair(pair);
        let exchange_order_ids: std::collections::HashSet<&trading_core::types::common::OrderId> =
            exchange_orders.iter().map(|o| &o.order_id).collect();

        tracing::debug!(
            "Trading-level reconciliation for {}: TradingState has {} active orders, ExchangeStateManager has {} orders",
            pair,
            trading_state.active_orders.len(),
            exchange_orders.len()
        );

        let mut orders_to_remove = Vec::new();
        for (order_id, _order) in trading_state.active_orders.iter() {
            if !exchange_order_ids.contains(order_id) {
                warn!(
                    "TradingState has active order {:?} for {} not found in ExchangeStateManager. Removing.",
                    order_id, pair
                );
                orders_to_remove.push(order_id.clone());
            }
        }
        for order_id in orders_to_remove {
            trading_state.remove_order(&order_id);
            reconciled = true;
        }

        if reconciled {
            tracing::debug!(
                "Trading-level reconciliation for {} completed: state updated",
                pair
            );
        } else {
            tracing::debug!(
                "Trading-level reconciliation for {} completed: no discrepancies found",
                pair
            );
        }

        Ok(ReconciliationResult {
            symbol: pair.clone(),
            internal_quantity: trading_position_qty,
            exchange_quantity: exchange_balance,
            discrepancy,
            reconciled,
        })
    }
}

/// Result of reconciliation operation
#[derive(Debug, Clone)]
pub struct ReconciliationResult {
    /// Trading pair that was reconciled
    pub symbol: Pair,
    /// Internal quantity (from TradingState)
    pub internal_quantity: Decimal,
    /// Exchange quantity (from ExchangeStateManager)
    pub exchange_quantity: Decimal,
    /// Discrepancy between internal and exchange quantities
    pub discrepancy: Decimal,
    /// Whether reconciliation was performed (discrepancy was found and fixed)
    pub reconciled: bool,
}
