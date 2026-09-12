//! Operator strategy config (no secrets). See design log 012.

use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct OperatorFields {
    pub strategy_type: String,
    pub symbol: String,
    pub position_size: String,
    pub max_position: String,
    pub buy_threshold_percent: String,
    pub sell_threshold_percent: String,
    pub stop_loss_percent: String,
    pub take_profit_percent: String,
    pub max_active_orders: i32,
    pub price_history_size: i32,
    pub min_price_history: i32,
}

#[async_trait]
pub trait StrategyOperator: Send + Sync {
    async fn get_config(&self, strategy_name: &str, symbol: &str)
        -> Result<OperatorFields, String>;
    async fn update_cap(
        &self,
        strategy_name: &str,
        symbol: &str,
        max_position: &str,
    ) -> Result<(), String>;
    /// Returns `needs_restart`.
    async fn update_config(&self, fields: OperatorFields) -> Result<bool, String>;
    async fn request_restart(&self, on_stop: &str) -> Result<(), String>;
}

pub type StrategyOperatorHandle = Arc<dyn StrategyOperator>;
