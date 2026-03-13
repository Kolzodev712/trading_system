//! Level-based trading mode runner.

use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::commands::RunnerCommandHandler;
use crate::config::BotConfig;
use crate::grpc::TradingGrpcServer;
use crate::SharedServices;
use live_config::{LiveConfigClient, LiveConfigService};
use trading_core::grpc::TradingControl;
use trading_engine::health::HealthServer as TradingHealthServer;
use trading_engine::levels::{
    CancellationManager, IndicatorSubscriber, LevelBasedEventLoop, LevelExecutor, LevelManager,
};
use trading_engine::risk::RiskManager;
use trading_networking::commands::{CommandDispatcher, MpscCommandRouter};
use trading_networking::events::{cold_event_bus, hot_event_channel};

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

    // Create TradingControl for enable/disable trading
    let trading_control = Arc::new(TradingControl::new(services.trading_client.clone()));

    // Get instrument type from config (defaults to Spot if not specified)
    let instrument_type = bot_config
        .get_instrument_type()
        .unwrap_or(trading_core::types::common::InstrumentType::Spot);

    let level_executor = Arc::new(
        LevelExecutor::new(
            services.trading_client.clone(),
            services.asset_service.clone(),
            bot_config.exchange_engine.exchange.clone(),
            instrument_type.clone(),
            config_min_order_size,
            config_step_size,
            config_tick_size,
            config_min_notional,
        )
        .with_trading_control(trading_control.enabled_flag()),
    );

    // Create risk manager with symbol registry
    let risk_manager = Arc::new(RiskManager::new(risk_limits, symbol_registry.clone()));

    // Wrap level_manager in Arc<RwLock<>> for sharing with LiveConfig and event loop
    let level_manager = Arc::new(RwLock::new(level_manager));

    // Start gRPC server for CLI (use the same TradingControl instance)
    // Bind to :: (IPv6 wildcard) to accept both IPv4 and IPv6 connections
    let grpc_addr: std::net::SocketAddr = "[::]:50051"
        .parse()
        .map_err(|e| format!("Failed to parse gRPC address: {}", e))?;

    // Create gRPC service with instrument_type (same as strategy_runner)
    use crate::grpc::service::TradingCliServiceImpl;
    let grpc_service = TradingCliServiceImpl::new(
        level_manager.clone(),
        services.trading_state_manager.clone(),
        services.trading_client.clone(),
        services.exchange_state_manager.clone(),
        services.market_data_state_manager.clone(),
        services.indicator_service.clone(),
        services.indicator_calculator.clone(),
        trading_control.clone(),
    )
    .with_instrument_type(instrument_type.clone())
    .with_exchange(format!("{:?}", bot_config.exchange_engine.exchange));

    let grpc_server = TradingGrpcServer::from_service(grpc_service);
    let grpc_handle = grpc_server.start(grpc_addr).await?;
    info!("gRPC server started on {}", grpc_addr);

    let (hot_tx, hot_rx) = hot_event_channel(bot_config.channels.market_data_capacity);
    let cold_bus = cold_event_bus(bot_config.channels.exchange_update_capacity);

    let (router, cmd_rx) = MpscCommandRouter::new(1024);
    let router = Arc::new(router);

    let handler = RunnerCommandHandler {
        trading_control: Arc::clone(&trading_control),
        trading_state_manager: Arc::clone(&services.trading_state_manager),
        exchange_state_manager: Arc::clone(&services.exchange_state_manager),
        level_manager: Arc::clone(&level_manager),
        strategy_manager: None,
    };
    let mut dispatcher = CommandDispatcher::new(cmd_rx, Box::new(handler));
    tokio::spawn(async move {
        dispatcher.run().await;
    });

    let hot_tx_for_bridge = hot_tx.clone();
    let mut md_rx = services.market_data_rx.resubscribe();
    let mut shutdown_rx_md = services.shutdown_manager.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                update = md_rx.recv() => {
                    match update {
                        Ok(trading_core::channels::market_data::MarketDataUpdate::Orderbook { orderbook, trace_context }) => {
                            let event = trading_networking::events::HotEvent::orderbook_from_structured(&orderbook, trace_context);
                            let _ = hot_tx_for_bridge.try_send(event);
                        }
                        Ok(trading_core::channels::market_data::MarketDataUpdate::Trade { trade, trace_context }) => {
                            let event = trading_networking::events::HotEvent::Trade {
                                pair: trade.pair.clone(),
                                price: trade.price,
                                quantity: trade.quantity,
                                timestamp: trade.timestamp,
                                trade_id: trade.trade_id.clone(),
                                trace_context,
                            };
                            let _ = hot_tx_for_bridge.try_send(event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!("[zenoh bridge] market data lagged; skipped {}", skipped);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = shutdown_rx_md.recv() => break,
            }
        }
    });

    let cold_bus_for_bridge = cold_bus.clone();
    let mut ex_rx = services.exec_report_rx.resubscribe();
    let mut shutdown_rx_ex = services.shutdown_manager.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                update = ex_rx.recv() => {
                    match update {
                        Ok(trading_core::channels::exchange::ExchangeUpdate::Order { order, trace_context }) => {
                            let _ = cold_bus_for_bridge.publish(Arc::new(trading_networking::events::ColdEvent::Order { order, trace_context }));
                        }
                        Ok(trading_core::channels::exchange::ExchangeUpdate::Balance { balance, trace_context }) => {
                            let _ = cold_bus_for_bridge.publish(Arc::new(trading_networking::events::ColdEvent::Balance { balance, trace_context }));
                        }
                        Ok(trading_core::channels::exchange::ExchangeUpdate::Position { position, trace_context }) => {
                            let _ = cold_bus_for_bridge.publish(Arc::new(trading_networking::events::ColdEvent::Position { position, trace_context }));
                        }
                        Ok(trading_core::channels::exchange::ExchangeUpdate::Trade { trade, trace_context }) => {
                            let _ = cold_bus_for_bridge.publish(Arc::new(trading_networking::events::ColdEvent::Trade { trade, trace_context }));
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!("[zenoh bridge] exchange updates lagged; skipped {}", skipped);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = shutdown_rx_ex.recv() => break,
            }
        }
    });

    crate::maybe_start_zenoh_gateway(hot_rx, cold_bus, router).await;

    // Wrap risk_manager in Arc<RwLock<>> for LiveConfig service
    let risk_limits_for_live_config = risk_manager.get_limits().clone();
    let risk_manager_for_live_config = Arc::new(RwLock::new(RiskManager::new(
        risk_limits_for_live_config,
        symbol_registry.clone(),
    )));

    // Create LiveConfig service for runtime configuration updates
    let audit_log_file = format!(
        "logs/live_config_audit_{}.jsonl",
        chrono::Utc::now().timestamp()
    );

    let live_config_service = LiveConfigService::new(
        level_manager.clone(),
        risk_manager_for_live_config.clone(),
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
    let event_loop = LevelBasedEventLoop::new(
        level_manager.clone(),
        level_executor,
        risk_manager,
        cancellation_manager,
        indicator_subscriber,
        services.trading_client.clone(),
        services.market_data_rx.resubscribe(),
        services.exec_report_rx.resubscribe(),
        configured_pairs.clone(),
        services.trading_state_manager.clone(),
        trading_reconciliation_interval_secs,
    );

    // Note: TradingStateManager automatically handles execution reports via ExchangeStateManager subscription
    // Execution reports are published through the exchange_update channel and handled by TradingStateManager
    // No separate execution report handler needed - TradingStateManager is the single source of truth

    // Set up health server using TradingStateManager's state
    let health_server = TradingHealthServer::new(services.trading_state_manager.get_state());
    let health_server = if let Some(ref h) = services.tracing_handles {
        let handles = h.clone();
        health_server.with_metrics(Arc::new(move || {
            trading_trace::encode_prometheus_metrics(&handles)
                .unwrap_or_default()
                .into_bytes()
        }))
    } else {
        health_server
    };
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
    // Cancel all orders for all configured pairs
    for pair in configured_pairs {
        info!("Cancelling all open orders for {}...", pair);
        match services
            .trading_client
            .write()
            .await
            .cancel_all_orders(&pair)
            .await
        {
            Ok(_) => info!("All orders cancelled successfully for {}", pair),
            Err(e) => warn!(
                "Failed to cancel all orders for {}: {error}",
                pair,
                error = e
            ),
        }
    }

    services.shutdown_manager.signal_shutdown();

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
