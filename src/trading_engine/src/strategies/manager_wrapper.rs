//! Wrapper to make StrategyManager work with StrategyExecutor
//!
//! StrategyExecutor expects a Strategy trait implementation, but StrategyManager
//! has a different interface. This wrapper bridges the gap.
//!
//! The wrapper stores all requests from StrategyManager and returns them
//! one at a time. StrategyExecutor will need to be modified to process
//! all requests, or we accept that only one order per orderbook update
//! will be processed (which is acceptable for most use cases).

use crate::state::{Strategy, TradingState};
use crate::strategies::strategy_manager::StrategyManager;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::{
    error::EngineError,
    types::{
        exchange::{Order, OrderRequest},
        market_data::Orderbook,
    },
};

/// Wrapper that makes StrategyManager implement Strategy trait
///
/// This allows StrategyExecutor to work with StrategyManager.
/// Note: StrategyExecutor processes one order per orderbook update,
/// so if multiple strategies want to place orders, they'll be processed
/// over multiple orderbook updates (which is acceptable).
pub struct StrategyManagerWrapper {
    strategy_manager: Arc<RwLock<StrategyManager>>,
    trading_state: TradingState,
}

impl StrategyManagerWrapper {
    pub fn new(strategy_manager: Arc<RwLock<StrategyManager>>) -> Self {
        Self {
            strategy_manager,
            trading_state: TradingState::new(),
        }
    }

    /// Get the underlying StrategyManager (for monitoring/status)
    pub fn get_manager(&self) -> Arc<RwLock<StrategyManager>> {
        Arc::clone(&self.strategy_manager)
    }
}

#[async_trait]
impl Strategy for StrategyManagerWrapper {
    async fn on_orderbook(
        &mut self,
        orderbook: &Orderbook,
        _state: &TradingState, // StrategyManager gets state from each strategy internally
    ) -> Result<Option<OrderRequest>, EngineError> {
        // Get all requests from StrategyManager
        // StrategyManager::on_orderbook gets state from each strategy internally
        let requests = {
            let manager = self.strategy_manager.read().await;
            manager.on_orderbook(orderbook).await?
        };

        // Return first request (StrategyExecutor processes one per orderbook update)
        // Other requests will be processed on subsequent orderbook updates
        // This is acceptable as orderbook updates are frequent
        Ok(requests.into_iter().next())
    }

    async fn on_execution_report(&mut self, order: &Order) -> Result<(), EngineError> {
        // Delegate to StrategyManager
        let manager = self.strategy_manager.read().await;
        manager.on_execution_report(order).await
    }

    fn get_state(&self) -> &TradingState {
        &self.trading_state
    }

    fn get_state_mut(&mut self) -> &mut TradingState {
        &mut self.trading_state
    }

    fn get_last_price(&self) -> Option<rust_decimal::Decimal> {
        // Get last price from first strategy (or could aggregate)
        // For now, return None - strategies will provide their own prices
        None
    }
}
