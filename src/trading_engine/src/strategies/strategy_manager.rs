use crate::state::Strategy;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::{error::EngineError, types::exchange::Order, types::market_data::Orderbook};

/// Manages multiple strategies
pub struct StrategyManager {
    strategies: Vec<Arc<RwLock<dyn Strategy>>>,
}

impl StrategyManager {
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
        }
    }

    pub fn add_strategy(&mut self, strategy: Arc<RwLock<dyn Strategy>>) {
        self.strategies.push(strategy);
    }

    pub async fn on_orderbook(
        &self,
        orderbook: &Orderbook,
    ) -> Result<Vec<trading_core::types::exchange::OrderRequest>, EngineError> {
        let mut requests = Vec::new();

        for strategy in &self.strategies {
            let mut strat = strategy.write().await;
            let state = strat.get_state().clone();
            if let Some(req) = strat.on_orderbook(orderbook, &state).await? {
                requests.push(req);
            }
        }

        Ok(requests)
    }

    pub async fn on_execution_report(&self, order: &Order) -> Result<(), EngineError> {
        for strategy in &self.strategies {
            let mut strat_guard = strategy.write().await;
            strat_guard.on_execution_report(order).await?;
        }
        Ok(())
    }

    pub fn get_strategies(&self) -> &[Arc<RwLock<dyn Strategy>>] {
        &self.strategies
    }
}

impl Default for StrategyManager {
    fn default() -> Self {
        Self::new()
    }
}
