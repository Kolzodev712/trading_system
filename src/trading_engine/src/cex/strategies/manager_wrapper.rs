//! Wrapper to make StrategyManager work with StrategyExecutor

use crate::cex::intent::{StrategyContext, StrategyIntent};
use crate::cex::state::Strategy;
use crate::cex::strategies::strategy_manager::StrategyManager;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use trading_core::{error::EngineError, types::exchange::Order};

pub struct StrategyManagerWrapper {
    strategy_manager: Arc<RwLock<StrategyManager>>,
}

impl StrategyManagerWrapper {
    pub fn new(strategy_manager: Arc<RwLock<StrategyManager>>) -> Self {
        Self { strategy_manager }
    }
}

#[async_trait]
impl Strategy for StrategyManagerWrapper {
    async fn evaluate(&mut self, ctx: &StrategyContext<'_>) -> Result<StrategyIntent, EngineError> {
        static WRAPPER_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let wrapper_count = WRAPPER_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let should_log = wrapper_count.is_multiple_of(20);

        if should_log {
            info!(
                "[StrategyManagerWrapper] evaluate called for {}",
                ctx.orderbook.pair
            );
        }

        let intents = {
            let manager = self.strategy_manager.read().await;
            manager.on_orderbook(ctx.orderbook, ctx.account).await?
        };

        Ok(intents
            .into_iter()
            .find(|intent| !matches!(intent, StrategyIntent::Hold { .. }))
            .unwrap_or_else(|| StrategyIntent::hold("StrategyManagerWrapper")))
    }

    async fn on_execution_report(&mut self, order: &Order) -> Result<(), EngineError> {
        let manager = self.strategy_manager.read().await;
        manager.on_execution_report(order).await
    }

    async fn on_completed_candle(
        &mut self,
        candle: &trading_core::types::CompletedCandle,
    ) -> Result<(), EngineError> {
        let manager = self.strategy_manager.read().await;
        manager.on_completed_candle(candle).await
    }
}
