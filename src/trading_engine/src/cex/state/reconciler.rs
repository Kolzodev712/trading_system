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
///
/// Futures: signed ESM/REST position, or `Some(0)` if the venue reported flat.
/// Spot: base-wallet only. `None` means the wallet is not available yet — not
/// a flat book. A REST `/positions=[]` qty is ignored for Spot.
pub fn authoritative_position_qty(
    instrument_type: InstrumentType,
    esm_position_qty: Option<Decimal>,
    spot_base_balance: Option<Decimal>,
) -> Option<Decimal> {
    match instrument_type {
        InstrumentType::Futures | InstrumentType::Derivitives => {
            Some(esm_position_qty.unwrap_or(Decimal::ZERO))
        }
        InstrumentType::Spot => spot_base_balance,
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

        // Reconcile positions. Futures use signed REST/ESM positions (design log 008).
        // Spot uses the base wallet. An empty REST /positions list is unsupported, not flat.
        let base_currency = pair.base.as_str();
        let esm_qty = exchange_state.get_position(pair).map(|p| p.quantity);
        let rest_qty = exchange_state.rest_position_qty(pair);
        let spot_base = exchange_state.get_balance(base_currency).map(|b| b.total);
        let exchange_qty_opt = authoritative_position_qty(
            self.instrument_type.clone(),
            rest_qty.or(esm_qty),
            spot_base,
        );
        let exchange_avg = exchange_state
            .get_position(pair)
            .map(|p| p.avg_price)
            .unwrap_or(Decimal::ZERO);

        let trading_position_qty = trading_state
            .get_position(pair)
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO);

        let mut reconciled = false;
        let mut exchange_qty = trading_position_qty;
        let mut discrepancy = Decimal::ZERO;

        if matches!(self.instrument_type, InstrumentType::Spot) {
            match exchange_qty_opt {
                None => {
                    tracing::debug!("Spot wallet unavailable for {}; not treating as flat", pair);
                }
                Some(wallet) => {
                    exchange_qty = wallet;
                    discrepancy = exchange_qty - trading_position_qty;
                    tracing::debug!(
                        "Trading-level spot reconciliation for {}: TradingState={}, wallet={}, discrepancy={}",
                        pair, trading_position_qty, exchange_qty, discrepancy
                    );
                    if discrepancy.abs() > Decimal::new(1, 6) {
                        if trading_position_qty.abs() <= Decimal::new(1, 6) {
                            let reconcile_price = if exchange_avg.is_zero() {
                                _current_price.unwrap_or(Decimal::ZERO)
                            } else {
                                exchange_avg
                            };
                            // Zero ESM avg is unknown cost basis, not a $0 entry.
                            trading_state.set_position(pair.clone(), exchange_qty, reconcile_price);
                            reconciled = true;
                        } else {
                            warn!(
                                "Spot position disagrees with base wallet for {}: internal={}, wallet={}",
                                pair, trading_position_qty, exchange_qty
                            );
                            trading_state.halt_new_exposure(format!(
                                "spot position disagrees with base balance: internal={trading_position_qty} wallet={exchange_qty}"
                            ));
                            trading_state.note_position_disagreement(pair.clone());
                        }
                    }
                }
            }
        } else if let Some(rest) = rest_qty {
            exchange_qty = rest;
            discrepancy = rest - trading_position_qty;
            let stream = esm_qty.unwrap_or(Decimal::ZERO);
            if (rest - stream).abs() > Decimal::new(1, 6) {
                warn!(
                    "REST/WS position disagreement for {}: rest={}, ws={}",
                    pair, rest, stream
                );
                trading_state.note_position_disagreement(pair.clone());
            } else {
                let reconcile_price = if exchange_avg.is_zero() {
                    _current_price.unwrap_or(Decimal::ZERO)
                } else {
                    exchange_avg
                };
                trading_state.set_position(pair.clone(), rest, reconcile_price);
                if (rest - trading_position_qty).abs() > Decimal::new(1, 6) {
                    reconciled = true;
                }
            }
        } else {
            exchange_qty = exchange_qty_opt.unwrap_or(Decimal::ZERO);
            discrepancy = exchange_qty - trading_position_qty;
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
        assert_eq!(qty, Some(Decimal::ZERO));
    }

    #[test]
    fn futures_uses_signed_esm_qty() {
        let qty = authoritative_position_qty(
            InstrumentType::Futures,
            Some(Decimal::new(-15, 1)),
            Some(Decimal::new(100, 0)),
        );
        assert_eq!(qty, Some(Decimal::new(-15, 1)));
    }

    #[test]
    fn spot_falls_back_to_base_balance() {
        let qty = authoritative_position_qty(InstrumentType::Spot, None, Some(Decimal::new(3, 0)));
        assert_eq!(qty, Some(Decimal::new(3, 0)));
    }

    #[test]
    fn spot_empty_rest_positions_are_not_flat() {
        let qty = authoritative_position_qty(
            InstrumentType::Spot,
            Some(Decimal::ZERO),
            Some(Decimal::new(2, 3)),
        );
        assert_eq!(qty, Some(Decimal::new(2, 3)));
    }

    #[test]
    fn spot_missing_wallet_is_unsupported() {
        let qty = authoritative_position_qty(InstrumentType::Spot, Some(Decimal::ZERO), None);
        assert_eq!(qty, None);
    }
}
