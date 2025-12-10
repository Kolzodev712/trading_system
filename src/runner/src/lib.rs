pub mod config;
pub mod shutdown;

use asset_engine::{AssetService, BinancePublicClient};
use indicator_engine::IndicatorService;
use market_data_engine::{
    config::{ExchangeConfig as MDExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
};

use crate::config::{BotConfig, StrategiesConfig};
use crate::shutdown::ShutdownManager;
use live_config::{LiveConfigClient, LiveConfigService};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use trading_core::persistence::mongo::client::MongoClient;
use trading_core::traits::TradingClient;
use trading_core::types::common::{Exchange, Pair};
use trading_core::types::exchange::OrderRequest;
use trading_engine::health::HealthServer as TradingHealthServer;
use trading_engine::levels::{
    CancellationManager, IndicatorSubscriber, LevelBasedEventLoop, LevelExecutor, LevelManager,
};
use trading_engine::risk::RiskManager;
use trading_engine::state::Strategy;
use trading_engine::strategies::{BuyLowSellHigh, StrategyExecutor};

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
    pub shutdown_manager: ShutdownManager,
    pub market_data_handle: JoinHandle<()>,
    pub trading_ws_handle: JoinHandle<()>,
    pub indicator_handle: JoinHandle<()>,
    pub trading_state_manager: Arc<trading_engine::state::manager::TradingStateManager>,
    pub asset_service: Option<Arc<AssetService>>,
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

    info!(
        "Creating {} Strategy...",
        strategies_config.strategy_type.as_str()
    );
    let strategy = Arc::new(RwLock::new(BuyLowSellHigh::new(
        services.trading_client.clone(),
        strategies_config.symbol.clone(),
        strategies_config.position_size,
        strategies_config.max_position,
        strategies_config.buy_threshold_percent,
        strategies_config.sell_threshold_percent,
        strategies_config.stop_loss_percent,
        strategies_config.take_profit_percent,
        strategies_config.max_active_orders,
        strategies_config.price_history_size,
        strategies_config.min_price_history,
    )));

    info!("Creating Strategy Executor...");
    let min_order_size = strategies_config
        .min_order_size
        .unwrap_or_else(|| rust_decimal::Decimal::new(1, 5));
    let step_size = strategies_config
        .step_size
        .unwrap_or_else(|| rust_decimal::Decimal::new(1, 5));
    let tick_size = strategies_config
        .tick_size
        .unwrap_or_else(|| rust_decimal::Decimal::new(1, 2));
    let min_notional = strategies_config
        .min_notional
        .unwrap_or_else(|| rust_decimal::Decimal::new(5, 0));

    let trading_reconciliation_interval_secs =
        bot_config.reconciliation.trading_level_interval_ms / 1000;
    let executor = StrategyExecutor::new(
        strategy.clone(),
        services.trading_client.clone(),
        services.market_data_rx.resubscribe(),
        strategies_config.symbol.clone(),
        services.trading_state_manager.clone(),
        trading_reconciliation_interval_secs,
        services.asset_service.clone(),
        bot_config.exchange.exchange.clone(),
        min_order_size,
        step_size,
        tick_size,
        min_notional,
    );

    // Set up execution report handler
    let strategy_for_reports = Arc::clone(&strategy);
    let shutdown_rx_reports = services.shutdown_manager.subscribe();
    let exec_report_rx = services.exec_report_rx;
    let _execution_report_handle = tokio::spawn(async move {
        execution_report_handler(exec_report_rx, shutdown_rx_reports, move |order| {
            let strategy = Arc::clone(&strategy_for_reports);
            async move {
                let mut strat = strategy.write().await;
                strat
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
    info!("Strategy: {}", strategies_config.strategy_type.as_str());
    info!("Position size: {}", strategies_config.position_size);
    info!("Max position: {}", strategies_config.max_position);
    info!("Symbol: {}", strategies_config.symbol);

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
    info!(
        "Cancelling all open orders for {}...",
        strategies_config.symbol
    );
    match services
        .trading_client
        .write()
        .await
        .cancel_all_orders(&strategies_config.symbol)
        .await
    {
        Ok(_) => info!("All orders cancelled successfully"),
        Err(e) => warn!("Failed to cancel all orders: {error}", error = e),
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
    if let Some(_) = level_persistence {
        if let Err(e) = level_manager.save_all_levels(&risk_limits).await {
            warn!("Failed to save levels to MongoDB: {}", e);
        } else {
            info!("Saved levels configuration to MongoDB");
        }
    }

    info!("Loaded {} levels", level_manager.get_all_levels().len());

    // Extract symbol from first level (needed for symbol registry)
    let symbol = {
        let level_manager_guard = level_manager.get_all_levels();
        level_manager_guard
            .first()
            .map(|l| l.config.symbol.clone())
            .ok_or("No levels configured")?
    };

    // Create symbol registry for risk manager
    let symbol_registry = Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
        vec![symbol.clone()],
    ));

    // Create level executor with AssetService integration
    // Use defaults for config values (Asset Engine will override if available)
    let config_min_order_size = rust_decimal::Decimal::new(1, 5); // 0.00001
    let config_step_size = rust_decimal::Decimal::new(1, 5); // 0.00001
    let config_tick_size = rust_decimal::Decimal::new(1, 2); // 0.01
    let config_min_notional = rust_decimal::Decimal::new(5, 0); // 5.0

    let level_executor = Arc::new(LevelExecutor::new(
        services.trading_client.clone(),
        services.asset_service.clone(),
        bot_config.exchange.exchange.clone(),
        config_min_order_size,
        config_step_size,
        config_tick_size,
        config_min_notional,
    ));

    // Create risk manager with symbol registry
    let risk_manager = Arc::new(RiskManager::new(risk_limits, symbol_registry.clone()));

    // Wrap level_manager in Arc<RwLock<>> for sharing with LiveConfig and event loop
    let level_manager = Arc::new(RwLock::new(level_manager));

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
    let indicator_subscriber = Arc::new(RwLock::new(IndicatorSubscriber::new(services.indicator_rx)));

    // Start indicator subscriber task
    let subscriber_for_task = Arc::clone(&indicator_subscriber);
    let mut shutdown_rx_subscriber = services.shutdown_manager.subscribe();
    let subscriber_handle = tokio::spawn(async move {
        let mut subscriber = subscriber_for_task.write().await;
        loop {
            tokio::select! {
                result = subscriber.run() => {
                    if let Err(e) = result {
                        error!("Indicator subscriber error: {e}");
                    }
                    break;
                }
                _ = shutdown_rx_subscriber.recv() => {
                    info!("Indicator subscriber shutting down");
                    break;
                }
            }
        }
    });

    // Create level-based event loop
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
        symbol.clone(),
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
    info!("Starting level-based trading for symbol: {}", symbol);

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
    info!("Cancelling all open orders for {}...", symbol);
    match services
        .trading_client
        .write()
        .await
        .cancel_all_orders(&symbol)
        .await
    {
        Ok(_) => info!("All orders cancelled successfully"),
        Err(e) => warn!("Failed to cancel all orders: {error}", error = e),
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
        } => {
            info!("All components shut down successfully");
        }
    }

    Ok(())
}

// ============================================================================
// Initialization Functions
// ============================================================================

/// Extract the configured pair from BotConfig (exchange-agnostic)
fn extract_bot_pair(bot_config: &BotConfig) -> Result<Pair, Box<dyn std::error::Error>> {
    match bot_config.exchange.exchange {
        Exchange::Binance => {
            let binance_config_json = bot_config
                .exchange
                .exchange_specific
                .get("binance")
                .ok_or("Binance exchange configuration is required")?;
            let binance_config: exchange_engine::exchanges::binance::config::BinanceConfig =
                serde_json::from_value(binance_config_json.clone())
                    .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;
            Ok(binance_config.symbol.clone().into())
        }
        Exchange::Unsupported => Err("Unsupported exchange".into()),
    }
}

/// Collect all configured pairs from bot config, levels config, and strategies config
pub fn collect_configured_pairs(
    bot_config: &BotConfig,
    mode: &str,
    levels_config_path: &str,
    strategies_config_path: &str,
) -> Result<Vec<Pair>, Box<dyn std::error::Error>> {
    let mut pairs = HashSet::new();
    let bot_pair = extract_bot_pair(bot_config)?;
    pairs.insert(bot_pair);

    match mode {
        "levels" => {
            if let Ok(levels) = load_pairs_from_levels_config(levels_config_path) {
                pairs.extend(levels);
            }
        }
        "strategies" => {
            if let Ok(pair) = load_pair_from_strategies_config(strategies_config_path) {
                pairs.insert(pair);
            }
        }
        _ => {}
    }

    Ok(pairs.into_iter().collect())
}

/// Load pairs from levels config file
fn load_pairs_from_levels_config(path: &str) -> Result<Vec<Pair>, Box<dyn std::error::Error>> {
    use std::fs;
    use trading_engine::levels::config::LevelConfig;

    let content = fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    let levels: Vec<LevelConfig> = json
        .get("levels")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'levels' array in config")?
        .iter()
        .filter_map(|level_json| serde_json::from_value(level_json.clone()).ok())
        .collect();

    Ok(levels.iter().map(|level| level.symbol.clone()).collect())
}

/// Load pair from strategies config file
fn load_pair_from_strategies_config(path: &str) -> Result<Pair, Box<dyn std::error::Error>> {
    let config = StrategiesConfig::load_from_file(path)?;
    Ok(config.symbol)
}

/// Initialize MongoDB client if enabled
pub async fn initialize_mongodb(bot_config: &BotConfig) -> Option<Arc<MongoClient>> {
    if !bot_config.mongo.enabled {
        return None;
    }

    match MongoClient::new(
        bot_config.mongo.connection_string.as_str(),
        bot_config.mongo.database.as_str(),
    )
    .await
    {
        Ok(client) => {
            info!(
                "MongoDB: Connected to {}/{}",
                bot_config.mongo.connection_string, bot_config.mongo.database
            );
            Some(Arc::new(client))
        }
        Err(e) => {
            error!("MongoDB: Connection failed: {e}");
            None
        }
    }
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
    use market_data_engine::exchanges::binance::websocket::types::common::BinanceSymbol as MDBinanceSymbol;

    info!(
        "Starting Market Data Engine (exchange: {:?})...",
        bot_config.exchange.exchange
    );

    // Extract exchange-specific config
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
                    symbol: MDBinanceSymbol {
                        base: binance_config.symbol.base.clone(),
                        quote: binance_config.symbol.quote.clone(),
                    },
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
/// Returns both the service handle and the indicator update receiver for use by runners
pub async fn initialize_indicator_service(
    bot_config: &BotConfig,
    mongo_client: Option<&Arc<MongoClient>>,
    market_data_rx: trading_core::channels::market_data::MarketDataUpdateReceiver,
    indicator_tx: trading_core::channels::indicators::IndicatorUpdateSender,
    shutdown_manager: &ShutdownManager,
) -> Result<JoinHandle<()>, Box<dyn std::error::Error>> {
    let default_pair = extract_bot_pair(bot_config)?;

    info!(
        "Initializing Indicator Service (MongoDB persistence: {})...",
        if mongo_client.is_some() {
            "enabled"
        } else {
            "disabled"
        }
    );

    let indicator_market_data_rx = market_data_rx.resubscribe();

    let mut indicator_service = if let Some(client) = mongo_client {
        let persistence = Arc::new(indicator_engine::persistence::IndicatorPersistence::new(
            client.as_ref().clone(),
        ));
        IndicatorService::new().with_persistence(persistence)
    } else {
        IndicatorService::new()
    };

    indicator_service.initialize(indicator_market_data_rx, indicator_tx, 1);

    if mongo_client.is_some() {
        if let Err(e) = indicator_service.load_from_mongo().await {
            warn!("Failed to load indicators from MongoDB: {e}");
        }
    }

    // Register default indicators
    let pair_str = default_pair.convert_to_smolstr();
    let sma_config = indicator_engine::indicators::IndicatorConfig {
        id: smol_str::SmolStr::new(format!("sma_20_{}", pair_str)),
        name: smol_str::SmolStr::new("SMA"),
        pair: pair_str.clone(),
        period: 20,
        source: indicator_engine::indicators::PriceSource::Close,
    };
    if let Err(e) = indicator_service.register_indicator(sma_config).await {
        warn!("Failed to register SMA indicator: {e}");
    }

    let ema_config = indicator_engine::indicators::IndicatorConfig {
        id: smol_str::SmolStr::new(format!("ema_12_{}", pair_str)),
        name: smol_str::SmolStr::new("EMA"),
        pair: pair_str,
        period: 12,
        source: indicator_engine::indicators::PriceSource::Close,
    };
    if let Err(e) = indicator_service.register_indicator(ema_config).await {
        warn!("Failed to register EMA indicator: {e}");
    }

    let mut shutdown_rx = shutdown_manager.subscribe();
    Ok(tokio::spawn(async move {
        tokio::select! {
            result = indicator_service.run() => {
                if let Err(e) = result {
                    error!("Indicator service error: {e}");
                }
            }
            _ = shutdown_rx.recv() => {
                info!("Indicator service shutting down");
            }
        }
    }))
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

    // Extract exchange-specific config
    let exchange_engine_config = match bot_config.exchange.exchange {
        Exchange::Binance => {
            let binance_config_json = bot_config
                .exchange
                .exchange_specific
                .get("binance")
                .ok_or("Binance exchange configuration is required")?;
            let binance_config: exchange_engine::exchanges::binance::config::BinanceConfig =
                serde_json::from_value(binance_config_json.clone())
                    .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;

            ExchangeEngineConfig {
                exchange: Exchange::Binance,
                exchange_config: exchange_engine::config::ExchangeConfig::Binance(binance_config),
            }
        }
        Exchange::Unsupported => {
            return Err("Unsupported exchange for Exchange Engine".into());
        }
    };

    let shutdown_rx = shutdown_manager.subscribe();
    let (exchange_state_manager, exchange_update_rx, trading_client) = run::run(
        exchange_engine_config,
        shutdown_rx,
        exchange_reconciliation_interval_secs,
        reconciliation_enabled,
        bot_config.channels.clone(),
    )
    .await?;

    info!("Exchange Engine: Bootstrap reconciliation completed");
    Ok((exchange_state_manager, exchange_update_rx, trading_client))
}
