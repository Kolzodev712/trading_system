//! Trading State Reconciler
//!
//! Reconciles TradingState with ExchangeStateManager (not with exchange directly).
//! Trading Engine does NOT call exchange directly - that's Exchange Engine's responsibility.
//! This reconciler uses ExchangeStateManager's state (which is the single point of contact with exchange).

use crate::cex::state::TradingState;
use exchange_engine::state::manager::ExchangeStateManager;
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::warn;
use trading_core::{
    error::EngineError,
    types::common::{InstrumentType, Pair},
};

/// Choose the authoritative quantity for target-vs-current.
/// Futures: signed ESM position, or 0 if flat. Spot: base-wallet heuristic.
pub fn authoritative_position_qty(
    instrument_type: InstrumentType,
    esm_position_qty: Option<Decimal>,
    spot_base_balance: Option<Decimal>,
) -> Decimal {
    match instrument_type {
        InstrumentType::Futures | InstrumentType::Derivitives => {
            esm_position_qty.unwrap_or(Decimal::ZERO)
        }
        InstrumentType::Spot => esm_position_qty
            .or(spot_base_balance)
            .unwrap_or(Decimal::ZERO),
    }
}

/// Reconciles TradingState with ExchangeStateManager (not with exchange directly)
///
/// Trading Engine does NOT call exchange directly - that's Exchange Engine's responsibility.
/// This reconciler uses ExchangeStateManager's state (which is the single point of contact with exchange).
#[derive(Clone)]
pub struct PositionReconciler {
    /// Exchange state manager (single point of contact with exchange)
    exchange_state_manager: Arc<ExchangeStateManager>,
    instrument_type: InstrumentType,
}

impl PositionReconciler {
    /// Create a new PositionReconciler
    ///
    /// # Arguments
    /// * `exchange_state_manager` - Exchange state manager (single point of contact with exchange)
    pub fn new(exchange_state_manager: Arc<ExchangeStateManager>) -> Self {
        Self {
            exchange_state_manager,
            instrument_type: InstrumentType::Spot,
        }
    }

    pub fn with_instrument_type(mut self, instrument_type: InstrumentType) -> Self {
        self.instrument_type = instrument_type;
        self
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

        // Reconcile positions. Futures use signed ESM positions (design log 008).
        let base_currency = pair.base.as_str();
        let exchange_qty = authoritative_position_qty(
            self.instrument_type.clone(),
            exchange_state.get_position(pair).map(|p| p.quantity),
            exchange_state.get_balance(base_currency).map(|b| b.total),
        );
        let exchange_avg = exchange_state
            .get_position(pair)
            .map(|p| p.avg_price)
            .unwrap_or(Decimal::ZERO);

        let trading_position_qty = trading_state
            .get_position(pair)
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO);

        let discrepancy = exchange_qty - trading_position_qty;

        let mut reconciled = false;

        tracing::debug!(
            "Trading-level reconciliation for {}: TradingState={}, ExchangeStateManager={}, discrepancy={}",
            pair, trading_position_qty, exchange_qty, discrepancy
        );

        if discrepancy.abs() > Decimal::new(1, 6) {
            warn!(
                "TradingState position discrepancy for {}: internal={}, exchange={}, diff={}",
                pair, trading_position_qty, exchange_qty, discrepancy
            );
            let reconcile_price = if exchange_avg.is_zero() {
                _current_price.unwrap_or(Decimal::ZERO)
            } else {
                exchange_avg
            };
            trading_state.set_position(pair.clone(), exchange_qty, reconcile_price);
            reconciled = true;
            tracing::debug!(
                "TradingState position reconciled for {}: updated to {}",
                pair,
                exchange_qty
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
        for order_id in trading_state.active_orders.keys() {
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

        for order in exchange_orders {
            if !matches!(
                order.status,
                trading_core::types::common::OrderStatus::New
                    | trading_core::types::common::OrderStatus::PartiallyFilled
            ) {
                continue;
            }
            if trading_state.active_orders.contains_key(&order.order_id) {
                continue;
            }
            trading_state.clear_submission_for_order(order);
            trading_state.add_order(order.clone());
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
            exchange_quantity: exchange_qty,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn futures_flat_is_zero_not_wallet() {
        let qty =
            authoritative_position_qty(InstrumentType::Futures, None, Some(Decimal::new(2, 0)));
        assert_eq!(qty, Decimal::ZERO);
    }

    #[test]
    fn futures_uses_signed_esm_qty() {
        let qty = authoritative_position_qty(
            InstrumentType::Futures,
            Some(Decimal::new(-15, 1)),
            Some(Decimal::new(100, 0)),
        );
        assert_eq!(qty, Decimal::new(-15, 1));
    }

    #[test]
    fn spot_falls_back_to_base_balance() {
        let qty = authoritative_position_qty(InstrumentType::Spot, None, Some(Decimal::new(3, 0)));
        assert_eq!(qty, Decimal::new(3, 0));
    }
}
