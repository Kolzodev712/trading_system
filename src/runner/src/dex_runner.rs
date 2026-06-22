//! DEX levels runner (DeepBook quotes via `DexLevelsEngine`).

use std::sync::Arc;

use smol_str::SmolStr;
use tracing::{info, warn};
use trading_engine::dex::DexLevelsEngine;
use trading_engine::health::HealthServer as TradingHealthServer;
use trading_engine::state::TradingState;
use trading_networking::commands::{CommandDispatcher, MpscCommandRouter};

use crate::config::BotConfig;
use crate::dex_monitoring::build_dex_monitoring_grpc;
use crate::event_bridge::{start_event_bridge, EventBridgeInputs};
use crate::grpc::TradingGrpcServer;
use crate::maybe_start_zenoh_gateway;
use crate::platform_health::{dex_pairs_from_config, PlatformReadiness};
use crate::SharedServices;
use tokio::sync::RwLock;

/// Run DEX fixed-price levels against `dex_engine`.
pub async fn dex_runner(
    bot_config: BotConfig,
    dex_levels_config_path: String,
    services: SharedServices,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Running in DEX levels mode ===");

    let runtime = services
        .venue
        .dex_runtime()
        .ok_or("DEX runtime required for dex mode")?;

    let dex_config = bot_config
        .dex_engine
        .as_ref()
        .ok_or("dex_engine config required")?;
    let deepbook = dex_config
        .deepbook_config()
        .map_err(|e| format!("invalid dex_engine config: {e}"))?;

    let balance_manager_id = deepbook
        .balance_manager_id()
        .ok_or(
            "balanceManagerId required in dex_engine config for DEX levels (run create_balance_manager first)",
        )?;
    let balance_manager_id = SmolStr::new(balance_manager_id);

    let pool_meta = runtime
        .pool_meta()
        .ok_or("DEX runtime missing pool metadata (DeepBook bootstrap failed)?")?
        .as_ref()
        .clone();

    let levels_file = trading_engine::dex::DexLevelsFile::load_from_file(&dex_levels_config_path)
        .map_err(|e| format!("{e} (path: {dex_levels_config_path})"))?;
    info!(
        "Loaded {} DEX quote(s) from {}",
        levels_file.quotes.len(),
        dex_levels_config_path
    );

    let grpc_addr: std::net::SocketAddr = "[::]:50051"
        .parse()
        .map_err(|e| format!("Failed to parse gRPC address: {e}"))?;
    let monitoring = build_dex_monitoring_grpc(&services)?;
    let grpc_server = TradingGrpcServer::from_service(monitoring.service);
    let _grpc_handle = grpc_server.start(grpc_addr).await?;
    info!("gRPC server started on {grpc_addr} (DEX monitoring)");

    let dex_pairs = dex_pairs_from_config(&bot_config);
    let cex_pairs = if bot_config.venue.mode == trading_core::venue::VenueMode::Both {
        bot_config.get_pairs().unwrap_or_default()
    } else {
        Vec::new()
    };
    let readiness = PlatformReadiness {
        venue_mode: bot_config.venue.mode,
        started_at: std::time::SystemTime::now(),
        exchange_state_manager: services.venue.exchange_state_manager(),
        market_data_state_manager: Some(services.market_data_state_manager.clone()),
        configured_pairs: cex_pairs,
        dex_pairs: dex_pairs.clone(),
        dex_ready: services.venue.dex_state_manager().is_some(),
        mongo_enabled: bot_config.mongo.enabled,
    };
    let readiness_check = {
        let r = readiness.clone();
        Arc::new(move || r.is_ready()) as Arc<dyn Fn() -> bool + Send + Sync>
    };
    let readiness_detail = {
        let r = readiness;
        Arc::new(move || r.status_detail()) as Arc<dyn Fn() -> String + Send + Sync>
    };
    let trading_state = Arc::new(RwLock::new(TradingState::default()));
    let health_server = TradingHealthServer::new(trading_state)
        .with_readiness(readiness_check, Some(readiness_detail))
        .with_metrics(crate::build_metrics_provider(
            services.tracing_handles.clone(),
            bot_config.venue.mode,
        ));
    let server_config = bot_config.server.clone();
    let mut shutdown_rx_health = services.shutdown_manager.subscribe();
    let _health_handle = tokio::spawn(async move {
        tokio::select! {
            result = health_server.start(&server_config.bind_address, server_config.port) => {
                if let Err(e) = result {
                    tracing::error!("Health server error: {e}");
                }
            }
            _ = shutdown_rx_health.recv() => {
                info!("Health server shutting down");
            }
        }
    });

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

    let dex_update_rx = services
        .venue
        .dex_update_rx()
        .ok_or("DEX venue update channel required")?;

    let levels_engine = DexLevelsEngine::new(
        levels_file,
        balance_manager_id,
        pool_meta,
        runtime.clone(),
        dex_update_rx,
    )?;

    let shutdown_rx = services.shutdown_manager.subscribe();
    let levels_handle = tokio::spawn(async move {
        if let Err(e) = levels_engine.run(shutdown_rx).await {
            warn!("DEX levels engine error: {e}");
        }
    });

    services.shutdown_manager.wait_for_shutdown().await;
    info!("Shutting down DEX levels mode...");
    levels_handle.abort();
    Ok(())
}
