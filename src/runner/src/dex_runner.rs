//! DEX strategy mode runner (no CEX levels; mock chain until Solana).

use std::sync::Arc;

use tracing::info;

use crate::config::BotConfig;
use crate::SharedServices;
use trading_engine::dex::{DexStrategyEngine, LiquidationStrategy};

/// Run DEX strategies against `dex_engine` (stub provider; no Solana RPC).
pub async fn dex_runner(
    _bot_config: BotConfig,
    services: SharedServices,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Running in DEX strategy mode ===");

    let runtime = services
        .venue
        .dex_runtime()
        .ok_or("DEX runtime required for dex mode")?;

    let strategies: Vec<Arc<dyn trading_engine::dex::DexStrategy>> =
        vec![Arc::new(LiquidationStrategy::default())];

    let engine = DexStrategyEngine::new(strategies, runtime, services.market_data_rx.resubscribe());

    let shutdown_rx = services.shutdown_manager.subscribe();
    engine.run(shutdown_rx).await?;
    Ok(())
}
