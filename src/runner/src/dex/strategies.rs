//! DEX strategies engine wiring.

use std::sync::Arc;

use dex_engine::runtime::DexEngineRuntime;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;
use trading_engine::dex::{DexStrategiesFile, DexStrategyEngine, DexStrategyManager};

use crate::SharedServices;

pub(crate) fn prepare(config_path: &str) -> Result<Arc<RwLock<DexStrategyManager>>, String> {
    let strategies_file = DexStrategiesFile::load_from_file(config_path)
        .map_err(|e| format!("{e} (path: {config_path})"))?;
    info!(
        "Loaded {} DEX strateg(ies) from {}",
        strategies_file.strategies.len(),
        config_path
    );
    Ok(Arc::new(RwLock::new(DexStrategyManager::from_file(
        strategies_file,
    )?)))
}

pub(crate) fn spawn(
    strategy_manager: Arc<RwLock<DexStrategyManager>>,
    runtime: Arc<DexEngineRuntime>,
    services: &SharedServices,
) -> Result<JoinHandle<()>, Box<dyn std::error::Error>> {
    let dex_update_rx = services
        .venue
        .dex_update_rx()
        .ok_or("DEX venue update channel required")?;

    let strategy_engine = DexStrategyEngine::new(
        strategy_manager,
        runtime,
        services.market_data_rx.resubscribe(),
        dex_update_rx,
    );

    let shutdown_rx = services.shutdown_manager.subscribe();
    Ok(tokio::spawn(async move {
        if let Err(e) = strategy_engine.run(shutdown_rx).await {
            tracing::warn!("DEX strategy engine error: {e}");
        }
    }))
}
