pub mod config;
pub mod grpc;
pub mod shutdown;

use asset_engine::{AssetService, BinancePublicClient};
use indicator_engine::IndicatorService;
use market_data_engine::{
    config::{ExchangeConfig as MDExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
};
use tokio::sync::Mutex;

use crate::config::{BotConfig, StrategiesConfig};
use crate::grpc::server::TradingGrpcServer;
use crate::shutdown::ShutdownManager;
use live_config::{LiveConfigClient, LiveConfigService};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use trading_core::grpc::TradingControl;
use trading_core::persistence::mongo::client::MongoClient;
use trading_core::traits::TradingClient;
use trading_core::types::common::{Exchange, Pair};
use trading_core::types::exchange::OrderRequest;
use trading_engine::health::HealthServer as TradingHealthServer;
use trading_engine::levels::{
    CancellationManager, IndicatorSubscriber, LevelBasedEventLoop, LevelExecutor, LevelManager,
};
use trading_engine::risk::RiskManager;
use trading_engine::strategies::{
    BuyLowSellHigh, StrategyExecutor, StrategyManager, StrategyManagerWrapper,
};

/// Shared execution report handler
/// Processes execution reports from the exchange and calls the provided handler function
async fn execution_report_handler<F, Fut>(
    mut exchange_update_rx: trading_core::channels::exchange::ExchangeUpdateReceiver,
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
                    Ok(trading_core::channels::exchange::ExchangeUpdate::Order(order)) => {
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

/// Shared services needed for both modes
pub struct SharedServices {
    pub trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    pub exec_report_rx: trading_core::channels::exchange::ExchangeUpdateReceiver,
    pub market_data_rx: trading_core::channels::market_data::MarketDataUpdateReceiver,
    pub indicator_rx: trading_core::channels::indicators::IndicatorUpdateReceiver,
    pub indicator_service: Arc<Mutex<IndicatorService>>,
    pub indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
    pub shutdown_manager: ShutdownManager,
    pub market_data_handle: JoinHandle<()>,
    pub trading_ws_handle: JoinHandle<()>,
    pub indicator_handle: JoinHandle<()>,
    pub trading_state_manager: Arc<trading_engine::state::manager::TradingStateManager>,
    pub exchange_state_manager: Arc<exchange_engine::state::manager::ExchangeStateManager>,
    pub asset_service: Option<Arc<AssetService>>,
    pub trading_control: Option<Arc<trading_core::grpc::TradingControl>>,
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
        let strategy = Arc::new(RwLock::new(BuyLowSellHigh::new(
            services.trading_client.clone(),
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
        )));

        // Create strategy ID: "strategy_name:symbol"
        let strategy_id =
            trading_engine::strategies::strategy_manager::StrategyManager::make_strategy_id(
                &config.strategy_type.to_string(),
                pair,
            );
        strategy_manager
            .add_strategy(strategy, strategy_id.clone())
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

    // Wrap StrategyManager to work with StrategyExecutor
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

    // Create TradingControl for enable/disable trading
    let trading_control = Arc::new(TradingControl::new(services.trading_client.clone()));

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

    // Create gRPC service with strategy manager for individual toggling
    use crate::grpc::service::TradingCliServiceImpl;
    let grpc_service = TradingCliServiceImpl::new(
        dummy_level_manager.clone(),
        services.trading_state_manager.clone(),
        services.trading_client.clone(),
        services.exchange_state_manager.clone(),
        services.indicator_service.clone(),
        services.indicator_calculator.clone(),
        trading_control.clone(),
    )
    .with_strategy_infos(monitoring_strategy_infos)
    .with_strategy_manager(strategy_manager_arc.clone());

    let grpc_server = TradingGrpcServer::from_service(grpc_service);
    let grpc_handle = grpc_server.start(grpc_addr).await?;
    info!("gRPC server started on {}", grpc_addr);

    // Pass all pairs to executor (no .first() - single source of truth)
    let executor = StrategyExecutor::new(
        strategy_wrapper.clone(),
        services.trading_client.clone(),
        services.market_data_rx.resubscribe(),
        all_symbols.clone(),
        services.trading_state_manager.clone(),
        trading_reconciliation_interval_secs,
        services.asset_service.clone(),
        bot_config.exchange.exchange.clone(),
        min_order_size,
        step_size,
        tick_size,
        min_notional,
    )
    .with_trading_control(trading_control.enabled_flag());

    // Set up execution report handler (StrategyManager handles routing to all strategies)
    let strategy_manager_for_reports = strategy_manager_arc.clone();
    let shutdown_rx_reports = services.shutdown_manager.subscribe();
    let exec_report_rx = services.exec_report_rx;
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
    let trading_state_for_health = services.trading_state_manager.get_state();
    let health_server = TradingHealthServer::new(trading_state_for_health);
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
    // Cancel orders for all symbols
    for symbol in &all_symbols {
        info!("Cancelling all open orders for {}...", symbol);
        match services
            .trading_client
            .write()
            .await
            .cancel_all_orders(symbol)
            .await
        {
            Ok(_) => info!("All orders cancelled successfully for {}", symbol),
            Err(e) => warn!(
                "Failed to cancel all orders for {}: {error}",
                symbol,
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

    // Try to load from MongoDB first (if enabled)
    let risk_limits = if level_persistence.is_some() {
        if let Ok(Some(mongo_risk_limits)) = level_manager.load_from_mongo().await {
            info!("Loaded levels configuration from MongoDB");
            mongo_risk_limits
        } else {
            // Fall back to file if MongoDB load fails
            info!(
                "Loading levels configuration from {}...",
                levels_config_path
            );
            level_manager
                .load_from_config_with_risk_limits(Path::new(&levels_config_path))
                .await
                .map_err(|e| format!("Failed to load levels config: {e}"))?
        }
    } else {
        // Load from file
        info!(
            "Loading levels configuration from {}...",
            levels_config_path
        );
        level_manager
            .load_from_config_with_risk_limits(Path::new(&levels_config_path))
            .await
            .map_err(|e| format!("Failed to load levels config: {e}"))?
    };

    // Save to MongoDB if enabled (after loading from file or MongoDB)
    if level_persistence.is_some() {
        if let Err(e) = level_manager.save_all_levels(&risk_limits).await {
            warn!("Failed to save levels to MongoDB: {}", e);
        } else {
            info!("Saved levels configuration to MongoDB");
        }
    }

    info!("Loaded {} levels", level_manager.get_all_levels().len());

    // Get pairs from exchange config
    let configured_pairs = bot_config.get_pairs()?;
    if configured_pairs.is_empty() {
        return Err("No pairs configured in bot.json".into());
    }

    // Validate that all level symbols match pairs from bot config
    let level_manager_guard = level_manager.get_all_levels();
    for level in level_manager_guard.iter() {
        let level_symbol = &level.config.symbol;
        if !configured_pairs
            .iter()
            .any(|p| p.base == level_symbol.base && p.quote == level_symbol.quote)
        {
            warn!(
                "Level {} uses symbol {} which is not in bot.json pairs. Available pairs: {:?}",
                level.config.id, level_symbol, configured_pairs
            );
        }
    }
    drop(level_manager_guard);

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

    let level_executor = Arc::new(
        LevelExecutor::new(
            services.trading_client.clone(),
            services.asset_service.clone(),
            bot_config.exchange.exchange.clone(),
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
    let grpc_server = TradingGrpcServer::new(
        level_manager.clone(),
        services.trading_state_manager.clone(),
        services.trading_client.clone(),
        services.exchange_state_manager.clone(),
        services.indicator_service.clone(),
        services.indicator_calculator.clone(),
        trading_control.clone(), // Pass the same TradingControl instance
    );
    let grpc_handle = grpc_server.start(grpc_addr).await?;
    info!("gRPC server started on {}", grpc_addr);

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

    // Create indicator subscriber using the shared indicator receiver
    let indicator_subscriber =
        Arc::new(RwLock::new(IndicatorSubscriber::new(services.indicator_rx)));

    // Start indicator subscriber task
    let subscriber_for_task = Arc::clone(&indicator_subscriber);
    let mut shutdown_rx_subscriber = services.shutdown_manager.subscribe();
    let subscriber_handle = tokio::spawn(async move {
        let mut subscriber = subscriber_for_task.write().await;
        tokio::select! {
            result = subscriber.run() => {
                if let Err(e) = result {
                    error!("Indicator subscriber error: {e}");
                }
            }
            _ = shutdown_rx_subscriber.recv() => {
                info!("Indicator subscriber shutting down");
            }
        }
    });

    // Create level-based event loop
    // Pass all pairs from bot config for reconciliation
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
        configured_pairs.clone(),
        services.trading_state_manager.clone(),
        trading_reconciliation_interval_secs,
    );

    // Note: TradingStateManager automatically handles execution reports via ExchangeStateManager subscription
    // Execution reports are published through the exchange_update channel and handled by TradingStateManager
    // No separate execution report handler needed - TradingStateManager is the single source of truth

    // Set up health server using TradingStateManager's state
    let health_server = TradingHealthServer::new(services.trading_state_manager.get_state());
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

// ============================================================================
// Initialization Functions
// ============================================================================

/// Initialize MongoDB client if enabled
/// Initialize MongoDB connection
///
/// MongoDB is REQUIRED for the trading system to operate.
/// This function will fail if MongoDB is not available or connection fails.
pub async fn initialize_mongodb(
    bot_config: &BotConfig,
) -> Result<Arc<MongoClient>, Box<dyn std::error::Error>> {
    if !bot_config.mongo.enabled {
        return Err("MongoDB is required but is disabled in configuration. Set 'mongo.enabled' to true in config/bot.json".into());
    }

    info!(
        "Connecting to MongoDB at {}/{}...",
        bot_config.mongo.connection_string, bot_config.mongo.database
    );

    let client = MongoClient::new(
        bot_config.mongo.connection_string.as_str(),
        bot_config.mongo.database.as_str(),
    )
    .await
    .map_err(|e| {
        format!(
            "MongoDB connection failed: {}. Make sure MongoDB is running and accessible at {}",
            e, bot_config.mongo.connection_string
        )
    })?;

    info!(
        "MongoDB: Connected to {}/{}",
        bot_config.mongo.connection_string, bot_config.mongo.database
    );

    Ok(Arc::new(client))
}

/// Setup Ctrl-C shutdown handler
pub fn setup_shutdown_handler(shutdown_manager: ShutdownManager) {
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to listen for Ctrl-C: {e}");
            return;
        }
        info!("Ctrl-C received, initiating graceful shutdown");
        shutdown_manager.signal_shutdown();
    });
}

/// Initialize Asset Service and register exchange client
pub async fn initialize_asset_service(
    exchange: Exchange,
) -> Result<Arc<AssetService>, Box<dyn std::error::Error>> {
    info!("Initializing Asset Engine...");
    let asset_service = Arc::new(AssetService::new());

    let base_url = match exchange {
        Exchange::Binance => "https://api.binance.com",
        Exchange::Unsupported => {
            return Err("Unsupported exchange for Asset Engine".into());
        }
    };

    asset_service
        .register_exchange(
            exchange,
            Arc::new(BinancePublicClient::new(base_url.to_string())),
        )
        .await
        .map_err(|e| format!("Failed to register exchange client: {e}"))?;

    Ok(asset_service)
}

/// Fetch asset data for configured pairs
pub async fn fetch_asset_data(
    asset_service: &Arc<AssetService>,
    exchange: Exchange,
    pairs: &[Pair],
) {
    info!(
        "Fetching asset data for {} configured pair(s): {:?}",
        pairs.len(),
        pairs
    );

    match asset_service.fetch_market_data(exchange, pairs).await {
        Ok(market_data) => {
            info!(
                "Asset data fetched: {} symbols, {} trading rules",
                market_data.symbol_info.len(),
                market_data.trading_rules.len()
            );
        }
        Err(e) => {
            warn!(
                "Failed to fetch asset data: {}. System will continue with config fallback.",
                e
            );
        }
    }
}

/// Initialize Market Data Engine
pub fn initialize_market_data_engine(
    bot_config: &BotConfig,
    shutdown_manager: &ShutdownManager,
) -> Result<
    (
        trading_core::channels::market_data::MarketDataUpdateSender,
        trading_core::channels::market_data::MarketDataUpdateReceiver,
        JoinHandle<()>,
    ),
    Box<dyn std::error::Error>,
> {
    use market_data_engine::exchanges::binance::config::BinanceConfig as MDBinanceConfig;

    info!(
        "Starting Market Data Engine (exchange: {:?})...",
        bot_config.exchange.exchange
    );

    // Get pairs from exchange config
    let pairs = bot_config.get_pairs()?;
    if pairs.is_empty() {
        return Err("No pairs configured in bot.json".into());
    }

    // Extract exchange-specific config
    // Pairs come from bot.json - single source of truth
    let market_data_config = match bot_config.exchange.exchange {
        Exchange::Binance => {
            let binance_config_json = bot_config
                .exchange
                .exchange_specific
                .get("binance")
                .ok_or("Binance exchange configuration is required")?;
            let binance_config: exchange_engine::exchanges::binance::config::BinanceConfig =
                serde_json::from_value(binance_config_json.clone())
                    .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;

            MarketDataConfig {
                redis_url: None,
                exchange: bot_config.exchange.exchange.clone(),
                exchange_config: MDExchangeConfig::Binance(MDBinanceConfig {
                    pairs: pairs.clone(), // Single source of truth from bot.json
                    limit: binance_config.limit.unwrap_or(100),
                    base_delay: binance_config.base_delay,
                    max_delay: binance_config.max_delay,
                }),
            }
        }
        Exchange::Unsupported => {
            return Err("Unsupported exchange for Market Data Engine".into());
        }
    };

    let (market_data_tx, market_data_rx) =
        trading_core::channels::market_data::market_data_update_channel(
            bot_config.channels.market_data_capacity,
        );

    let mut market_data_driver = MarketDataDriver::new(
        market_data_config,
        market_data_tx.clone(),
        bot_config.channels.clone(),
        pairs.clone(),
    )?;

    let mut shutdown_rx = shutdown_manager.subscribe();
    let handle = tokio::spawn(async move {
        tokio::select! {
            result = market_data_driver.run(Method::Orderbooks) => {
                if let Err(e) = result {
                    error!("Market data engine error: {e}");
                }
            }
            _ = shutdown_rx.recv() => {
                info!("Market data engine shutting down");
            }
        }
    });

    Ok((market_data_tx, market_data_rx, handle))
}

/// Initialize Indicator Service
/// Returns the service, handle, and calculator reference for use by runners
/// MongoDB is REQUIRED for indicator persistence
pub async fn initialize_indicator_service(
    _bot_config: &BotConfig,
    mongo_client: &Arc<MongoClient>,
    market_data_rx: trading_core::channels::market_data::MarketDataUpdateReceiver,
    indicator_tx: trading_core::channels::indicators::IndicatorUpdateSender,
    shutdown_manager: &ShutdownManager,
    configured_pairs: &[trading_core::types::common::Pair],
) -> Result<
    (
        Arc<Mutex<IndicatorService>>,
        JoinHandle<()>,
        Arc<indicator_engine::calculator::IndicatorCalculator>,
    ),
    Box<dyn std::error::Error>,
> {
    info!("Initializing Indicator Service (MongoDB persistence: enabled)...");

    let indicator_market_data_rx = market_data_rx.resubscribe();

    // Create indicator service with MongoDB persistence (required)
    let persistence = Arc::new(indicator_engine::persistence::IndicatorPersistence::new(
        mongo_client.as_ref().clone(),
    ));
    let indicator_service = Arc::new(Mutex::new(
        IndicatorService::new().with_persistence(persistence),
    ));

    indicator_service
        .lock()
        .await
        .initialize(indicator_market_data_rx, indicator_tx, 1);

    // Load indicators from MongoDB (non-fatal - system can run without pre-configured indicators)
    let loaded_count = match indicator_service.lock().await.load_from_mongo().await {
        Ok(count) => {
            info!("Loaded {} indicator(s) from MongoDB", count);
            count
        }
        Err(e) => {
            warn!(
                "Failed to load indicators from MongoDB: {}. System will continue without pre-configured indicators. You can add indicators via the API or they will be created on-demand.",
                e
            );
            0 // Continue with 0 indicators - system can still work
        }
    };

    // Log indicator status (non-fatal warning if none loaded)
    if loaded_count == 0 {
        warn!(
            "WARNING: No indicators loaded from MongoDB. The database may be empty or indicators haven't been configured yet. System will continue and indicators can be created on-demand."
        );
        warn!(
            "To add indicators, use the indicator management API or ensure indicators are configured in MongoDB collection 'indicators'."
        );
        warn!(
            "The system will continue with default indicators (SMA, EMA), but trading may be limited without proper indicator configuration."
        );
    } else {
        info!(
            "Successfully loaded {} indicator(s) from MongoDB",
            loaded_count
        );
    }

    // Register default indicators (SMA and EMA) for all configured pairs
    info!(
        "Registering default indicators (SMA, EMA) for {} configured pair(s): {:?}",
        configured_pairs.len(),
        configured_pairs
    );

    let mut registered_count = 0;
    for pair in configured_pairs {
        let pair_str = pair.convert_to_smolstr();
        info!("Processing pair: {} (formatted as: {})", pair, pair_str);

        // Register SMA indicator
        let sma_id = smol_str::SmolStr::new(format!("sma_20_{}", pair_str));
        let sma_config = indicator_engine::indicators::IndicatorConfig {
            id: sma_id.clone(),
            name: smol_str::SmolStr::new("SMA"),
            pair: pair_str.clone(),
            period: 20,
            source: indicator_engine::indicators::PriceSource::Close,
        };
        match indicator_service
            .lock()
            .await
            .register_indicator(sma_config.clone())
            .await
        {
            Ok(_) => {
                info!(
                    "✓ Registered SMA indicator: {} for pair: {}",
                    sma_id, pair_str
                );
                registered_count += 1;
            }
            Err(e) => {
                warn!("✗ Failed to register SMA indicator for {}: {e}", pair_str);
            }
        }

        // Register EMA indicator
        let ema_id = smol_str::SmolStr::new(format!("ema_12_{}", pair_str));
        let ema_config = indicator_engine::indicators::IndicatorConfig {
            id: ema_id.clone(),
            name: smol_str::SmolStr::new("EMA"),
            pair: pair_str.clone(),
            period: 12,
            source: indicator_engine::indicators::PriceSource::Close,
        };
        match indicator_service
            .lock()
            .await
            .register_indicator(ema_config.clone())
            .await
        {
            Ok(_) => {
                info!(
                    "✓ Registered EMA indicator: {} for pair: {}",
                    ema_id, pair_str
                );
                registered_count += 1;
            }
            Err(e) => {
                warn!("✗ Failed to register EMA indicator for {}: {e}", pair_str);
            }
        }
    }

    info!(
        "Successfully registered {} indicator(s) for {} pair(s)",
        registered_count,
        configured_pairs.len()
    );

    // Log total indicator count after registration
    let total_indicators = indicator_service.lock().await.get_indicator_count();
    info!(
        "Total indicators registered: {} (across {} pair(s))",
        total_indicators,
        configured_pairs.len()
    );

    // Get calculator reference BEFORE spawning the task (which will hold the lock forever)
    let indicator_calculator = {
        let service = indicator_service.lock().await;
        service.calculator()
    };

    let mut shutdown_rx = shutdown_manager.subscribe();
    let indicator_for_task = indicator_service.clone();
    Ok((
        indicator_service,
        tokio::spawn(async move {
            tokio::select! {
                result = async {
                    let mut guard = indicator_for_task.lock().await;
                    guard.run().await
                } => {
                    if let Err(e) = result {
                        error!("Indicator service error: {e}");
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Indicator service shutting down");
                }
            }
        }),
        indicator_calculator,
    ))
}

/// Initialize Exchange Engine
pub async fn initialize_exchange_engine(
    bot_config: &BotConfig,
    shutdown_manager: &ShutdownManager,
) -> Result<
    (
        Arc<exchange_engine::state::manager::ExchangeStateManager>,
        trading_core::channels::exchange::ExchangeUpdateReceiver,
        Arc<
            tokio::sync::RwLock<
                dyn trading_core::traits::TradingClient<
                    trading_core::types::exchange::OrderRequest,
                >,
            >,
        >,
    ),
    Box<dyn std::error::Error>,
> {
    use exchange_engine::{config::ExchangeEngineConfig, run};

    let reconciliation_enabled =
        bot_config.features.enable_reconciliation && bot_config.reconciliation.enabled;
    let exchange_reconciliation_interval_secs =
        bot_config.reconciliation.exchange_level_interval_ms / 1000;

    info!(
        "Starting Exchange Engine (exchange: {:?}, reconciliation: {})...",
        bot_config.exchange.exchange,
        if reconciliation_enabled {
            format!(
                "enabled (interval: {}s)",
                exchange_reconciliation_interval_secs
            )
        } else {
            "disabled".to_string()
        }
    );

    // Extract exchange-specific config and get pairs
    let (exchange_engine_config, pairs) = match bot_config.exchange.exchange {
        Exchange::Binance => {
            let binance_config_json = bot_config
                .exchange
                .exchange_specific
                .get("binance")
                .ok_or("Binance exchange configuration is required")?;
            let binance_config: exchange_engine::exchanges::binance::config::BinanceConfig =
                serde_json::from_value(binance_config_json.clone())
                    .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;

            // Validate pairs
            if binance_config.pairs.is_empty() {
                return Err("No pairs configured in binance config".into());
            }

            // Pairs are now the only source of truth - no symbol field needed

            let pairs = binance_config.pairs.clone();

            let exchange_config = ExchangeEngineConfig {
                exchange: Exchange::Binance,
                exchange_config: exchange_engine::config::ExchangeConfig::Binance(binance_config),
            };

            (exchange_config, pairs)
        }
        Exchange::Unsupported => {
            return Err("Unsupported exchange for Exchange Engine".into());
        }
    };

    let shutdown_rx = shutdown_manager.subscribe();
    let (exchange_state_manager, exchange_update_rx, trading_client) = run::run(
        exchange_engine_config,
        pairs,
        shutdown_rx,
        exchange_reconciliation_interval_secs,
        reconciliation_enabled,
        bot_config.channels.clone(),
    )
    .await?;

    info!("Exchange Engine: Bootstrap reconciliation completed");
    Ok((exchange_state_manager, exchange_update_rx, trading_client))
}
