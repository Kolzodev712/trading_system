//! Shared DEX runner bootstrap: health, gRPC, event bridge, zenoh.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;
use trading_engine::health::HealthServer as TradingHealthServer;
use trading_engine::state::TradingState;
use trading_networking::commands::{CommandDispatcher, MpscCommandRouter};

use super::grpc::{build_dex_monitoring_grpc, DexGrpcControl};
use crate::config::BotConfig;
use crate::grpc::TradingGrpcServer;
use crate::platform::commands::DexNoopHandler;
use crate::platform::event_bridge::{start_event_bridge, EventBridgeInputs};
use crate::platform::health::{dex_pairs_from_config, PlatformReadiness};
use crate::platform::maybe_start_zenoh_gateway;
use crate::SharedServices;

pub(crate) async fn start_platform(
    bot_config: &BotConfig,
    services: &SharedServices,
    grpc_control: DexGrpcControl,
) -> Result<(), Box<dyn std::error::Error>> {
    let grpc_addr: std::net::SocketAddr = "[::]:50051"
        .parse()
        .map_err(|e| format!("Failed to parse gRPC address: {e}"))?;
    let monitoring = build_dex_monitoring_grpc(services, grpc_control, bot_config)?;
    let grpc_server = TradingGrpcServer::from_service(monitoring.service);
    let mut shutdown_rx_grpc = services.shutdown_manager.subscribe();
    let _grpc_handle = grpc_server
        .start_with_shutdown(grpc_addr, async move {
            let _ = shutdown_rx_grpc.recv().await;
        })
        .await?;
    info!("gRPC server started on {grpc_addr} (DEX monitoring)");

    let dex_pairs = dex_pairs_from_config(bot_config);
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
        dex_pairs,
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
    let mut dispatcher = CommandDispatcher::new(cmd_rx, Box::new(DexNoopHandler));
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

    Ok(())
}
