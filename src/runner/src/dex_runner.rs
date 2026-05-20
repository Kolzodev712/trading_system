//! DEX strategy mode runner (no CEX levels; mock chain until Solana).

use std::sync::Arc;

use tracing::info;
use trading_networking::commands::{CommandDispatcher, MpscCommandRouter};

use crate::config::BotConfig;
use crate::event_bridge::{start_event_bridge, EventBridgeInputs};
use crate::maybe_start_zenoh_gateway;
use crate::SharedServices;
use trading_engine::dex::{DexStrategyEngine, LiquidationStrategy};

/// Run DEX strategies against `dex_engine` (stub provider; no Solana RPC).
pub async fn dex_runner(
    bot_config: BotConfig,
    services: SharedServices,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Running in DEX strategy mode ===");

    let runtime = services
        .venue
        .dex_runtime()
        .ok_or("DEX runtime required for dex mode")?;

    let (router, cmd_rx) = MpscCommandRouter::new(1024);
    let router = Arc::new(router);
    let mut dispatcher = CommandDispatcher::new(cmd_rx, Box::new(crate::commands::DexNoopHandler));
    tokio::spawn(async move {
        dispatcher.run().await;
    });

    let bridge = start_event_bridge(EventBridgeInputs {
        market_data_rx: services.market_data_rx.resubscribe(),
        cex_update_rx: services.venue.cex_update_rx(),
        dex_update_rx: services.venue.dex_update_rx(),
        shutdown_manager: &services.shutdown_manager,
        md_capacity: bot_config.channels.market_data_capacity,
        cex_capacity: bot_config.channels.cex_venue_update_capacity,
        dex_capacity: bot_config.channels.dex_venue_update_capacity,
    });
    maybe_start_zenoh_gateway(bridge.hot_rx, bridge.cold_bus, bridge.dex_bus, router).await;

    let strategies: Vec<Arc<dyn trading_engine::dex::DexStrategy>> =
        vec![Arc::new(LiquidationStrategy::default())];

    let engine = DexStrategyEngine::new(strategies, runtime, services.market_data_rx.resubscribe());

    let shutdown_rx = services.shutdown_manager.subscribe();
    engine.run(shutdown_rx).await?;
    Ok(())
}
