//! Event loop: bootstrap quotes, listen for fills, repost via `DexEngineRuntime`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dex_engine::runtime::DexEngineRuntime;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};
use trading_core::venue::dex::channels::DexVenueUpdate;

use super::manager::DexQuoteManager;

const PLACEMENT_POLL_MS: u64 = 2_000;

/// Manages fixed-price DeepBook quotes (place + repost-on-fill).
pub struct DexLevelsEngine {
    manager: Arc<RwLock<DexQuoteManager>>,
    runtime: Arc<DexEngineRuntime>,
    dex_update_rx: broadcast::Receiver<DexVenueUpdate>,
}

impl DexLevelsEngine {
    pub fn new(
        manager: Arc<RwLock<DexQuoteManager>>,
        runtime: Arc<DexEngineRuntime>,
        dex_update_rx: broadcast::Receiver<DexVenueUpdate>,
    ) -> Self {
        Self {
            manager,
            runtime,
            dex_update_rx,
        }
    }

    pub async fn run(
        mut self,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let quote_count = self.manager.read().await.quote_count();
        info!("DEX levels engine started ({quote_count} quote(s))");

        let mut placement_tick = tokio::time::interval(Duration::from_millis(PLACEMENT_POLL_MS));
        placement_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        // Seed open orders from current dex state.
        {
            let orders = {
                let state = self.runtime.state_manager.state();
                let guard = state.read();
                guard.open_orders.clone()
            };
            let mut mgr = self.manager.write().await;
            mgr.on_open_orders(orders.as_slice(), Instant::now());
        }

        loop {
            tokio::select! {
                _ = placement_tick.tick() => {
                    self.try_place_quotes().await;
                }
                update = self.dex_update_rx.recv() => {
                    match update {
                        Ok(DexVenueUpdate::OpenOrders { orders, .. }) => {
                            {
                                let mut mgr = self.manager.write().await;
                                mgr.on_open_orders(&orders, Instant::now());
                            }
                            self.try_place_quotes().await;
                        }
                        Ok(DexVenueUpdate::ExecutionResult { result, .. }) => {
                            {
                                let mut mgr = self.manager.write().await;
                                mgr.on_execution_result(&result);
                            }
                            if result.status == trading_core::venue::dex::types::TxStatus::Failed {
                                self.try_place_quotes().await;
                            }
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("DEX levels engine lagged {n} venue update(s)");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("DEX levels engine shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn try_place_quotes(&self) {
        let now = Instant::now();
        let ids = {
            let mgr = self.manager.read().await;
            mgr.quote_ids_needing_placement(now)
        };
        for quote_id in ids {
            let (cmd, resolved_price) = {
                let mgr = self.manager.read().await;
                match mgr.build_place_command(&quote_id).await {
                    Ok(c) => {
                        let price = match &c {
                            trading_core::venue::dex::TradeCommand::PlaceLimitOrder {
                                price,
                                ..
                            } => *price,
                            _ => continue,
                        };
                        (c, price)
                    }
                    Err(e) => {
                        warn!(quote_id = %quote_id, "invalid place command: {e}");
                        continue;
                    }
                }
            };
            {
                let mut mgr = self.manager.write().await;
                mgr.record_resolved_price(&quote_id, resolved_price);
            }
            match self.runtime.execute(cmd).await {
                Ok(result) => {
                    info!(
                        quote_id = %quote_id,
                        signature = %result.signature,
                        status = ?result.status,
                        "DEX quote placed"
                    );
                    let mut mgr = self.manager.write().await;
                    mgr.on_place_submitted(&quote_id, result.signature);
                }
                Err(e) => {
                    warn!(quote_id = %quote_id, "DEX quote execute failed: {e}");
                    let mut mgr = self.manager.write().await;
                    mgr.on_place_failed(&quote_id, Instant::now());
                }
            }
        }
    }
}
