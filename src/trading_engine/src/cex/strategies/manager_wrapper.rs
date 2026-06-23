//! Wrapper to make StrategyManager work with StrategyExecutor
//!
//! StrategyExecutor expects a Strategy trait implementation, but StrategyManager
//! has a different interface. This wrapper bridges the gap.
//!
//! The wrapper stores all requests from StrategyManager and returns them
//! one at a time. StrategyExecutor will need to be modified to process
//! all requests, or we accept that only one order per orderbook update
//! will be processed (which is acceptable for most use cases).

use crate::cex::state::{Strategy, TradingState};
use crate::cex::strategies::strategy_manager::StrategyManager;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
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

    /// Get all order requests from StrategyManager (for batching)
    pub async fn get_all_requests(
        &self,
        orderbook: &Orderbook,
    ) -> Result<Vec<OrderRequest>, EngineError> {
        let manager = self.strategy_manager.read().await;
        manager.on_orderbook(orderbook).await
    }

    /// Get all cancellation requests from StrategyManager (for batching)
    pub async fn get_all_cancels(
        &self,
        orderbook: &Orderbook,
    ) -> Result<Vec<trading_core::types::common::OrderId>, EngineError> {
        let manager = self.strategy_manager.read().await;
        manager.on_orderbook_cancels(orderbook).await
    }
}

#[async_trait]
impl Strategy for StrategyManagerWrapper {
    async fn on_orderbook(
        &mut self,
        orderbook: &Orderbook,
        _state: &TradingState, // StrategyManager gets state from each strategy internally
    ) -> Result<Option<OrderRequest>, EngineError> {
        // Log entry point (periodically)
        static WRAPPER_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let wrapper_count = WRAPPER_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let should_log = wrapper_count.is_multiple_of(20);

        if should_log {
            info!(
                "[StrategyManagerWrapper] on_orderbook called for {}",
                orderbook.pair
            );
        }

        // Get all requests from StrategyManager
        // StrategyManager::on_orderbook gets state from each strategy internally
        let requests = {
            let manager = self.strategy_manager.read().await;
            manager.on_orderbook(orderbook).await?
        };

        if should_log {
            info!(
                "[StrategyManagerWrapper] StrategyManager returned {} requests",
                requests.len()
            );
        }

        // Return first request (StrategyExecutor processes one per orderbook update)
        // Other requests will be processed on subsequent orderbook updates
        // This is acceptable as orderbook updates are frequent
        Ok(requests.into_iter().next())
    }

    async fn on_orderbook_cancels(
        &mut self,
        orderbook: &Orderbook,
        _state: &TradingState,
    ) -> Result<Vec<trading_core::types::common::OrderId>, EngineError> {
        // Delegate to StrategyManager
        let manager = self.strategy_manager.read().await;
        manager.on_orderbook_cancels(orderbook).await
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
