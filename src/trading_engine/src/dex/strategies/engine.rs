//! Subscribes to market data and routes strategy commands through `DexEngineRuntime`.

use std::sync::Arc;
use std::time::Instant;

use dex_engine::runtime::DexEngineRuntime;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};
use trading_core::channels::market_data::MarketDataUpdateReceiver;
use trading_core::venue::dex::channels::DexVenueUpdate;

use super::manager::DexStrategyManager;

/// Event loop for configured DEX strategies (separate from CEX strategies and DEX levels).
pub struct DexStrategyEngine {
    manager: Arc<RwLock<DexStrategyManager>>,
    runtime: Arc<DexEngineRuntime>,
    market_data_rx: MarketDataUpdateReceiver,
    dex_update_rx: broadcast::Receiver<DexVenueUpdate>,
}

impl DexStrategyEngine {
    pub fn new(
        manager: Arc<RwLock<DexStrategyManager>>,
        runtime: Arc<DexEngineRuntime>,
        market_data_rx: MarketDataUpdateReceiver,
        dex_update_rx: broadcast::Receiver<DexVenueUpdate>,
    ) -> Self {
        Self {
            manager,
            runtime,
            market_data_rx,
            dex_update_rx,
        }
    }

    pub async fn run(
        mut self,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let count = self.manager.read().await.strategy_count();
        info!("DEX strategy engine started ({count} strateg(ies))");

        loop {
            tokio::select! {
                update = self.market_data_rx.recv() => {
                    match update {
                        Ok(md) => self.on_market_data(&md).await,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("DEX strategy engine MD lagged {n} update(s)");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                venue = self.dex_update_rx.recv() => {
                    if venue.is_err() {
                        break;
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("DEX strategy engine shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn on_market_data(&self, md: &trading_core::channels::market_data::MarketDataUpdate) {
        let now = Instant::now();
        let dex_state = {
            let state = self.runtime.state_manager.state();
            let guard = state.read();
            guard.clone()
        };
        let commands = {
            let mut mgr = self.manager.write().await;
            mgr.evaluate(md, &dex_state, now)
        };
        for (strategy_id, cmd) in commands {
            match self.runtime.execute(cmd).await {
                Ok(result) => info!(
                    strategy_id = %strategy_id,
                    signature = %result.signature,
                    status = ?result.status,
                    "DEX strategy command submitted"
                ),
                Err(e) => {
                    warn!(strategy_id = %strategy_id, "DEX strategy execute failed: {e}");
                    let mut mgr = self.manager.write().await;
                    mgr.on_execute_failed(strategy_id.as_str(), now);
                }
            }
        }
    }
}
