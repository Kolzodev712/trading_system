pub mod config;
pub mod shutdown;

use crate::config::{BotConfig, StrategiesConfig};
use crate::shutdown::ShutdownManager;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use trading_core::channels::indicators::indicator_update_channel;
use trading_core::traits::TradingClient;
use trading_core::types::exchange::OrderRequest;
use trading_engine::health::HealthServer as TradingHealthServer;
use trading_engine::levels::{
    CancellationManager, IndicatorSubscriber, LevelBasedEventLoop, LevelExecutor, LevelManager,
    RiskManager,
};
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
    pub shutdown_manager: ShutdownManager,
    pub market_data_handle: JoinHandle<()>,
    pub trading_ws_handle: JoinHandle<()>,
    pub indicator_handle: JoinHandle<()>,
    pub trading_state_manager: Arc<trading_engine::state::manager::TradingStateManager>,
}

/// Run strategies mode
pub async fn strategy_runner(
    bot_config: BotConfig,
    strategies_config_path: String,
    services: SharedServices,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Running in STRATEGIES mode ===");

    let strategies_config = StrategiesConfig::load_from_file(&strategies_config_path)?;
    strategies_config.validate()?;

    info!("Creating {} Strategy...", strategies_config.strategy_type);
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
    info!("Strategy: {}", strategies_config.strategy_type);
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
            let _ = services.trading_ws_handle.await;
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
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Running in LEVELS mode ===");

    // Load levels configuration
    info!(
        "Loading levels configuration from {}...",
        levels_config_path
    );
    let mut level_manager = LevelManager::new();
    let risk_limits = level_manager
        .load_from_config_with_risk_limits(Path::new(&levels_config_path))
        .await
        .map_err(|e| format!("Failed to load levels config: {e}"))?;

    info!("Loaded {} levels", level_manager.get_all_levels().len());

    // Create level executor
    let level_executor = Arc::new(LevelExecutor::new(services.trading_client.clone()));

    // Create risk manager
    let risk_manager = Arc::new(RiskManager::new(risk_limits));

    // Create cancellation manager (empty conditions for now - can be configured later)
    let cancellation_manager = Arc::new(CancellationManager::new(Vec::new()));

    // Create indicator subscriber
    // NOTE: For now, create a new channel. In the future, this should come from SharedServices
    let (_indicator_tx, indicator_rx) = indicator_update_channel(1000);
    let indicator_subscriber = Arc::new(RwLock::new(IndicatorSubscriber::new(indicator_rx)));

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

    // Extract symbol from first level (assuming all levels use same symbol for now)
    let symbol = level_manager
        .get_all_levels()
        .first()
        .map(|l| l.config.symbol.clone())
        .ok_or("No levels configured")?;

    // Create level-based event loop
    let trading_reconciliation_interval_secs =
        bot_config.reconciliation.trading_level_interval_ms / 1000;
    let event_loop = LevelBasedEventLoop::new(
        Arc::new(RwLock::new(level_manager)),
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

    info!("All systems ready! Starting level-based trading...");
    info!("Symbol: {}", symbol);

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
        } => {
            info!("All components shut down successfully");
        }
    }

    Ok(())
}
