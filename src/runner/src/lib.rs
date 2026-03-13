pub mod config;
pub mod shutdown;

pub use gateway::grpc;

use asset_engine::AssetService;
use indicator_engine::IndicatorService;
use market_data_engine::{
    config::{ExchangeConfig as MDExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
};
use std::collections::HashSet;
use tokio::sync::Mutex;

use crate::config::{BotConfig, StrategiesConfig};
use crate::grpc::TradingGrpcServer;
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
use trading_engine::state::manager::TradingStateManager;
use trading_engine::strategies::{
    BuyLowSellHigh, StrategyExecutor, StrategyManager, StrategyManagerWrapper,
};
use trading_networking::commands::{
    Command, CommandDispatcher, CommandHandler, CommandPayload, CommandResponse, MpscCommandRouter,
};
use trading_networking::events::{cold_event_bus, hot_event_channel};
use trading_networking::gateway::backpressure::BackpressurePolicy;
use trading_networking::gateway::command_gateway::serve_zenoh_commands;
use trading_networking::gateway::event_gateway::EventGateway;
use trading_networking::gateway::transports::zenoh::{
    open_default_session, ZenohEventTransport, ZenohKeyspace,
};

struct RunnerCommandHandler {
    trading_control: Arc<trading_core::grpc::TradingControl>,
    trading_state_manager: Arc<trading_engine::state::manager::TradingStateManager>,
    exchange_state_manager: Arc<exchange_engine::state::manager::ExchangeStateManager>,
    level_manager: Arc<RwLock<trading_engine::levels::LevelManager>>,
    strategy_manager:
        Option<Arc<RwLock<trading_engine::strategies::strategy_manager::StrategyManager>>>,
}

#[async_trait::async_trait]
impl CommandHandler for RunnerCommandHandler {
    async fn handle(&self, command: Command) -> Result<CommandResponse, String> {
        match command.payload {
            CommandPayload::SetTradingEnabled { enabled } => {
                let mut active_order_ids_set = HashSet::new();
                let mut pairs_with_orders_set = HashSet::new();

                {
                    let state_arc = self.trading_state_manager.get_state();
                    let state = state_arc.read().await;
                    for order in state.active_orders.values() {
                        active_order_ids_set.insert(order.order_id.clone());
                    }
                }

                {
                    let exchange_state_arc = self.exchange_state_manager.get_state();
                    let exchange_state = exchange_state_arc.read().await;
                    for order in exchange_state.get_open_orders() {
                        active_order_ids_set.insert(order.order_id.clone());
                        if let Some(pair) = exchange_state
                            .symbol_registry()
                            .get_pair(&order.instrument.symbol().into())
                        {
                            pairs_with_orders_set.insert(pair.clone());
                        }
                    }
                }

                {
                    let level_manager = self.level_manager.read().await;
                    for level in level_manager.get_all_levels() {
                        for order_id in &level.active_orders {
                            active_order_ids_set.insert(order_id.clone());
                            pairs_with_orders_set.insert(level.pair.clone());
                        }
                    }
                }

                let active_order_ids: Vec<_> = active_order_ids_set.into_iter().collect();
                let pairs_with_orders: Vec<_> = pairs_with_orders_set.into_iter().collect();

                let cancelled = self
                    .trading_control
                    .set_enabled(enabled, active_order_ids, pairs_with_orders)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(CommandResponse {
                    success: true,
                    message: Some(if enabled {
                        "Trading enabled".to_string()
                    } else {
                        format!("Trading disabled. Cancelled {} orders.", cancelled.len())
                    }),
                    data: None,
                })
            }
            CommandPayload::GetTradingStatus => {
                let enabled = self.trading_control.is_enabled().await;
                Ok(CommandResponse {
                    success: true,
                    message: Some(format!("enabled={}", enabled)),
                    data: None,
                })
            }
            CommandPayload::ToggleStrategy {
                strategy_name,
                symbol,
                enabled,
            } => {
                let Some(ref strategy_manager) = self.strategy_manager else {
                    return Err("Strategy manager not available".to_string());
                };

                let parts: Vec<&str> = symbol.split('/').collect();
                if parts.len() != 2 {
                    return Err("Invalid symbol format; expected BASE/QUOTE".to_string());
                }
                let pair = trading_core::types::common::Pair::new(parts[0], parts[1]);
                let strategy_id = StrategyManager::make_strategy_id(&strategy_name, &pair);

                let global_trading_enabled = self.trading_control.is_enabled().await;
                if !global_trading_enabled && enabled {
                    return Err(
                        "Cannot enable strategy when global trading is disabled".to_string()
                    );
                }

                let current_enabled = strategy_manager
                    .read()
                    .await
                    .is_strategy_enabled(&strategy_id)
                    .await;
                if current_enabled != enabled {
                    strategy_manager
                        .write()
                        .await
                        .set_strategy_enabled(&strategy_id, enabled)
                        .await;
                }

                Ok(CommandResponse {
                    success: true,
                    message: Some(format!(
                        "strategy {} {}",
                        strategy_id,
                        if enabled { "enabled" } else { "disabled" }
                    )),
                    data: None,
                })
            }
            CommandPayload::GetLevels
            | CommandPayload::GetLevel { .. }
            | CommandPayload::CreateLevel { .. }
            | CommandPayload::UpdateLevel { .. }
            | CommandPayload::DeleteLevel { .. }
            | CommandPayload::ToggleLevel { .. } => Err("command not implemented".to_string()),
        }
    }
}

async fn maybe_start_zenoh_gateway(
    hot_rx: trading_networking::events::HotEventRx,
    cold_bus: trading_networking::events::ColdEventBus,
    router: Arc<MpscCommandRouter>,
) {
    let enabled = std::env::var("ZENOH_ENABLED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enabled {
        return;
    }

    let session = match open_default_session().await {
        Ok(s) => s,
        Err(e) => {
            warn!("Zenoh session open failed: {e}");
            return;
        }
    };
    let keyspace = ZenohKeyspace::default();
    let transport = Arc::new(ZenohEventTransport::new(session.clone(), keyspace.clone()));

    let mut gateway = EventGateway::new(
        BackpressurePolicy::Sample { every_nth: 1 },
        BackpressurePolicy::Drop,
    );
    let _health = gateway.add_transport("zenoh", transport, 1024);

    let cold_rx = cold_bus.subscribe();
    tokio::spawn(async move {
        gateway.run(hot_rx, cold_rx).await;
    });

    let cmd_key = keyspace.commands_key;
    tokio::spawn(async move {
        if let Err(e) = serve_zenoh_commands(&session, &cmd_key, router).await {
            tracing::warn!("Zenoh command gateway stopped: {e}");
        }
    });
}

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
                    Ok(trading_core::channels::exchange::ExchangeUpdate::Order {
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
    pub market_data_state_manager: Arc<market_data_engine::state::manager::MarketDataStateManager>,
    pub asset_service: Option<Arc<AssetService>>,
    pub trading_control: Option<Arc<trading_core::grpc::TradingControl>>,
    /// Tracing exporter handles (for /metrics endpoint when present).
    pub tracing_handles: Option<trading_trace::ExporterHandles>,
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
    let trading_control = Arc::new(TradingControl::new(services.trading_client.clone()));

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
        services.trading_state_manager.clone(),
        services.trading_client.clone(),
        services.exchange_state_manager.clone(),
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

    let (hot_tx, hot_rx) = hot_event_channel(bot_config.channels.market_data_capacity);
    let cold_bus = cold_event_bus(bot_config.channels.exchange_update_capacity);

    let (router, cmd_rx) = MpscCommandRouter::new(1024);
    let router = Arc::new(router);

    let handler = RunnerCommandHandler {
        trading_control: Arc::clone(&trading_control),
        trading_state_manager: Arc::clone(&services.trading_state_manager),
        exchange_state_manager: Arc::clone(&services.exchange_state_manager),
        level_manager: Arc::clone(&dummy_level_manager),
        strategy_manager: Some(Arc::clone(&strategy_manager_arc)),
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

    maybe_start_zenoh_gateway(hot_rx, cold_bus, router).await;

    // Pass all pairs to executor (no .first() - single source of truth)
    let executor = StrategyExecutor::new(
        strategy_wrapper.clone(),
        services.trading_client.clone(),
        services.market_data_rx.resubscribe(),
        all_symbols.clone(),
        services.trading_state_manager.clone(),
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

    maybe_start_zenoh_gateway(hot_rx, cold_bus, router).await;

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

/// Create a shutdown manager for the runner. Use this from binaries so the lib
/// constructs and uses `ShutdownManager` (avoids dead_code in the library).
pub fn create_shutdown_manager(capacity: usize) -> ShutdownManager {
    ShutdownManager::new(capacity)
}

/// Start Trading State Manager subscriber (used by `run` and by binaries that build services themselves).
pub fn start_trading_state_subscriber(
    trading_state_manager: Arc<TradingStateManager>,
    shutdown_manager: ShutdownManager,
) {
    let mut shutdown_rx = shutdown_manager.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            result = trading_state_manager.start_subscriber() => {
                if let Err(e) = result {
                    error!("TradingStateManager subscriber error: {e}");
                }
            }
            _ = shutdown_rx.recv() => {
                info!("TradingStateManager subscriber shutting down");
            }
        }
    });
}

/// Create strategy persistence if MongoDB is enabled (used by `run` and by binaries).
pub fn create_strategy_persistence(
    mongo_client: Option<&Arc<MongoClient>>,
    bot_config: &BotConfig,
) -> Option<Arc<trading_engine::strategies::persistence::StrategyPersistence>> {
    mongo_client.map(|client| {
        Arc::new(
            trading_engine::strategies::persistence::StrategyPersistence::new(
                client.as_ref().clone(),
                bot_config.mongo.bot_id.clone(),
            ),
        )
    })
}

/// Create level persistence if MongoDB is enabled (used by `run` and by binaries).
pub fn create_level_persistence(
    mongo_client: Option<&Arc<MongoClient>>,
    bot_config: &BotConfig,
) -> Option<Arc<trading_engine::levels::persistence::LevelPersistence>> {
    mongo_client.map(|client| {
        Arc::new(trading_engine::levels::persistence::LevelPersistence::new(
            client.as_ref().clone(),
            bot_config.mongo.bot_id.clone(),
        ))
    })
}

/// Main entry point: creates shutdown manager, builds services, and runs the chosen runner.
/// Call this from the runner binary so that `ShutdownManager` is constructed in the lib.
pub async fn run(
    bot_config: BotConfig,
    mode: String,
    strategies_config: String,
    levels_config: String,
    mongo_client: Arc<MongoClient>,
    tracing_handles: Option<trading_trace::ExporterHandles>,
) -> Result<(), Box<dyn std::error::Error>> {
    let shutdown_manager = ShutdownManager::new(bot_config.channels.shutdown_capacity);
    setup_shutdown_handler(shutdown_manager.clone());

    let paper_rest_base =
        match bot_config.exchange_engine.exchange {
            Exchange::Paper => serde_json::from_value::<
                exchange_engine::exchanges::k_paper::PaperConfig,
            >(bot_config.exchange_engine.config.clone())
            .ok()
            .map(|c| {
                exchange_engine::exchanges::k_paper::ws_url_to_http_base(&c.paper_exchange_url)
            }),
            _ => None,
        };
    let asset_service =
        initialize_asset_service(bot_config.exchange_engine.exchange.clone(), paper_rest_base)
            .await?;
    let configured_pairs = bot_config.get_pairs()?;
    if configured_pairs.is_empty() {
        return Err("No pairs configured in bot.json".into());
    }
    fetch_asset_data(
        &asset_service,
        bot_config.exchange_engine.exchange.clone(),
        &configured_pairs,
    )
    .await;

    // For Paper: start exchange_engine first so its WebSocket connects before market_data and
    // receives the account snapshot (k-paper may send snapshot only to the first connection).
    let (
        _,
        market_data_rx,
        market_data_handle,
        market_data_state_manager,
        exchange_state_manager,
        exchange_update_rx,
        trading_client,
    ) = if bot_config.exchange_engine.exchange == Exchange::Paper {
        let (exchange_state_manager, exchange_update_rx, trading_client) =
            initialize_exchange_engine(&bot_config, &shutdown_manager).await?;
        let (_, market_data_rx, market_data_handle, market_data_state_manager) =
            initialize_market_data_engine(&bot_config, &shutdown_manager)?;
        (
            (),
            market_data_rx,
            market_data_handle,
            market_data_state_manager,
            exchange_state_manager,
            exchange_update_rx,
            trading_client,
        )
    } else {
        let (_, market_data_rx, market_data_handle, market_data_state_manager) =
            initialize_market_data_engine(&bot_config, &shutdown_manager)?;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        info!("Market Data Engine: WebSocket subscribed to orderbook stream");

        let (exchange_state_manager, exchange_update_rx, trading_client) =
            initialize_exchange_engine(&bot_config, &shutdown_manager).await?;
        (
            (),
            market_data_rx,
            market_data_handle,
            market_data_state_manager,
            exchange_state_manager,
            exchange_update_rx,
            trading_client,
        )
    };

    if bot_config.exchange_engine.exchange != Exchange::Paper {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        info!("Market Data Engine: WebSocket subscribed to orderbook stream");
    }

    let (indicator_tx, indicator_rx) = trading_core::channels::indicators::indicator_update_channel(
        bot_config.channels.indicator_update_capacity,
    );
    let (indicator_service, indicator_handle, indicator_calculator) = initialize_indicator_service(
        &bot_config,
        &mongo_client,
        market_data_rx.resubscribe(),
        indicator_tx,
        &shutdown_manager,
        configured_pairs.as_slice(),
    )
    .await?;

    let trading_state_manager = Arc::new(TradingStateManager::new(
        exchange_update_rx.resubscribe(),
        exchange_state_manager.clone(),
    ));
    start_trading_state_subscriber(trading_state_manager.clone(), shutdown_manager.clone());

    let services = SharedServices {
        trading_client,
        exec_report_rx: exchange_update_rx.resubscribe(),
        market_data_rx: market_data_rx.resubscribe(),
        indicator_rx,
        indicator_service,
        indicator_calculator,
        shutdown_manager,
        market_data_handle,
        trading_ws_handle: tokio::spawn(async {}),
        indicator_handle,
        trading_state_manager,
        exchange_state_manager,
        market_data_state_manager,
        asset_service: Some(asset_service),
        trading_control: None,
        tracing_handles,
    };

    match mode.as_str() {
        "strategies" => {
            let strategy_persistence =
                create_strategy_persistence(Some(&mongo_client), &bot_config);
            strategy_runner(
                bot_config,
                strategies_config,
                services,
                strategy_persistence,
            )
            .await?;
        }
        "levels" => {
            let level_persistence = create_level_persistence(Some(&mongo_client), &bot_config);
            level_runner(bot_config, levels_config, services, level_persistence).await?;
        }
        _ => {
            return Err(format!("Unknown mode: {}. Use 'strategies' or 'levels'", mode).into());
        }
    }

    trading_trace::shutdown_exporters();
    Ok(())
}

/// Initialize Asset Service and register exchange client.
/// For Paper, `paper_rest_base` must be Some (HTTP base URL derived from paper_exchange_url).
pub async fn initialize_asset_service(
    exchange: Exchange,
    paper_rest_base: Option<String>,
) -> Result<Arc<AssetService>, Box<dyn std::error::Error>> {
    info!("Initializing Asset Engine...");
    let asset_service = Arc::new(AssetService::new());

    match exchange {
        Exchange::Binance => {
            let base_url = "https://api.binance.com";
            asset_service
                .register_exchange(
                    exchange,
                    Arc::new(asset_engine::exchanges::BinancePublicClient::new(
                        base_url.to_string(),
                    )),
                )
                .await
                .map_err(|e| format!("Failed to register Binance client: {e}"))?;
        }
        Exchange::Bybit => {
            let base_url = "https://api.bybit.com";
            asset_service
                .register_exchange(
                    exchange,
                    Arc::new(asset_engine::exchanges::BybitPublicClient::new(
                        base_url.to_string(),
                    )),
                )
                .await
                .map_err(|e| format!("Failed to register Bybit client: {e}"))?;
        }
        Exchange::Paper => {
            let base_url = paper_rest_base.ok_or(
                "Paper exchange requires paper_rest_base (HTTP base URL) for asset engine",
            )?;
            asset_service
                .register_exchange(
                    exchange,
                    Arc::new(asset_engine::exchanges::KPaperPublicClient::new(base_url)),
                )
                .await
                .map_err(|e| format!("Failed to register k-paper asset client: {e}"))?;
        }
        Exchange::Unsupported => {
            return Err("Unsupported exchange for Asset Engine".into());
        }
    }

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

type MarketDataEngineResult = (
    trading_core::channels::market_data::MarketDataUpdateSender,
    trading_core::channels::market_data::MarketDataUpdateReceiver,
    JoinHandle<()>,
    Arc<market_data_engine::state::manager::MarketDataStateManager>,
);

/// Initialize Market Data Engine
pub fn initialize_market_data_engine(
    bot_config: &BotConfig,
    shutdown_manager: &ShutdownManager,
) -> Result<MarketDataEngineResult, Box<dyn std::error::Error>> {
    use market_data_engine::exchanges::{
        binance::config::BinanceConfig as MDBinanceConfig,
        bybit::{config::BybitCategory, config::BybitConfig as MDBybitConfig},
    };

    info!(
        "Starting Market Data Engine (exchange: {:?})...",
        bot_config.exchange_engine.exchange
    );

    // Get pairs from exchange config
    let pairs = bot_config.get_pairs()?;
    if pairs.is_empty() {
        return Err("No pairs configured in bot.json".into());
    }

    // Extract exchange-specific config
    // Pairs come from bot.json - single source of truth
    let market_data_config = match bot_config.exchange_engine.exchange {
        Exchange::Binance => {
            let binance_config: exchange_engine::exchanges::binance::config::BinanceConfig =
                serde_json::from_value(bot_config.exchange_engine.config.clone())
                    .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;

            MarketDataConfig {
                redis_url: None,
                exchange: bot_config.exchange_engine.exchange.clone(),
                exchange_config: MDExchangeConfig::Binance(MDBinanceConfig {
                    pairs: pairs.clone(), // Single source of truth from bot.json
                    limit: binance_config.limit.unwrap_or(100),
                    base_delay: binance_config.base_delay,
                    max_delay: binance_config.max_delay,
                }),
            }
        }
        Exchange::Bybit => {
            let _bybit_config: exchange_engine::exchanges::bybit::config::BybitConfig =
                serde_json::from_value(bot_config.exchange_engine.config.clone())
                    .map_err(|e| format!("Failed to deserialize Bybit config: {e}"))?;

            // Extract market data specific fields from raw JSON config
            // These are not part of exchange engine config, but needed for market data engine
            let config_json = &bot_config.exchange_engine.config;
            let category = config_json
                .get("category")
                .and_then(|v| v.as_str())
                .map(|s| match s {
                    "linear" => BybitCategory::Linear,
                    "inverse" => BybitCategory::Inverse,
                    _ => BybitCategory::Spot,
                })
                .unwrap_or(BybitCategory::Spot);

            MarketDataConfig {
                redis_url: None,
                exchange: bot_config.exchange_engine.exchange.clone(),
                exchange_config: MDExchangeConfig::Bybit(MDBybitConfig {
                    pairs: pairs.clone(), // Single source of truth from bot.json
                    limit: config_json
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(50), // Default to 50 (Bybit supports 1, 50, or 200)
                    base_delay: config_json.get("baseDelay").and_then(|v| v.as_u64()),
                    max_delay: config_json.get("maxDelay").and_then(|v| v.as_u64()),
                    category,
                }),
            }
        }
        Exchange::Paper => {
            let paper_config: exchange_engine::exchanges::k_paper::PaperConfig =
                serde_json::from_value(bot_config.exchange_engine.config.clone())
                    .map_err(|e| format!("Failed to deserialize Paper config: {e}"))?;
            MarketDataConfig {
                redis_url: None,
                exchange: bot_config.exchange_engine.exchange.clone(),
                exchange_config: MDExchangeConfig::Paper(
                    market_data_engine::exchanges::k_paper::config::PaperConfig {
                        url: paper_config.paper_exchange_url.clone(),
                        pairs: pairs.clone(),
                    },
                ),
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

    // Get state manager before moving driver into task
    let state_manager = market_data_driver.get_state_manager();

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

    Ok((market_data_tx, market_data_rx, handle, state_manager))
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
        bot_config.exchange_engine.exchange,
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
    let (exchange_engine_config, pairs) = match bot_config.exchange_engine.exchange {
        Exchange::Binance => {
            let binance_config: exchange_engine::exchanges::binance::config::BinanceConfig =
                serde_json::from_value(bot_config.exchange_engine.config.clone())
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
        Exchange::Bybit => {
            let bybit_config: exchange_engine::exchanges::bybit::config::BybitConfig =
                serde_json::from_value(bot_config.exchange_engine.config.clone())
                    .map_err(|e| format!("Failed to deserialize Bybit config: {e}"))?;

            // Validate pairs
            if bybit_config.pairs.is_empty() {
                return Err("No pairs configured in bybit config".into());
            }

            let pairs = bybit_config.pairs.clone();

            let exchange_config = ExchangeEngineConfig {
                exchange: Exchange::Bybit,
                exchange_config: exchange_engine::config::ExchangeConfig::Bybit(bybit_config),
            };

            (exchange_config, pairs)
        }
        Exchange::Paper => {
            let paper_config: exchange_engine::exchanges::k_paper::PaperConfig =
                serde_json::from_value(bot_config.exchange_engine.config.clone())
                    .map_err(|e| format!("Failed to deserialize Paper config: {e}"))?;

            if paper_config.pairs.is_empty() {
                return Err("No pairs configured in paper config".into());
            }

            let pairs = paper_config.pairs.clone();
            let exchange_config = ExchangeEngineConfig {
                exchange: Exchange::Paper,
                exchange_config: exchange_engine::config::ExchangeConfig::Paper(paper_config),
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
