use core_logic::{
    config::TradingSystemConfig,
    data::base_data::Pair,
    retry::order_retry_config,
    traits::TradingClient,
    Exchange,
};
use exchange_engine::exchanges::binance::{
    config::BinanceConfig,
    ws_client::BinanceClient,
};
use market_data_engine::{
    config::{ExchangeConfig as MDExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
};
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use trading_engine::{
    executor::StrategyExecutor,
    strategies::BuyLowSellHigh,
    strategy::Strategy,
};
use runner::{health::HealthServer, shutdown::ShutdownManager};

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
    tracing_subscriber::fmt().with_max_level(log_level).init();

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
    let market_data_config = MarketDataConfig {
        redis_url: config.redis.url.clone(),
        exchange: config.exchange.exchange,
        exchange_config: MDExchangeConfig::Binance(binance_config.clone()),
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

    // Wait for market data to start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 2. Create Trading Client
    info!("Creating Trading Client...");
    let binance_client = BinanceClient::new(binance_config.clone(), 1024);

    // Set up execution report channel
    let (exec_report_tx, mut exec_report_rx) = tokio::sync::mpsc::channel(100);
    binance_client.set_execution_report_channel(exec_report_tx).await;

    let trading_client: Arc<
        RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>,
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

    // Wait for trading client to be ready
    let mut attempts = 0;
    loop {
        if trading_client
            .read()
            .await
            .get_connection_session()
            .await
            .is_some()
        {
            info!("Trading client ready");
            break;
        }
        attempts += 1;
        if attempts > 40 {
            return Err("Trading client failed to connect".into());
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
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
    let executor = StrategyExecutor::new(
        strategy.clone(),
        trading_client.clone(),
        config.redis.url.clone(),
        symbol.clone(),
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
                            let order: core_logic::data::private_data::Order = report.into();
                            let mut strat = strategy_for_reports.write().await;
                            if let Err(e) = strat
                                .on_execution_report(&order, strat.get_state_mut())
                                .await
                            {
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

    // Wait for shutdown signal
    shutdown_manager.wait_for_shutdown(config.system.shutdown_timeout_secs).await;

    // Graceful shutdown
    info!("Shutting down gracefully...");
    
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
        } => {
            info!("All components shut down successfully");
        }
    }

    Ok(())
}
