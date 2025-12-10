//! Cancellation manager - manages cancellation logic to prevent system wipeouts
use crate::state::TradingState;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::error::EngineError;
use trading_core::traits::TradingClient;
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
                            None,                                           // asset_service
                            trading_core::types::common::Exchange::Binance, // exchange (not used for cancellation)
                            rust_decimal::Decimal::new(1, 5),               // config_min_order_size
                            rust_decimal::Decimal::new(1, 5),               // config_step_size
                            rust_decimal::Decimal::new(1, 2),               // config_tick_size
                            rust_decimal::Decimal::new(5, 0),               // config_min_notional
                        );
                        executor.cancel_level_orders(level).await?;
                    }
                }
                CancellationAction::CancelLevelGroup(group) => {
                    // Cancel all orders for levels in this group
                    let level_ids: Vec<_> = level_manager
                        .get_all_levels()
                        .iter()
                        .filter_map(|level| {
                            if level.config.group.as_ref() == Some(&group) {
                                Some(level.config.id.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    for level_id in level_ids {
                        if let Some(level) = level_manager.get_level(&level_id) {
                            // For cancellation, we don't need AssetService - use minimal defaults
                            let executor = crate::levels::LevelExecutor::new(
                                Arc::clone(&trading_client),
                                None,                                           // asset_service
                                trading_core::types::common::Exchange::Binance, // exchange (not used for cancellation)
                                rust_decimal::Decimal::new(1, 5), // config_min_order_size
                                rust_decimal::Decimal::new(1, 5), // config_step_size
                                rust_decimal::Decimal::new(1, 2), // config_tick_size
                                rust_decimal::Decimal::new(5, 0), // config_min_notional
                            );
                            executor.cancel_level_orders(level).await?;
                        }
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
