//! Event loop: bootstrap quotes, listen for fills, repost via `DexEngineRuntime`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dex_engine::runtime::DexEngineRuntime;
use dex_engine::DeepBookPoolMeta;
use smol_str::SmolStr;
use tokio::sync::broadcast;
use tracing::{info, warn};
use trading_core::venue::dex::channels::DexVenueUpdate;

use super::config::DexLevelsFile;
use super::manager::DexQuoteManager;

const PLACEMENT_POLL_MS: u64 = 2_000;

/// Manages fixed-price DeepBook quotes (place + repost-on-fill).
pub struct DexLevelsEngine {
    manager: DexQuoteManager,
    runtime: Arc<DexEngineRuntime>,
    dex_update_rx: broadcast::Receiver<DexVenueUpdate>,
}

impl DexLevelsEngine {
    pub fn new(
        file: DexLevelsFile,
        balance_manager_id: SmolStr,
        pool_meta: HashMap<SmolStr, DeepBookPoolMeta>,
        runtime: Arc<DexEngineRuntime>,
        dex_update_rx: broadcast::Receiver<DexVenueUpdate>,
    ) -> Result<Self, String> {
        let manager = DexQuoteManager::from_file(file, balance_manager_id, pool_meta)?;
        Ok(Self {
            manager,
            runtime,
            dex_update_rx,
        })
    }

    pub async fn run(
        mut self,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let quote_count = self.manager.quote_count();
        info!("DEX levels engine started ({quote_count} quote(s))");

        let mut placement_tick = tokio::time::interval(Duration::from_millis(PLACEMENT_POLL_MS));
        placement_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        // Seed open orders from current dex state.
        {
            let state = self.runtime.state_manager.state();
            let guard = state.read();
            self.manager
                .on_open_orders(guard.open_orders.as_slice(), Instant::now());
        }

        loop {
            tokio::select! {
                _ = placement_tick.tick() => {
                    self.try_place_quotes().await;
                }
                update = self.dex_update_rx.recv() => {
                    match update {
                        Ok(DexVenueUpdate::OpenOrders { orders, .. }) => {
                            self.manager.on_open_orders(&orders, Instant::now());
                            self.try_place_quotes().await;
                        }
                        Ok(DexVenueUpdate::ExecutionResult { result, .. }) => {
                            self.manager.on_execution_result(&result);
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

    async fn try_place_quotes(&mut self) {
        let now = Instant::now();
        let ids = self.manager.quote_ids_needing_placement(now);
        for quote_id in ids {
            let cmd = match self.manager.build_place_command(&quote_id) {
                Ok(c) => c,
                Err(e) => {
                    warn!(quote_id = %quote_id, "invalid place command: {e}");
                    continue;
                }
            };
            match self.runtime.execute(cmd).await {
                Ok(result) => {
                    info!(
                        quote_id = %quote_id,
                        signature = %result.signature,
                        status = ?result.status,
                        "DEX quote placed"
                    );
                    self.manager.on_place_submitted(&quote_id, result.signature);
                }
                Err(e) => warn!(quote_id = %quote_id, "DEX quote execute failed: {e}"),
            }
        }
    }
}
