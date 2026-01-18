//! Cancellation manager - manages cancellation logic to prevent system wipeouts
use crate::state::TradingState;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::error::EngineError;
use trading_core::traits::TradingClient;
use trading_core::types::exchange::BulkCancelRequest;
use trading_core::types::exchange::{Balance, OrderRequest};

/// Cancellation condition
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancellationCondition {
    /// Cancel all orders if daily loss limit reached
    DailyLossLimitReached,

    /// Cancel all orders if total position exceeds limit
    TotalPositionExceeded(Decimal),

    /// Cancel all orders if balance below threshold
    BalanceBelowThreshold(Decimal),

    /// Cancel all orders if exchange connection lost
    ExchangeConnectionLost,

    /// Cancel orders for a specific level group
    LevelGroupPaused(SmolStr),
}

/// Cancellation action
#[derive(Debug, Clone)]
pub enum CancellationAction {
    /// Cancel all orders
    CancelAll,

    /// Cancel orders for a specific level
    CancelLevel(SmolStr),

    /// Cancel orders for a level group
    CancelLevelGroup(SmolStr),
}

/// Cancellation manager
pub struct CancellationManager {
    conditions: Vec<CancellationCondition>,
}

impl CancellationManager {
    /// Create a new cancellation manager
    pub fn new(conditions: Vec<CancellationCondition>) -> Self {
        Self { conditions }
    }

    /// Check cancellation conditions
    pub async fn check_conditions(
        &self,
        _state: &TradingState,
        balances: &[Balance],
        daily_pnl: Decimal,
    ) -> Vec<CancellationAction> {
        let mut actions = Vec::new();

        for condition in &self.conditions {
            match condition {
                CancellationCondition::DailyLossLimitReached => {
                    // TODO: Check daily loss limit
                    // For now, simplified check
                    if daily_pnl < Decimal::ZERO {
                        // Could trigger cancellation if loss is too high
                        // This will be fully implemented in Proposal 07 (Risk System)
                    }
                }
                CancellationCondition::TotalPositionExceeded(_limit) => {
                    // TODO: Calculate total position across all symbols
                    // Simplified for now
                }
                CancellationCondition::BalanceBelowThreshold(threshold) => {
                    let total_balance: Decimal = balances.iter().map(|b| b.free).sum();

                    if total_balance < *threshold {
                        actions.push(CancellationAction::CancelAll);
                    }
                }
                CancellationCondition::ExchangeConnectionLost => {
                    // This would be checked externally
                }
                CancellationCondition::LevelGroupPaused(group) => {
                    actions.push(CancellationAction::CancelLevelGroup(group.clone()));
                }
            }
        }

        actions
    }

    /// Execute cancellation actions
    pub async fn execute_cancellations(
        &self,
        actions: Vec<CancellationAction>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        level_manager: &crate::levels::LevelManager,
    ) -> Result<(), EngineError> {
        for action in actions {
            match action {
                CancellationAction::CancelAll => {
                    // Cancel all open orders
                    // This would require access to all active orders
                    // Simplified for now - would need integration with order tracking
                    tracing::warn!(
                        "CancelAll action - would cancel all orders (not fully implemented yet)"
                    );
                }
                CancellationAction::CancelLevel(level_id) => {
                    if let Some(level) = level_manager.get_level(&level_id) {
                        // For cancellation, we don't need AssetService - use minimal defaults
                        let executor = crate::levels::LevelExecutor::new(
                            Arc::clone(&trading_client),
                            None,                                              // asset_service
                            trading_core::types::common::Exchange::Binance, // exchange (not used for cancellation)
                            trading_core::types::common::InstrumentType::Spot, // Default to Spot for cancellation
                            rust_decimal::Decimal::new(1, 5), // config_min_order_size
                            rust_decimal::Decimal::new(1, 5), // config_step_size
                            rust_decimal::Decimal::new(1, 2), // config_tick_size
                            rust_decimal::Decimal::new(5, 0), // config_min_notional
                        );
                        executor.cancel_level_orders(level).await?;
                    }
                }
                CancellationAction::CancelLevelGroup(group) => {
                    // Collect all order IDs from all levels in this group
                    let mut all_order_ids = Vec::new();
                    let levels_in_group: Vec<_> = level_manager
                        .get_all_levels()
                        .iter()
                        .filter(|level| level.config.group.as_ref() == Some(&group))
                        .cloned()
                        .collect();

                    for level in &levels_in_group {
                        all_order_ids.extend(level.active_orders.iter().cloned());
                    }

                    if !all_order_ids.is_empty() {
                        tracing::info!(
                            "Cancelling {} order(s) from {} level(s) in group {}",
                            all_order_ids.len(),
                            levels_in_group.len(),
                            group
                        );

                        let bulk_request = BulkCancelRequest {
                            order_ids: all_order_ids.clone(),
                            pair: None,
                        };
                        let trading_client_guard = trading_client.write().await;
                        let bulk_response =
                            trading_client_guard.bulk_cancel_orders(bulk_request).await;

                        let mut cancelled_count = 0;
                        let mut failed_count = 0;

                        match bulk_response {
                            Ok(response) => {
                                for result in response.results {
                                    if result.success {
                                        cancelled_count += 1;
                                    } else {
                                        failed_count += 1;
                                        tracing::warn!(
                                            "Failed to cancel order in group {}: {}",
                                            group,
                                            result
                                                .error
                                                .unwrap_or_else(|| "Unknown error".to_string())
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                failed_count = all_order_ids.len();
                                tracing::error!(
                                    "Bulk cancel orders failed for group {}: {}",
                                    group,
                                    e
                                );
                            }
                        }

                        tracing::info!(
                            "Group {} cancellation complete: {} cancelled, {} failed",
                            group,
                            cancelled_count,
                            failed_count
                        );

                        if failed_count > 0 {
                            return Err(EngineError::CustomError(anyhow::anyhow!(
                                "Failed to cancel {} order(s) in group {}",
                                failed_count,
                                group
                            )));
                        }
                    } else {
                        tracing::debug!("No orders to cancel for group {}", group);
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for CancellationManager {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}
