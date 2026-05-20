use std::sync::Arc;

use dex_engine::runtime::DexEngineRuntime;
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use trading_core::channels::market_data::MarketDataUpdateReceiver;

use super::strategy::DexStrategy;

/// Subscribes to market data and routes `TradeCommand`s through `DexEngineRuntime`.
pub struct DexStrategyEngine {
    strategies: Vec<Arc<dyn DexStrategy>>,
    runtime: Arc<DexEngineRuntime>,
    market_data_rx: MarketDataUpdateReceiver,
}

impl DexStrategyEngine {
    pub fn new(
        strategies: Vec<Arc<dyn DexStrategy>>,
        runtime: Arc<DexEngineRuntime>,
        market_data_rx: MarketDataUpdateReceiver,
    ) -> Self {
        Self {
            strategies,
            runtime,
            market_data_rx,
        }
    }

    pub async fn run(
        self,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "DEX strategy engine started ({} strateg(ies))",
            self.strategies.len()
        );
        let mut md_rx = self.market_data_rx;
        loop {
            tokio::select! {
                update = md_rx.recv() => {
                    match update {
                        Ok(md) => {
                            let commands: Vec<_> = {
                                let state = self.runtime.state_manager.state();
                                let guard = state.read();
                                self.strategies
                                    .iter()
                                    .filter_map(|strategy| strategy.evaluate(&md, &guard))
                                    .collect()
                            };
                            for cmd in commands {
                                match self.runtime.execute(cmd).await {
                                    Ok(result) => info!(
                                        signature = %result.signature,
                                        status = ?result.status,
                                        "DEX command executed"
                                    ),
                                    Err(e) => warn!("DEX execute failed: {e}"),
                                }
                            }
                        }
                        Err(e) => {
                            error!("DEX strategy engine market data recv: {e}");
                            break;
                        }
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
}
