//! Level-based trading mode runner.

use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::grpc::TradingGrpcServer;
use crate::platform::commands::RunnerCommandHandler;
use crate::platform::event_bridge::{start_event_bridge, EventBridgeInputs};
use crate::platform::health::{dex_pairs_from_config, PlatformReadiness};
use crate::platform::{build_metrics_provider, maybe_start_zenoh_gateway};
use crate::SharedServices;
use live_config::{LiveConfigClient, LiveConfigService};
use trading_core::grpc::TradingControl;
use trading_engine::health::HealthServer as TradingHealthServer;
use trading_engine::levels::{
    CancellationManager, IndicatorSubscriber, LevelBasedEventLoop, LevelExecutor, LevelManager,
};
use trading_engine::risk::RiskManager;
use trading_networking::commands::{CommandDispatcher, MpscCommandRouter};

use crate::config::BotConfig;

/// Run levels mode
pub async fn level_runner(
    bot_config: BotConfig,
    levels_config_path: String,
    services: SharedServices,
    level_persistence: Option<Arc<trading_engine::levels::persistence::LevelPersistence>>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Running in LEVELS mode ===");

    // Initialize level manager with persistence if enabled
    let mut level_manager = if let Some(ref persistence) = level_persistence {
        LevelManager::new().with_persistence(Arc::clone(persistence))
    } else {
        LevelManager::new()
    };
    let instrument_type = bot_config
        .get_instrument_type()
        .unwrap_or(trading_core::types::common::InstrumentType::Spot);
    level_manager = level_manager.with_identity_scope(
        bot_config.exchange_engine.exchange.clone(),
        instrument_type.clone(),
        bot_config.mongo.bot_id.clone(),
    );

    // Get pairs from exchange config (needed for expanding levels)
    let configured_pairs = bot_config.get_pairs()?;
    if configured_pairs.is_empty() {
        return Err("No pairs configured in bot.json".into());
    }

    // Always load from file first (file is source of truth)
    info!(
        "Loading levels configuration from {}...",
        levels_config_path
    );
    let risk_limits = level_manager
        .load_from_config_with_risk_limits(Path::new(&levels_config_path), &configured_pairs)
        .await
        .map_err(|e| format!("Failed to load levels config: {e}"))?;
    risk_limits.require_explicit_for_exchange(bot_config.execution_mode)?;

    // Update MongoDB with file-based config (file is source of truth)
    if level_persistence.is_some() {
        if let Err(e) = level_manager
            .save_all_levels(&risk_limits, &configured_pairs)
            .await
        {
            warn!("Failed to update levels in MongoDB: {}", e);
        } else {
            info!("Updated levels configuration in MongoDB with file-based config");
        }
    }

    info!(
        "Loaded {} levels (expanded for {} pairs)",
        level_manager.get_all_levels().len(),
        configured_pairs.len()
    );

    // Use all configured pairs for symbol registry (levels can use any of these pairs)
    let symbol_registry = Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
        configured_pairs.clone(),
    ));

    // Create level executor with AssetService integration
    // Use defaults for config values (Asset Engine will override if available)
    let config_min_order_size = rust_decimal::Decimal::new(1, 5); // 0.00001
    let config_step_size = rust_decimal::Decimal::new(1, 5); // 0.00001
    let config_tick_size = rust_decimal::Decimal::new(1, 2); // 0.01
    let config_min_notional = rust_decimal::Decimal::new(5, 0); // 5.0

    let trading_client = {
        let inner = services.trading_client()?.clone();
        if bot_config.execution_mode == trading_core::types::ExecutionMode::Shadow {
            trading_engine::cex::shadow::ShadowGuardClient::wrap(inner)
        } else {
            inner
        }
    };

    let start_paused =
        bot_config.exchange_engine.exchange != trading_core::types::common::Exchange::Paper;
    let trading_control =
        Arc::new(TradingControl::new(trading_client.clone()).with_entries_paused(start_paused));
    if start_paused {
        info!("entries start paused until authenticated Resume (non-Paper venue)");
    }

    let level_executor = Arc::new(
        LevelExecutor::new(
            trading_client.clone(),
            services.asset_service.clone(),
            bot_config.exchange_engine.exchange.clone(),
            instrument_type.clone(),
            services.level_pricing.clone(),
            config_min_order_size,
            config_step_size,
            config_tick_size,
            config_min_notional,
        )
        .with_trading_control(trading_control.enabled_flag())
        .with_operator_flags(
            trading_control.entries_paused_flag(),
            trading_control.flatten_latched_flag(),
        )
        .with_execution_mode(bot_config.execution_mode),
    );

    // Share the executor risk manager so live updates hit the same state.
    let risk_manager = Arc::new(RwLock::new(
        RiskManager::new(risk_limits, symbol_registry.clone()).with_execution_ledger(
            services
                .trading_state_manager
                .as_ref()
                .and_then(|tsm| tsm.execution_ledger())
                .ok_or("execution ledger is required")?,
        ),
    ));

    // Wrap level_manager in Arc<RwLock<>> for sharing with LiveConfig and event loop
    let level_manager = Arc::new(RwLock::new(level_manager));

    let grpc_addr = bot_config.grpc_listen_addr()?;

    // Create gRPC service with instrument_type (same as strategy_runner)
    use crate::grpc::service::TradingCliServiceImpl;
    let grpc_service = TradingCliServiceImpl::new(
        level_manager.clone(),
        services
            .trading_state_manager
            .clone()
            .ok_or("Trading state requires venue.mode \"cex\"")?,
        trading_client.clone(),
        services.exchange_state_manager()?.clone(),
        services.market_data_state_manager.clone(),
        services.indicator_service.clone(),
        services.indicator_calculator.clone(),
        trading_control.clone(),
    )
    .with_instrument_type(instrument_type.clone())
    .with_exchange(format!("{:?}", bot_config.exchange_engine.exchange));
    let journal = std::sync::Arc::new(
        trading_engine::state::SubmissionJournal::open(&bot_config.submission_journal_path)
            .map_err(|err| format!("submission journal: {err}"))?,
    );
    let (exec_mode, venue, environment) = crate::config::execution_status_from_bot(&bot_config);
    let grpc_service = grpc_service
        .with_execution_status(exec_mode, venue, environment)
        .with_submission_journal(journal.clone())
        .with_restart_flag_path(bot_config.restart_flag_path.clone());
    let grpc_service = if let Some(dex_sm) = services.venue.dex_state_manager() {
        grpc_service.with_dex_state_manager(dex_sm, format!("{:?}", services.venue.mode()))
    } else {
        grpc_service
    };

    let grpc_server = TradingGrpcServer::from_service(grpc_service);
    let mut shutdown_rx_grpc = services.shutdown_manager.subscribe();
    let grpc_handle = grpc_server
        .start_with_shutdown(grpc_addr, async move {
            let _ = shutdown_rx_grpc.recv().await;
        })
        .await?;
    info!("gRPC server started on {}", grpc_addr);

    let (router, cmd_rx) = MpscCommandRouter::new(1024);
    let router = Arc::new(router);

    let handler = RunnerCommandHandler {
        trading_control: Arc::clone(&trading_control),
        trading_state_manager: services
            .trading_state_manager
            .clone()
            .ok_or("Trading state requires venue.mode cex")?,
        exchange_state_manager: services.exchange_state_manager()?.clone(),
        level_manager: Arc::clone(&level_manager),
        strategy_manager: None,
    };
    let mut dispatcher = CommandDispatcher::new(cmd_rx, Box::new(handler));
    tokio::spawn(async move {
        dispatcher.run().await;
    });

    let bridge = start_event_bridge(EventBridgeInputs {
        market_data_rx: services.market_data_rx.resubscribe(),
        cex_update_rx: Some(services.exec_report_rx()?.resubscribe()),
        dex_update_rx: services.venue.dex_update_rx(),
        shutdown_manager: &services.shutdown_manager,
        md_capacity: bot_config.channels.market_data_capacity,
        cex_capacity: bot_config.channels.cex_venue_update_capacity,
        dex_capacity: bot_config.channels.dex_venue_update_capacity,
    });
    maybe_start_zenoh_gateway(bridge.hot_rx, bridge.cold_bus, bridge.dex_bus, router).await;

    let audit_log_file = format!(
        "logs/live_config_audit_{}.jsonl",
        chrono::Utc::now().timestamp()
    );

    let live_config_service = LiveConfigService::new(
        level_manager.clone(),
        risk_manager.clone(),
        level_persistence.clone(),
        audit_log_file,
    );

    let (live_config_client, live_config_api) = LiveConfigClient::new(
        live_config_service,
        bot_config.channels.live_config_request_capacity,
    );

    // Spawn LiveConfig API handler
    let mut shutdown_rx_live_config = services.shutdown_manager.subscribe();
    let live_config_handle = tokio::spawn(async move {
        tokio::select! {
            _ = live_config_api.run() => {
                info!("LiveConfig API handler completed");
            }
            _ = shutdown_rx_live_config.recv() => {
                info!("LiveConfig API handler shutting down");
            }
        }
    });

    info!("LiveConfig service initialized and running");

    // Store client for potential future use (e.g., CLI commands, admin interface)
    // For now, it's created but not exposed - can be added to SharedServices if needed
    let _live_config_client = live_config_client;

    // Create cancellation manager (empty conditions for now - can be configured later)
    let cancellation_manager = Arc::new(CancellationManager::new(Vec::new()));

    // Create shared indicators cache (used by both subscriber task and event loop)
    let indicators_cache = Arc::new(RwLock::new(std::collections::HashMap::new()));

    // Start indicator subscriber task - run loop directly without holding write lock on subscriber
    let subscriber_cache = indicators_cache.clone();
    let mut indicator_receiver = services.indicator_rx.resubscribe();
    let mut shutdown_rx_subscriber = services.shutdown_manager.subscribe();
    let subscriber_handle = tokio::spawn(async move {
        info!("[INDICATOR SUBSCRIBER] Task started, waiting for indicator updates...");
        loop {
            tokio::select! {
                update = indicator_receiver.recv() => {
                    match update {
                        Ok(update) => {
                            let indicator_value = indicator_engine::data::IndicatorValue {
                                indicator_id: update.indicator_id.clone(),
                                indicator_name: update.indicator_name.clone(),
                                pair: update.pair.clone(),
                                value: update.value,
                                timestamp: update.timestamp,
                                period: update.period,
                                is_composite: update.is_composite,
                            };

                            let mut indicators = subscriber_cache.write().await;
                            indicators.insert(update.indicator_id.clone(), indicator_value.clone());
                            drop(indicators);

                            tracing::debug!(
                                "[INDICATOR SUBSCRIBER] Updated cache with {}: {} = {}",
                                update.indicator_id,
                                update.indicator_name,
                                update.value
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                "[INDICATOR SUBSCRIBER] Receiver lagged, skipped {skipped} messages. This may indicate slow processing."
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("[INDICATOR SUBSCRIBER] Channel closed - shutting down subscriber");
                            break;
                        }
                    }
                }
                _ = shutdown_rx_subscriber.recv() => {
                    info!("[INDICATOR SUBSCRIBER] Shutting down");
                    break;
                }
            }
        }
    });

    // Create indicator subscriber wrapper (for compatibility with event loop)
    // Replace the subscriber's internal cache with our shared cache so both tasks use the same cache
    let mut subscriber = IndicatorSubscriber::new(services.indicator_rx.resubscribe());
    subscriber.set_cache(indicators_cache.clone());
    let indicator_subscriber = Arc::new(RwLock::new(subscriber));

    // Create level-based event loop
    // Pass all pairs from bot config for reconciliation and exchange update rx so levels receive Order (e.g. Canceled) and clear active_orders
    let trading_reconciliation_interval_secs =
        bot_config.reconciliation.trading_level_interval_ms / 1000;
    let md_for_seed = services.market_data_state_manager.clone();
    let event_loop = LevelBasedEventLoop::new(
        level_manager.clone(),
        level_executor,
        risk_manager,
        cancellation_manager,
        indicator_subscriber,
        trading_client.clone(),
        services.market_data_rx.resubscribe(),
        services.exec_report_rx()?.resubscribe(),
        configured_pairs.clone(),
        services
            .trading_state_manager
            .clone()
            .ok_or("Trading state requires venue.mode \"cex\"")?,
        trading_reconciliation_interval_secs,
    )
    .with_submission_journal(journal.clone())
    .with_cached_books(Arc::new(move || {
        let state = md_for_seed.get_state();
        let Ok(guard) = state.try_read() else {
            return Vec::new();
        };
        guard
            .get_pairs_with_orderbooks()
            .into_iter()
            .filter_map(|pair| guard.get_orderbook(&pair).cloned())
            .collect()
    }));

    // Note: TradingStateManager automatically handles execution reports via ExchangeStateManager subscription
    // Execution reports are published through the CEX venue update channel and handled by TradingStateManager
    // No separate execution report handler needed - TradingStateManager is the single source of truth

    let trading_state = services
        .trading_state_manager
        .clone()
        .ok_or("Trading state requires venue.mode \"cex\"")?;
    for pair in &configured_pairs {
        if let Err(e) = trading_state.reconcile(pair, None).await {
            warn!("Startup position reconcile failed for {pair}: {e}");
        }
    }
    let readiness = PlatformReadiness {
        venue_mode: bot_config.venue.mode,
        started_at: std::time::SystemTime::now(),
        exchange_state_manager: services.exchange_state_manager().ok(),
        market_data_state_manager: Some(services.market_data_state_manager.clone()),
        configured_pairs: configured_pairs.clone(),
        dex_pairs: dex_pairs_from_config(&bot_config),
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
    let health_server = TradingHealthServer::new(trading_state.get_state())
        .with_readiness(readiness_check, Some(readiness_detail))
        .with_metrics(build_metrics_provider(
            services.tracing_handles.clone(),
            bot_config.venue.mode,
        ));
    let server_config = bot_config.server.clone();
    let mut shutdown_rx_health = services.shutdown_manager.subscribe();
    let health_handle = tokio::spawn(async move {
        tokio::select! {
            result = health_server.start(&server_config.bind_address, server_config.port) => {
                if let Err(e) = result {
                    error!("Health server error: {e}");
                }
            }
            _ = shutdown_rx_health.recv() => {
                info!("Health server shutting down");
            }
        }
    });

    info!("=== All systems ready! ===");
    info!(
        "Starting level-based trading for {} pair(s): {:?}",
        configured_pairs.len(),
        configured_pairs
    );

    // Run event loop
    let mut shutdown_rx_event_loop = services.shutdown_manager.subscribe();
    let event_loop_handle = tokio::spawn(async move {
        tokio::select! {
            result = event_loop.run() => {
                if let Err(e) = result {
                    error!("Level-based event loop error: {e}");
                }
            }
            _ = shutdown_rx_event_loop.recv() => {
                info!("Level-based event loop shutting down");
            }
        }
    });

    services.shutdown_manager.wait_for_shutdown().await;

    info!("Shutting down gracefully...");
    if bot_config.execution_mode == trading_core::types::ExecutionMode::Shadow {
        info!("Shadow mode: skipping owned-order cancel on shutdown");
    } else {
        let owned = level_manager.read().await.owned_order_ids();
        if !owned.is_empty() {
            info!(
                "Cancelling {} owned level order(s) on shutdown",
                owned.len()
            );
            let client = trading_client.write().await;
            if let Err(err) = trading_engine::levels::cancel_owned_order_ids(&*client, &owned).await
            {
                warn!("shutdown owned-order cancel unverified: {err}");
            }
        }
    }

    services.shutdown_manager.signal_shutdown();
    grpc_handle.abort();

    let shutdown_timeout =
        tokio::time::Duration::from_secs(bot_config.system.shutdown_timeout_secs);
    tokio::select! {
        _ = tokio::time::sleep(shutdown_timeout) => {
            warn!("Shutdown timeout reached, forcing exit");
        }
        _ = async {
            let _ = services.market_data_handle.await;
            let _ = services.trading_ws_handle.await;
            let _ = health_handle.await;
            let _ = event_loop_handle.await;
            let _ = subscriber_handle.await;
            let _ = services.indicator_handle.await;
            let _ = live_config_handle.await;
            let _ = grpc_handle.await;
        } => {
            info!("All components shut down successfully");
        }
    }

    Ok(())
}
