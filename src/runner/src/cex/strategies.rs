//! Strategy-based trading mode runner.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::config::{BotConfig, StrategiesConfig};
use crate::grpc::TradingGrpcServer;
use crate::platform::commands::RunnerCommandHandler;
use crate::platform::event_bridge::{start_event_bridge, EventBridgeInputs};
use crate::platform::health::{dex_pairs_from_config, PlatformReadiness};
use crate::platform::{build_metrics_provider, maybe_start_zenoh_gateway};
use crate::SharedServices;
use trading_core::grpc::TradingControl;
use trading_engine::health::HealthServer as TradingHealthServer;
use trading_engine::levels::LevelManager;
use trading_engine::strategies::{
    BuyLowSellHigh, StrategyExecutor, StrategyManager, StrategyManagerWrapper,
};
use trading_networking::commands::{CommandDispatcher, MpscCommandRouter};

/// Processes execution reports
async fn execution_report_handler<F, Fut>(
    mut exchange_update_rx: trading_core::venue::cex::channels::CexVenueUpdateReceiver,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    handler: F,
) where
    F: Fn(trading_core::types::exchange::Order) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send,
{
    loop {
        tokio::select! {
            update = exchange_update_rx.recv() => {
                match update {
                    Ok(trading_core::venue::cex::channels::CexVenueUpdate::Order {
                        order,
                        ..
                    }) => {
                        if let Err(e) = handler(order).await {
                            error!("Error handling execution report: {e}");
                        }
                    }
                    Ok(_) => {
                        // Ignore non-order updates (balances, trades, positions)
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("Execution report handler lagged, skipped {} messages", skipped);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        info!("Exchange update channel closed");
                        break;
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                info!("Execution report handler shutting down");
                break;
            }
        }
    }
}

/// Run strategies mode
pub async fn strategy_runner(
    bot_config: BotConfig,
    strategies_config_path: String,
    services: SharedServices,
    _strategy_persistence: Option<
        Arc<trading_engine::strategies::persistence::StrategyPersistence>,
    >,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Running in STRATEGIES mode ===");

    let strategies_config = StrategiesConfig::load_from_file(&strategies_config_path)?;
    strategies_config.validate()?;

    // Get pairs from exchange config
    let configured_pairs = bot_config.get_pairs()?;
    if configured_pairs.is_empty() {
        return Err("No pairs configured in bot.json".into());
    }

    // Get all strategy configs (supports both single and multiple)
    let strategy_configs = strategies_config.into_vec();
    info!("Creating {} strategy(ies)...", strategy_configs.len());

    if strategy_configs.is_empty() {
        return Err("No strategies configured".into());
    }

    // Validate that we have enough pairs for all strategies
    if strategy_configs.len() > configured_pairs.len() {
        return Err(format!(
            "Not enough pairs configured: {} strategies but only {} pairs in bot.json",
            strategy_configs.len(),
            configured_pairs.len()
        )
        .into());
    }

    // Create StrategyManager and add all strategies
    // Validate that all strategy symbols match pairs from bot config
    let mut strategy_manager = StrategyManager::new();
    let mut all_symbols = Vec::new();
    let mut strategy_infos = Vec::new(); // For gRPC monitoring

    for config in strategy_configs.iter() {
        let pair = &config.symbol;

        // Validate that strategy symbol matches one of the configured pairs
        if !configured_pairs
            .iter()
            .any(|p| p.base == pair.base && p.quote == pair.quote)
        {
            return Err(format!(
                "Strategy {} uses symbol {} which is not in bot.json pairs. Available pairs: {:?}",
                config.strategy_type, pair, configured_pairs
            )
            .into());
        }

        info!(
            "Creating {} strategy for {}...",
            config.strategy_type.as_str(),
            pair
        );
        // Get order size configs from first strategy (they should be the same for all)
        let first_config = &strategy_configs[0];
        let min_order_size = first_config
            .min_order_size
            .unwrap_or_else(|| rust_decimal::Decimal::new(1, 5));
        let step_size = first_config
            .step_size
            .unwrap_or_else(|| rust_decimal::Decimal::new(1, 5));
        let tick_size = first_config
            .tick_size
            .unwrap_or_else(|| rust_decimal::Decimal::new(1, 2));
        let min_notional = first_config
            .min_notional
            .unwrap_or_else(|| rust_decimal::Decimal::new(5, 0));

        let strategy = Arc::new(RwLock::new(BuyLowSellHigh::new(
            services.trading_client()?.clone(),
            pair.clone(),
            config.position_size,
            config.max_position,
            config.buy_threshold_percent,
            config.sell_threshold_percent,
            config.stop_loss_percent,
            config.take_profit_percent,
            config.max_active_orders,
            config.price_history_size,
            config.min_price_history,
            Some(services.indicator_calculator.clone()), // Use calculator directly (read-only, no Mutex)
            services.asset_service.clone(),
            bot_config.exchange_engine.exchange.clone(),
            min_order_size,
            step_size,
            tick_size,
            min_notional,
        )));

        // Create strategy ID: "strategy_name:symbol"
        let strategy_id =
            trading_engine::strategies::strategy_manager::StrategyManager::make_strategy_id(
                &config.strategy_type.to_string(),
                pair,
            );
        strategy_manager
            .add_strategy(strategy, strategy_id.clone(), pair.clone())
            .await;
        all_symbols.push(pair.clone());
        strategy_infos.push((
            config.strategy_type.to_string(),
            pair.clone(),
            config.max_active_orders,
        ));
    }

    // Get order size configs from first strategy (they should be the same for all)
    let first_config = &strategy_configs[0];
    let min_order_size = first_config
        .min_order_size
        .unwrap_or_else(|| rust_decimal::Decimal::new(1, 5));
    let step_size = first_config
        .step_size
        .unwrap_or_else(|| rust_decimal::Decimal::new(1, 5));
    let tick_size = first_config
        .tick_size
        .unwrap_or_else(|| rust_decimal::Decimal::new(1, 2));
    let min_notional = first_config
        .min_notional
        .unwrap_or_else(|| rust_decimal::Decimal::new(5, 0));

    // Create TradingControl for enable/disable trading
    let trading_control = Arc::new(TradingControl::new(services.trading_client()?.clone()));

    // Wrap StrategyManager and set trading control for hierarchical control
    let strategy_manager =
        strategy_manager.with_trading_control(trading_control.enabled_flag().clone());
    let strategy_manager_arc = Arc::new(RwLock::new(strategy_manager));
    let strategy_wrapper = Arc::new(RwLock::new(StrategyManagerWrapper::new(
        strategy_manager_arc.clone(),
    )));

    info!(
        "Creating Strategy Executor for {} strategy(ies) with {} pair(s)...",
        strategy_infos.len(),
        all_symbols.len()
    );

    let trading_reconciliation_interval_secs =
        bot_config.reconciliation.trading_level_interval_ms / 1000;

    // Create a dummy LevelManager for gRPC server (not used in strategies mode, but required by server)
    let dummy_level_manager = Arc::new(RwLock::new(LevelManager::new()));

    // Start gRPC server for CLI (use the same TradingControl instance)
    // Bind to :: (IPv6 wildcard) to accept both IPv4 and IPv6 connections
    let grpc_addr: std::net::SocketAddr = "[::]:50051"
        .parse()
        .map_err(|e| format!("Failed to parse gRPC address: {}", e))?;

    // Convert strategy infos to monitoring format
    let monitoring_strategy_infos: Vec<crate::grpc::service::StrategyInfo> = strategy_infos
        .iter()
        .map(
            |(name, symbol, max_orders)| crate::grpc::service::StrategyInfo {
                strategy_name: name.clone(),
                symbol: symbol.clone(),
                max_active_orders: *max_orders,
            },
        )
        .collect();

    info!(
        "Created {} monitoring strategy infos: {:?}",
        monitoring_strategy_infos.len(),
        monitoring_strategy_infos
            .iter()
            .map(|s| format!("{}:{}", s.strategy_name, s.symbol))
            .collect::<Vec<_>>()
    );

    // Get instrument type from config (defaults to Spot if not specified)
    let instrument_type = bot_config
        .get_instrument_type()
        .unwrap_or(trading_core::types::common::InstrumentType::Spot);

    // Create gRPC service with strategy manager for individual toggling
    use crate::grpc::service::TradingCliServiceImpl;
    let grpc_service = TradingCliServiceImpl::new(
        dummy_level_manager.clone(),
        services
            .trading_state_manager
            .clone()
            .ok_or("Trading state requires venue.mode \"cex\"")?,
        services.trading_client()?.clone(),
        services.exchange_state_manager()?.clone(),
        services.market_data_state_manager.clone(),
        services.indicator_service.clone(),
        services.indicator_calculator.clone(),
        trading_control.clone(),
    )
    .with_strategy_infos(monitoring_strategy_infos)
    .with_strategy_manager(strategy_manager_arc.clone())
    .with_instrument_type(instrument_type.clone())
    .with_exchange(format!("{:?}", bot_config.exchange_engine.exchange));

    let grpc_server = TradingGrpcServer::from_service(grpc_service);
    let grpc_handle = grpc_server.start(grpc_addr).await?;
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
        level_manager: Arc::clone(&dummy_level_manager),
        strategy_manager: Some(Arc::clone(&strategy_manager_arc)),
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

    // Pass all pairs to executor (no .first() - single source of truth)
    let executor = StrategyExecutor::new(
        strategy_wrapper.clone(),
        services.trading_client()?.clone(),
        services.market_data_rx.resubscribe(),
        all_symbols.clone(),
        services
            .trading_state_manager
            .clone()
            .ok_or("Trading state requires venue.mode \"cex\"")?,
        trading_reconciliation_interval_secs,
        services.asset_service.clone(),
        bot_config.exchange_engine.exchange.clone(),
        instrument_type,
        min_order_size,
        step_size,
        tick_size,
        min_notional,
    )
    .with_trading_control(trading_control.enabled_flag())
    .with_strategy_manager(strategy_manager_arc.clone());

    // Set up execution report handler (StrategyManager handles routing to all strategies)
    let strategy_manager_for_reports = strategy_manager_arc.clone();
    let shutdown_rx_reports = services.shutdown_manager.subscribe();
    let exec_report_rx = services.exec_report_rx()?;
    let _execution_report_handle = tokio::spawn(async move {
        execution_report_handler(exec_report_rx, shutdown_rx_reports, move |order| {
            let manager = Arc::clone(&strategy_manager_for_reports);
            async move {
                let manager_guard = manager.read().await;
                manager_guard
                    .on_execution_report(&order)
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                Ok(())
            }
        })
        .await;
    });

    // Get TradingState from TradingStateManager for health server
    let trading_state_for_health = services
        .trading_state_manager
        .clone()
        .ok_or("Trading state requires venue.mode \"cex\"")?
        .get_state();
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
    let health_server = TradingHealthServer::new(trading_state_for_health)
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

    info!("All systems ready! Starting strategy execution...");
    for (strategy_type, symbol, max_orders) in &strategy_infos {
        info!(
            "  - {} for {} (max orders: {})",
            strategy_type, symbol, max_orders
        );
    }

    let mut shutdown_rx_executor = services.shutdown_manager.subscribe();
    let executor_handle = tokio::spawn(async move {
        tokio::select! {
            result = executor.run() => {
                if let Err(e) = result {
                    error!("Strategy executor error: {e}");
                }
            }
            _ = shutdown_rx_executor.recv() => {
                info!("Strategy executor shutting down");
            }
        }
    });

    services.shutdown_manager.wait_for_shutdown().await;

    info!("Shutting down gracefully...");
    if let Ok(trading_client) = services.trading_client() {
        for symbol in &all_symbols {
            info!("Cancelling all open orders for {}...", symbol);
            match trading_client.write().await.cancel_all_orders(symbol).await {
                Ok(_) => info!("All orders cancelled successfully for {}", symbol),
                Err(e) => warn!(
                    "Failed to cancel all orders for {}: {error}",
                    symbol,
                    error = e
                ),
            }
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
            let _ = health_handle.await;
            let _ = executor_handle.await;
            let _ = services.indicator_handle.await;
            let _ = grpc_handle.await;
        } => {
            info!("All components shut down successfully");
        }
    }

    Ok(())
}
