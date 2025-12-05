use super::strategy::Strategy;
use core_logic::{
    data::base_data::Pair,
    error::EngineError,
    data::public_data::Orderbook,
    data::private_data::Order,
};
use std::sync::Arc;
use tokio::sync::RwLock;

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

    pub async fn on_orderbook(&self, orderbook: &Orderbook) -> Result<Vec<core_logic::data::private_data::OrderRequest>, EngineError> {
        let mut requests = Vec::new();
        
        for strategy in &self.strategies {
            let mut strat = strategy.write().await;
            let state = strat.get_state();
            if let Some(req) = strat.on_orderbook(orderbook, state).await? {
                requests.push(req);
            }
        }
        
        Ok(requests)
    }

    pub async fn on_execution_report(&self, order: &Order) -> Result<(), EngineError> {
        for strategy in &self.strategies {
            let mut strat = strategy.write().await;
            strat.on_execution_report(order, strat.get_state_mut()).await?;
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

