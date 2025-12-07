use exchange_engine::exchanges::binance::{config::BinanceConfig, ws_client::BinanceClient};
use indicator_engine::indicators::{IndicatorConfig, PriceSource};
use indicator_engine::IndicatorService;
use market_data_engine::{
    config::{ExchangeConfig as MDExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
};
use runner::{health::HealthServer, shutdown::ShutdownManager};
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use trading_core::{
    channels::{indicator_update_channel, market_data_update_channel},
    config::TradingSystemConfig,
    traits::TradingClient,
    types::common::Pair,
};
use trading_engine::{executor::StrategyExecutor, strategies::BuyLowSellHigh, strategy::Strategy};

mod health;
mod shutdown;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = TradingSystemConfig::load_from_file("config/trading_system.json")?;
    config.validate()?;

    // Initialize logging
    let log_level = match config.system.log_level.as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };
    // Simple logging initialization
    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(log_level)
            .finish(),
    );

    info!("Starting Trading System");

    // Create shutdown manager
    let shutdown_manager = ShutdownManager::new();

    // Set up Ctrl-C handler
    let shutdown_manager_clone = shutdown_manager.clone();
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to listen for Ctrl-C: {e}");
            return;
        }
        info!("Ctrl-C received, initiating graceful shutdown");
        shutdown_manager_clone.signal_shutdown();
    });

    // Convert unified config to exchange-specific configs
    // Extract the exchange-specific config from JSON
    let binance_config_json = config
        .exchange
        .exchange_specific
        .get("binance")
        .ok_or("Binance exchange configuration is required")?;

    let binance_config: BinanceConfig = serde_json::from_value(binance_config_json.clone())
        .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;

    let symbol: Pair = binance_config.symbol.clone().into();

    // 1. Start Market Data Engine
    info!("Starting Market Data Engine...");
    // Convert exchange_engine BinanceConfig to market_data_engine BinanceConfig
    // The market_data_engine version has a different structure
    use market_data_engine::exchanges::binance::config::BinanceConfig as MDBinanceConfig;
    use market_data_engine::exchanges::binance::websocket::types::common::BinanceSymbol as MDBinanceSymbol;

    let market_data_binance_config = MDBinanceConfig {
        symbol: MDBinanceSymbol {
            base: binance_config.symbol.base.clone(),
            quote: binance_config.symbol.quote.clone(),
        },
        limit: binance_config.limit.unwrap_or(100),
        base_delay: binance_config.base_delay,
        max_delay: binance_config.max_delay,
    };
    let market_data_config = MarketDataConfig {
        redis_url: config.redis.url.clone(),
        exchange: config.exchange.exchange,
        exchange_config: MDExchangeConfig::Binance(market_data_binance_config),
    };
    let mut market_data_driver = MarketDataDriver::new(market_data_config)?;

    let mut shutdown_rx_md = shutdown_manager.subscribe();
    let market_data_handle = tokio::spawn(async move {
        tokio::select! {
            result = market_data_driver.run(Method::Orderbooks) => {
                if let Err(e) = result {
                    error!("Market data engine error: {e}");
                }
            }
            _ = shutdown_rx_md.recv() => {
                info!("Market data engine shutting down");
            }
        }
    });

    // Wait for market data to start and validate Redis connectivity
    info!("Validating Redis connectivity...");
    let redis_url: redis::ConnectionInfo = config.redis.url.parse().map_err(|e| {
        format!(
            "Invalid Redis URL '{url}': {error}",
            url = config.redis.url,
            error = e
        )
    })?;

    let redis_client = redis::Client::open(redis_url.clone())
        .map_err(|e| format!("Failed to create Redis client: {error}", error = e))?;

    // Test Redis connection
    let mut test_conn = redis_client.get_connection().map_err(|e| {
        format!(
            "Failed to connect to Redis: {error}. Please ensure Redis is running.",
            error = e
        )
    })?;

    let _: () = redis::cmd("PING")
        .query(&mut test_conn)
        .map_err(|e| format!("Redis PING failed: {error}", error = e))?;

    info!("Redis connection validated successfully");

    // Wait for market data to start publishing
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 1.5. Initialize Indicator Service
    // NOTE: Currently, the Indicator Service is set up but won't receive data until
    // the Market Data Engine is updated to publish to channels (Proposal 04).
    // For now, this creates the structure and registers indicators.
    info!("Initializing Indicator Service...");
    let (_market_data_tx, market_data_rx) = market_data_update_channel();
    let (indicator_tx, _indicator_rx) = indicator_update_channel();

    let mut indicator_service = IndicatorService::new();
    indicator_service.initialize(
        market_data_rx,
        indicator_tx,
        1, // Publish every 1 second
    );

    // Register default indicators (can be configured later)
    let pair_str = symbol.convert_to_smolstr();
    let sma_config = IndicatorConfig {
        id: SmolStr::new(format!("sma_20_{}", pair_str)),
        name: SmolStr::new("SMA"),
        pair: pair_str.clone(),
        period: 20,
        source: PriceSource::Close,
    };
    if let Err(e) = indicator_service.register_indicator(sma_config) {
        warn!("Failed to register SMA indicator: {e}");
    }

    let ema_config = IndicatorConfig {
        id: SmolStr::new(format!("ema_12_{}", pair_str)),
        name: SmolStr::new("EMA"),
        pair: pair_str,
        period: 12,
        source: PriceSource::Close,
    };
    if let Err(e) = indicator_service.register_indicator(ema_config) {
        warn!("Failed to register EMA indicator: {e}");
    }

    // Start Indicator Service
    // NOTE: This will run but won't receive data until Market Data Engine publishes to channels
    let mut shutdown_rx_indicator = shutdown_manager.subscribe();
    let indicator_handle = tokio::spawn(async move {
        tokio::select! {
            result = indicator_service.run() => {
                if let Err(e) = result {
                    error!("Indicator service error: {e}");
                }
            }
            _ = shutdown_rx_indicator.recv() => {
                info!("Indicator service shutting down");
            }
        }
    });

    info!("Indicator Service initialized (awaiting channel integration in Proposal 04)");

    // 2. Create Trading Client
    info!("Creating Trading Client...");
    let mut binance_client = BinanceClient::new(binance_config.clone(), 1024);

    // Set up execution report channel
    let (exec_report_tx, mut exec_report_rx) = tokio::sync::mpsc::channel(100);
    binance_client
        .set_execution_report_channel(exec_report_tx)
        .await;

    // Clone client for connection check before moving it
    let binance_client_for_check = binance_client.clone();
    let trading_client: Arc<
        RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>,
    > = Arc::new(RwLock::new(binance_client.clone()));

    let mut shutdown_rx_ws = shutdown_manager.subscribe();
    let trading_ws_handle = tokio::spawn(async move {
        tokio::select! {
            result = binance_client.stream() => {
                if let Err(e) = result {
                    error!("Trading WebSocket error: {e}");
                }
            }
            _ = shutdown_rx_ws.recv() => {
                info!("Trading WebSocket shutting down");
            }
        }
    });

    // Wait for trading client to be ready (poll connection session)
    info!("Waiting for trading client connection...");
    let connection_timeout = tokio::time::Duration::from_secs(30);
    let start = std::time::Instant::now();
    let mut connected = false;

    while start.elapsed() < connection_timeout {
        // Check if BinanceClient has a connection session
        if let Some(_session) = binance_client_for_check.get_connection_session().await {
            info!("Trading client connected successfully");
            connected = true;
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    if !connected {
        return Err("Trading client failed to connect within 30 seconds".into());
    }

    // 3. Create Strategy
    info!("Creating {} Strategy...", config.strategy.strategy_type);
    let strategy = Arc::new(RwLock::new(BuyLowSellHigh::new(
        trading_client.clone(),
        symbol.clone(),
        config.strategy.position_size,
        config.strategy.max_position,
        config.strategy.buy_threshold_percent,
        config.strategy.sell_threshold_percent,
        config.strategy.stop_loss_percent,
        config.strategy.take_profit_percent,
        config.strategy.max_active_orders,
        config.strategy.price_history_size,
        config.strategy.min_price_history,
    )));

    // 4. Create Strategy Executor
    info!("Creating Strategy Executor...");
    let min_order_size = config.strategy.min_order_size.unwrap_or_else(|| {
        // Default to 0.00001 BTC if not specified
        rust_decimal::Decimal::new(1, 5)
    });
    let step_size = config.strategy.step_size.unwrap_or_else(|| {
        // Default to 0.00001 BTC if not specified (5 decimal places)
        rust_decimal::Decimal::new(1, 5)
    });
    let tick_size = config.strategy.tick_size.unwrap_or_else(|| {
        // Default to 0.01 USDC if not specified (2 decimal places for price)
        rust_decimal::Decimal::new(1, 2)
    });
    let min_notional = config.strategy.min_notional.unwrap_or_else(|| {
        // Default to 5.0 USDC if not specified
        rust_decimal::Decimal::new(5, 0)
    });
    let executor = StrategyExecutor::new(
        strategy.clone(),
        trading_client.clone(),
        config.redis.url.clone(),
        symbol.clone(),
        min_order_size,
        step_size,
        tick_size,
        min_notional,
    );

    // 5. Set up execution report handler
    let strategy_for_reports = Arc::clone(&strategy);
    let mut shutdown_rx_reports = shutdown_manager.subscribe();
    let execution_report_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                report = exec_report_rx.recv() => {
                    match report {
                        Some(report) => {
                            let order: trading_core::types::exchange::Order = report.into();
                            let mut strat = strategy_for_reports.write().await;
                            if let Err(e) = strat.on_execution_report(&order).await {
                                error!("Error handling execution report: {e}");
                            }
                        }
                        None => break,
                    }
                }
                _ = shutdown_rx_reports.recv() => {
                    info!("Execution report handler shutting down");
                    break;
                }
            }
        }
    });

    // 6. Start Health Check Server
    let health_server = HealthServer::new(strategy.clone());
    let server_config = config.server.clone();
    let mut shutdown_rx_health = shutdown_manager.subscribe();
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
    info!("Strategy: {}", config.strategy.strategy_type);
    info!("Position size: {} BTC", config.strategy.position_size);
    info!("Max position: {} BTC", config.strategy.max_position);
    info!("Symbol: {symbol}");

    // 7. Run strategy executor with shutdown handling
    let mut shutdown_rx_executor = shutdown_manager.subscribe();
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

    // Wait for shutdown signal (Ctrl+C or manual shutdown)
    shutdown_manager.wait_for_shutdown().await;

    // Graceful shutdown
    info!("Shutting down gracefully...");

    // Cancel all open orders before shutting down
    info!(
        "Cancelling all open orders for {symbol}...",
        symbol = symbol
    );
    match trading_client
        .write()
        .await
        .cancel_all_orders(&symbol)
        .await
    {
        Ok(_) => info!("All orders cancelled successfully"),
        Err(e) => warn!("Failed to cancel all orders: {error}", error = e),
    }

    // Signal all components to stop
    shutdown_manager.signal_shutdown();

    // Wait for tasks to finish (with timeout)
    let shutdown_timeout = tokio::time::Duration::from_secs(config.system.shutdown_timeout_secs);
    tokio::select! {
        _ = tokio::time::sleep(shutdown_timeout) => {
            warn!("Shutdown timeout reached, forcing exit");
        }
        _ = async {
            let _ = market_data_handle.await;
            let _ = trading_ws_handle.await;
            let _ = execution_report_handle.await;
            let _ = health_handle.await;
            let _ = executor_handle.await;
            let _ = indicator_handle.await;
        } => {
            info!("All components shut down successfully");
        }
    }

    Ok(())
}
