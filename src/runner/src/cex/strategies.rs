//! Strategy-based trading mode runner.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::cex::operator::FileStrategyOperator;
use crate::config::runtime_snapshot::{RuntimeSnapshot, SNAPSHOT_FILE};
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
use trading_engine::risk::RiskManager;
use trading_engine::state::Strategy;
use trading_engine::strategies::{
    build_strategy_composed, ExtraStrategyFactory, StrategyExecutor, StrategyFactoryArgs,
    StrategyManager, StrategyManagerWrapper,
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

async fn flatten_open_positions(
    trading_client: Arc<
        RwLock<
            dyn trading_core::venue::cex::traits::TradingClient<
                trading_core::types::exchange::OrderRequest,
            >,
        >,
    >,
) -> Result<(), Box<dyn std::error::Error>> {
    let positions = trading_client.read().await.fetch_positions().await?;
    let mut closed = 0usize;
    for position in positions {
        let qty = position.quantity.abs();
        if qty <= rust_decimal::Decimal::new(1, 6) {
            continue;
        }
        let side = if position.quantity > rust_decimal::Decimal::ZERO {
            trading_core::types::common::Side::Sell
        } else {
            trading_core::types::common::Side::Buy
        };
        let mut exchange_params = std::collections::HashMap::new();
        exchange_params.insert(
            smol_str::SmolStr::new("reduceOnly"),
            smol_str::SmolStr::new("true"),
        );
        info!("Flattening {} {:?} qty={}", position.pair(), side, qty);
        let request = trading_core::types::exchange::OrderRequest {
            instrument: position.instrument.clone(),
            side,
            order_type: trading_core::types::common::OrderType::Market,
            price: rust_decimal::Decimal::ZERO,
            amount: qty,
            timestamp: None,
            exchange_params,
        };
        let order = trading_client.write().await.open_order(request).await?;
        info!("Flatten order placed: {:?}", order.order_id);
        closed += 1;
    }
    if closed == 0 {
        info!("Flatten: no open futures positions");
    }
    Ok(())
}

async fn append_shadow_decisions(
    journal: &trading_engine::cex::shadow::ShadowJournal,
    manager: &StrategyManager,
    strategy_infos: &[(String, trading_core::types::common::Pair, usize)],
    execution_mode: trading_core::types::ExecutionMode,
    trading_state_manager: Option<&Arc<trading_engine::state::manager::TradingStateManager>>,
    would_have_requested_orders: usize,
) {
    let now_ms = trading_engine::cex::shadow::now_ms();
    let account = trading_state_manager.map(|tsm| tsm.get_state());
    for (name, symbol, _) in strategy_infos {
        let strategy_id = StrategyManager::make_strategy_id(name, symbol);
        let Some(snapshot) = manager.runtime_snapshot(&strategy_id, now_ms).await else {
            continue;
        };
        let (filled, working, reserved, effective) = if let Some(state) = &account {
            let state = state.read().await;
            (
                state.position_qty(symbol),
                state.working_qty(symbol),
                state.reserved_submission_qty(symbol),
                state.effective_position_qty(symbol),
            )
        } else {
            (
                rust_decimal::Decimal::ZERO,
                rust_decimal::Decimal::ZERO,
                rust_decimal::Decimal::ZERO,
                rust_decimal::Decimal::ZERO,
            )
        };
        let record = trading_engine::cex::shadow::record_from_snapshot(
            &snapshot,
            execution_mode,
            filled,
            working,
            reserved,
            effective,
            would_have_requested_orders,
        );
        if let Err(err) = journal.append(&record) {
            warn!("Shadow journal append failed: {err}");
        }
    }
}

/// Run strategies mode
pub async fn strategy_runner(
    bot_config: BotConfig,
    strategies_config_path: String,
    services: SharedServices,
) -> Result<(), Box<dyn std::error::Error>> {
    strategy_runner_with(bot_config, strategies_config_path, services, None).await
}

/// Run strategies mode with an optional in-process extra factory.
pub async fn strategy_runner_with(
    bot_config: BotConfig,
    strategies_config_path: String,
    services: SharedServices,
    extra_factory: Option<Arc<dyn ExtraStrategyFactory>>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Running in STRATEGIES mode ===");

    let strategies_config = StrategiesConfig::load_from_file(&strategies_config_path)?;
    strategies_config.validate()?;
    let risk_limits = strategies_config.risk_limits();
    risk_limits.require_explicit_for_exchange(bot_config.execution_mode)?;

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
    let execution_mode = bot_config.execution_mode;
    let instrument_type = bot_config
        .get_instrument_type()
        .unwrap_or(trading_core::types::common::InstrumentType::Spot);
    if execution_mode == trading_core::types::ExecutionMode::Exchange
        && matches!(
            instrument_type,
            trading_core::types::common::InstrumentType::Futures
                | trading_core::types::common::InstrumentType::Derivitives
        )
    {
        for config in &strategy_configs {
            if config.max_market_data_age_ms.is_none() {
                return Err(
                    "max_market_data_age_ms is required for Exchange futures; no default is invented"
                        .into(),
                );
            }
        }
    }
    let shadow_forbidden_calls = Arc::new(AtomicU64::new(0));
    let trading_client = {
        let raw = services.trading_client()?.clone();
        if execution_mode == trading_core::types::ExecutionMode::Shadow {
            trading_engine::cex::shadow::ShadowGuardClient::wrap_counted(
                raw,
                shadow_forbidden_calls.clone(),
            )
        } else {
            raw
        }
    };

    let mut strategy_manager = StrategyManager::new();
    let mut all_symbols = Vec::new();
    let mut strategy_infos = Vec::new(); // For gRPC monitoring
    let mut strategy_caps = std::collections::HashMap::new();

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

        let strategy = build_strategy_composed(
            StrategyFactoryArgs {
                strategy_type: config.strategy_type.to_string(),
                symbol: pair.clone(),
                base_position_size: config.position_size,
                max_position: config.max_position,
                base_buy_threshold: config.buy_threshold_percent,
                base_sell_threshold: config.sell_threshold_percent,
                stop_loss_percent: config.stop_loss_percent,
                take_profit_percent: config.take_profit_percent,
                max_active_orders: config.max_active_orders,
                price_history_size: config.price_history_size,
                min_price_history: config.min_price_history,
                asset_service: services.asset_service.clone(),
                exchange: bot_config.exchange_engine.exchange.clone(),
                config_min_order_size: min_order_size,
                config_step_size: step_size,
                config_tick_size: tick_size,
                config_min_notional: min_notional,
                execution_mode,
                instrument_type: instrument_type.clone(),
                max_entry_age_secs: config.max_entry_age_secs,
                lookback: config.lookback,
            },
            extra_factory.as_deref(),
        )?;

        if matches!(config.strategy_type.as_str(), "Donchian" | "donchian") {
            match crate::cex::fetch_bybit_klines(pair, "D", 60).await {
                Ok(candles) => {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    let closed = crate::cex::completed_candles(candles, now_ms, 86_400_000);
                    let bars: Vec<_> = closed
                        .iter()
                        .map(|c| (c.timestamp_ms, c.high, c.low, c.close))
                        .collect();
                    Strategy::seed_daily_bars(&mut *strategy.write().await, &bars).await?;
                    info!(
                        "Donchian warmup: {} closed daily bars for {}",
                        bars.len(),
                        pair
                    );
                }
                Err(e) => {
                    warn!("Donchian daily warmup failed for {pair}: {e}; will fill from live books")
                }
            }
        }

        // Create strategy ID: "strategy_name:symbol"
        let strategy_id =
            trading_engine::strategies::strategy_manager::StrategyManager::make_strategy_id(
                &config.strategy_type.to_string(),
                pair,
            );
        strategy_manager
            .add_strategy(strategy, strategy_id.clone(), pair.clone())
            .await;
        strategy_caps.insert(strategy_id, config.max_position);
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

    let start_paused =
        bot_config.exchange_engine.exchange != trading_core::types::common::Exchange::Paper;
    let trading_control =
        Arc::new(TradingControl::new(trading_client.clone()).with_entries_paused(start_paused));
    if start_paused {
        info!("entries start paused until authenticated Resume (non-Paper venue)");
    }

    // Wrap StrategyManager and set trading control for hierarchical control
    let strategy_manager = strategy_manager.with_instrument_type(instrument_type.clone());
    let strategy_manager_arc = Arc::new(RwLock::new(strategy_manager));
    let strategy_wrapper = Arc::new(RwLock::new(StrategyManagerWrapper::new(
        strategy_manager_arc.clone(),
    )));
    let strategy_caps = Arc::new(RwLock::new(strategy_caps));
    let operator = Arc::new(FileStrategyOperator::new(
        strategies_config_path.clone(),
        strategy_caps.clone(),
        strategy_manager_arc.clone(),
        trading_control.clone(),
        services.shutdown_manager.clone(),
    ));

    let shadow_journal = if execution_mode == trading_core::types::ExecutionMode::Shadow {
        Some(trading_engine::cex::shadow::ShadowJournal::new(
            bot_config.shadow_journal_path.clone(),
        ))
    } else {
        None
    };
    {
        let mut daily_source = crate::cex::DailyCompletedCandleSource::new(
            bot_config.exchange_engine.exchange.clone(),
            instrument_type.clone(),
        );
        let manager = strategy_manager_arc.read().await;
        daily_source.poll_pairs(&all_symbols, &manager).await;
        manager.finish_model_reconstruction().await;
        if let Some(journal) = &shadow_journal {
            append_shadow_decisions(
                journal,
                &manager,
                &strategy_infos,
                execution_mode,
                services.trading_state_manager.as_ref(),
                0,
            )
            .await;
        }
    }

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

    // Create gRPC service with strategy manager for individual toggling
    use crate::grpc::service::TradingCliServiceImpl;
    let grpc_service = TradingCliServiceImpl::new(
        dummy_level_manager.clone(),
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
    .with_strategy_infos(monitoring_strategy_infos)
    .with_strategy_manager(strategy_manager_arc.clone())
    .with_instrument_type(instrument_type.clone())
    .with_exchange(format!("{:?}", bot_config.exchange_engine.exchange));
    let (_, venue, environment) = crate::config::execution_status_from_bot(&bot_config);
    let grpc_service = grpc_service
        .with_execution_status(execution_mode, venue, environment)
        .with_strategy_operator(operator)
        .with_strategy_caps(strategy_caps.clone());

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

    if matches!(
        instrument_type,
        trading_core::types::common::InstrumentType::Futures
            | trading_core::types::common::InstrumentType::Derivitives
    ) {
        let tsm = services
            .trading_state_manager
            .as_ref()
            .ok_or("Trading state requires venue.mode \"cex\"")?;
        let trading_state = tsm.get_state();
        {
            let mut state = trading_state.write().await;
            for pair in &all_symbols {
                state.mark_position_sync_pending(pair.clone());
            }
        }
        let positions = trading_client.read().await.fetch_positions().await?;
        {
            let mut state = trading_state.write().await;
            for position in positions {
                state.set_position(
                    position.instrument.pair.clone(),
                    position.quantity,
                    position.avg_price,
                );
            }
            for pair in &all_symbols {
                state.mark_position_synced(pair);
            }
        }
    }

    // Pass all pairs to executor (no .first() - single source of truth)
    let executor = StrategyExecutor::new(
        strategy_wrapper.clone(),
        trading_client.clone(),
        services.market_data_rx.resubscribe(),
        all_symbols.clone(),
        services
            .trading_state_manager
            .clone()
            .ok_or("Trading state requires venue.mode \"cex\"")?,
        trading_reconciliation_interval_secs,
        services.asset_service.clone(),
        bot_config.exchange_engine.exchange.clone(),
        instrument_type.clone(),
        min_order_size,
        step_size,
        tick_size,
        min_notional,
    )
    .with_trading_control(trading_control.enabled_flag())
    .with_operator_flags(
        trading_control.entries_paused_flag(),
        trading_control.flatten_latched_flag(),
    )
    .with_execution_mode(execution_mode)
    .with_strategy_manager(strategy_manager_arc.clone())
    .with_shared_strategy_caps(strategy_caps)
    .with_risk_manager(Arc::new(
        RiskManager::new(
            risk_limits,
            Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
                all_symbols.clone(),
            )),
        )
        .with_execution_ledger(
            services
                .trading_state_manager
                .as_ref()
                .and_then(|tsm| tsm.execution_ledger())
                .ok_or("execution ledger is required")?,
        ),
    ))
    .with_max_active_orders(first_config.max_active_orders)
    .with_leverage(
        bot_config
            .exchange_engine
            .config
            .get("leverage")
            .and_then(|value| value.as_u64())
            .map(|value| value as u32),
    )
    .with_max_market_data_age_ms(first_config.max_market_data_age_ms)
    .with_submission_journal(std::sync::Arc::new(
        trading_engine::state::SubmissionJournal::open(&bot_config.submission_journal_path)
            .map_err(|err| format!("submission journal: {err}"))?,
    ))
    .with_exchange_state_manager(services.exchange_state_manager()?.clone());

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

    let refresh_manager = strategy_manager_arc.clone();
    let refresh_pairs = all_symbols.clone();
    let mut refresh_shutdown = services.shutdown_manager.subscribe();
    let mut daily_source = crate::cex::DailyCompletedCandleSource::new(
        bot_config.exchange_engine.exchange.clone(),
        instrument_type.clone(),
    );
    let refresh_journal = shadow_journal.clone();
    let refresh_infos = strategy_infos.clone();
    let refresh_execution_mode = execution_mode;
    let refresh_tsm = services.trading_state_manager.clone();
    let refresh_donchian = strategy_configs
        .iter()
        .any(|config| matches!(config.strategy_type.as_str(), "Donchian" | "donchian"));
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    {
                        let manager = refresh_manager.read().await;
                        let accepted = daily_source.poll_pairs(&refresh_pairs, &manager).await;
                        if accepted > 0 {
                            if let Some(journal) = &refresh_journal {
                                append_shadow_decisions(
                                    journal,
                                    &manager,
                                    &refresh_infos,
                                    refresh_execution_mode,
                                    refresh_tsm.as_ref(),
                                    0,
                                )
                                .await;
                            }
                        }
                    }
                    if !refresh_donchian {
                        continue;
                    }
                    for pair in &refresh_pairs {
                        match crate::cex::fetch_bybit_klines(pair, "D", 60).await {
                            Ok(candles) => {
                                let closed = crate::cex::completed_candles(candles, now_ms, 86_400_000);
                                let bars: Vec<_> = closed
                                    .iter()
                                    .map(|c| (c.timestamp_ms, c.high, c.low, c.close))
                                    .collect();
                                if let Err(e) = refresh_manager
                                    .read()
                                    .await
                                    .seed_daily_bars(pair, &bars)
                                    .await
                                {
                                    warn!("Daily candle refresh failed for {pair}: {e}");
                                }
                            }
                            Err(e) => warn!("Daily kline refresh failed for {pair}: {e}"),
                        }
                    }
                }
                _ = refresh_shutdown.recv() => break,
            }
        }
    });

    if let Some(tsm) = services.trading_state_manager.clone() {
        for symbol in &all_symbols {
            if let Err(e) = tsm.reconcile(symbol, None).await {
                warn!("Startup order import failed for {symbol}: {e}");
            }
        }
    }

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

    if std::env::var("TRADING_FLATTEN").ok().as_deref() == Some("1") {
        if execution_mode == trading_core::types::ExecutionMode::Shadow {
            warn!("TRADING_FLATTEN ignored in Shadow (no exchange order calls)");
        } else {
            info!("TRADING_FLATTEN=1: closing open futures positions (reduce-only)");
            flatten_open_positions(trading_client.clone()).await?;
        }
    }

    let snap = load_and_apply_strategy_enablement(
        &*strategy_manager_arc.read().await,
        &strategy_configs,
        SNAPSHOT_FILE,
    )
    .await?;
    if let Some(snap) = snap.filter(|s| s.mode == "strategies") {
        if snap.global_trading {
            *trading_control.enabled_flag().write().await = true;
        }
        info!(
            "Restored runtime snapshot ({} strategies, trading={})",
            snap.strategies.len(),
            snap.global_trading
        );
    }

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
    let on_stop = RuntimeSnapshot::load(crate::config::runtime_snapshot::SNAPSHOT_FILE)
        .ok()
        .flatten()
        .map(|snap| snap.on_stop)
        .unwrap_or_else(|| "cancel_working".into());
    match on_stop.as_str() {
        "keep_working" => {
            info!("shutdown policy keep_working: leaving working orders");
        }
        "flatten" => {
            info!("shutdown policy flatten: flatten orders already submitted; skip cancel_all");
        }
        _ => {
            cancel_working_orders_on_stop(execution_mode, &trading_client, &all_symbols).await;
        }
    }
    if execution_mode == trading_core::types::ExecutionMode::Shadow {
        info!(
            "Shadow guard forbidden mutation attempts: {}",
            shadow_forbidden_calls.load(Ordering::Relaxed)
        );
    }

    services.shutdown_manager.signal_shutdown();
    // Long-lived Monitor RPCs keep tonic `serve_with_shutdown` from returning.
    // Abort the server task so a connected UI cannot pin the process for
    // `shutdown_timeout_secs` (the previous 30s forced-exit after RequestRestart).
    grpc_handle.abort();

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

/// Snapshot `enabled` overrides `initially_enabled` for strategies listed in the snapshot.
/// Strategies absent from the snapshot keep the config flag (cold start).
pub(crate) async fn apply_strategy_enablement(
    manager: &StrategyManager,
    strategy_configs: &[crate::config::StrategyConfig],
    snapshot: Option<&RuntimeSnapshot>,
) {
    for config in strategy_configs {
        let strategy_id =
            StrategyManager::make_strategy_id(config.strategy_type.as_str(), &config.symbol);
        manager
            .set_strategy_enabled(&strategy_id, config.initially_enabled)
            .await;
    }
    let Some(snap) = snapshot.filter(|s| s.mode == "strategies") else {
        return;
    };
    for id in manager.strategy_ids().to_vec() {
        let Some(enabled) = snap
            .strategies
            .iter()
            .find(|s| s.strategy_id == id)
            .or_else(|| {
                snap.strategies
                    .iter()
                    .find(|s| symbol_key(&s.strategy_id) == symbol_key(&id))
            })
            .map(|s| s.enabled)
        else {
            continue;
        };
        manager.set_strategy_enabled(&id, enabled).await;
    }
}

/// Production startup path: load an optional snapshot, then always apply enablement.
/// A missing file is a cold start (`initially_enabled` wins). A strategies snapshot
/// overlays saved `enabled` afterward. Global trading restore stays with the caller.
pub(crate) async fn load_and_apply_strategy_enablement(
    manager: &StrategyManager,
    strategy_configs: &[crate::config::StrategyConfig],
    snapshot_path: impl AsRef<Path>,
) -> Result<Option<RuntimeSnapshot>, String> {
    let snap = RuntimeSnapshot::load(snapshot_path)?;
    apply_strategy_enablement(manager, strategy_configs, snap.as_ref()).await;
    Ok(snap)
}

/// Shadow never issues order mutations on stop, including via the unwrapped venue client.
pub(crate) async fn cancel_working_orders_on_stop(
    execution_mode: trading_core::types::ExecutionMode,
    trading_client: &Arc<
        RwLock<
            dyn trading_core::venue::cex::traits::TradingClient<
                trading_core::types::exchange::OrderRequest,
            >,
        >,
    >,
    symbols: &[trading_core::types::common::Pair],
) {
    if execution_mode == trading_core::types::ExecutionMode::Shadow {
        info!("Shadow mode: skipping cancel_all_orders on shutdown");
        return;
    }
    for symbol in symbols {
        info!("Cancelling all open orders for {symbol}...");
        match trading_client.write().await.cancel_all_orders(symbol).await {
            Ok(_) => info!("All orders cancelled successfully for {symbol}"),
            Err(e) => warn!(
                "Failed to cancel all orders for {symbol}: {error}",
                error = e
            ),
        }
    }
}

fn symbol_key(strategy_id: &str) -> &str {
    strategy_id
        .rsplit_once(':')
        .map(|(_, s)| s)
        .unwrap_or(strategy_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::runtime_snapshot::SnapshotStrategy;
    use crate::config::StrategyConfig;
    use crate::platform::shutdown::ShutdownManager;
    use gateway::grpc::service::StrategyOperator;
    use rust_decimal::Decimal;
    use std::sync::{Arc, Mutex};
    use tokio::sync::RwLock;
    use trading_core::error::EngineError;
    use trading_core::grpc::TradingControl;
    use trading_core::types::common::{Exchange, OrderId, Pair};
    use trading_core::types::exchange::{
        BulkAmendRequest, BulkAmendResponse, BulkCancelRequest, BulkCancelResponse,
        BulkOrderRequest, BulkOrderResponse, Order, OrderRequest, Trade,
    };
    use trading_core::types::ExecutionMode;
    use trading_core::venue::cex::traits::TradingClient;
    use trading_engine::cex::intent::{StrategyContext, StrategyIntent};
    use trading_engine::cex::shadow::ShadowGuardClient;
    use trading_engine::state::Strategy;

    #[derive(Debug, Default)]
    struct RecordingClient {
        mutations: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingClient {
        fn record(&self, method: &str) {
            self.mutations.lock().unwrap().push(method.to_string());
        }
    }

    #[async_trait::async_trait]
    impl TradingClient<OrderRequest> for RecordingClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }

        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            self.record("test_open_order");
            Ok(())
        }

        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            self.record("open_order");
            Err(EngineError::new_custom_error("recording".into()))
        }

        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            self.record("cancel_order_by_id");
            Ok(())
        }

        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            self.record("cancel_all_orders");
            Ok(())
        }

        async fn fetch_balance(
            &self,
        ) -> Result<Vec<trading_core::venue::cex::types::exchange::Balance>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_trades(&self, _pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }

        async fn amend_order(
            &self,
            _order_id: OrderId,
            _new_request: OrderRequest,
        ) -> Result<Order, EngineError> {
            self.record("amend_order");
            Err(EngineError::new_custom_error("recording".into()))
        }

        async fn bulk_open_orders(
            &self,
            _req: BulkOrderRequest,
        ) -> Result<BulkOrderResponse, EngineError> {
            self.record("bulk_open_orders");
            Err(EngineError::new_custom_error("recording".into()))
        }

        async fn bulk_amend_orders(
            &self,
            _req: BulkAmendRequest,
        ) -> Result<BulkAmendResponse, EngineError> {
            self.record("bulk_amend_orders");
            Err(EngineError::new_custom_error("recording".into()))
        }

        async fn bulk_cancel_orders(
            &self,
            _req: BulkCancelRequest,
        ) -> Result<BulkCancelResponse, EngineError> {
            self.record("bulk_cancel_orders");
            Err(EngineError::new_custom_error("recording".into()))
        }
    }

    struct NoopStrategy;

    #[async_trait::async_trait]
    impl Strategy for NoopStrategy {
        async fn evaluate(
            &mut self,
            _ctx: &StrategyContext<'_>,
        ) -> Result<StrategyIntent, EngineError> {
            Ok(StrategyIntent::hold("noop"))
        }

        async fn on_execution_report(&mut self, _order: &Order) -> Result<(), EngineError> {
            Ok(())
        }
    }

    fn sample_config(initially_enabled: bool) -> StrategyConfig {
        StrategyConfig {
            strategy_type: "BTCPriceMALongFlatV1".into(),
            symbol: Pair::new("BTC", "USDT"),
            position_size: Decimal::ONE,
            max_position: Decimal::ONE,
            buy_threshold_percent: Decimal::ZERO,
            sell_threshold_percent: Decimal::ZERO,
            stop_loss_percent: None,
            take_profit_percent: None,
            max_active_orders: 1,
            price_history_size: 1,
            min_price_history: 1,
            min_order_size: None,
            step_size: None,
            tick_size: None,
            min_notional: None,
            lookback: None,
            max_entry_age_secs: None,
            initially_enabled,
            max_market_data_age_ms: None,
        }
    }

    const C2_ID: &str = "BTCPriceMALongFlatV1:BTC/USDT";

    async fn manager_with_c2() -> StrategyManager {
        let mut manager = StrategyManager::new();
        manager
            .add_strategy(
                Arc::new(RwLock::new(NoopStrategy)),
                C2_ID.into(),
                Pair::new("BTC", "USDT"),
            )
            .await;
        manager
    }

    #[tokio::test]
    async fn shadow_ordinary_shutdown_records_zero_inner_mutations() {
        let inner = RecordingClient::default();
        let mutations = inner.mutations.clone();
        let client: Arc<RwLock<dyn TradingClient<OrderRequest>>> = Arc::new(RwLock::new(inner));
        cancel_working_orders_on_stop(ExecutionMode::Shadow, &client, &[Pair::new("BTC", "USDT")])
            .await;
        assert!(
            mutations.lock().unwrap().is_empty(),
            "Shadow shutdown must not reach the backing client: {:?}",
            mutations.lock().unwrap()
        );
    }

    #[tokio::test]
    async fn exchange_shutdown_records_cancel_all_on_backing_client() {
        let inner = RecordingClient::default();
        let mutations = inner.mutations.clone();
        let client: Arc<RwLock<dyn TradingClient<OrderRequest>>> = Arc::new(RwLock::new(inner));
        cancel_working_orders_on_stop(
            ExecutionMode::Exchange,
            &client,
            &[Pair::new("BTC", "USDT")],
        )
        .await;
        assert_eq!(*mutations.lock().unwrap(), vec!["cancel_all_orders"]);
    }

    #[tokio::test]
    async fn shadow_request_restart_save_load_keeps_disabled_and_zero_mutations() {
        let dir = std::env::temp_dir().join(format!(
            "ts-shadow-restart-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let snapshot_path = dir.join("runtime-snapshot.json");
        let restart_flag_path = dir.join("restart.requested");
        let strategies_path = dir.join("strategies.json");
        std::fs::write(&strategies_path, "{}").unwrap();

        let inner = RecordingClient::default();
        let mutations = inner.mutations.clone();
        let backing: Arc<RwLock<dyn TradingClient<OrderRequest>>> = Arc::new(RwLock::new(inner));
        let guarded = ShadowGuardClient::wrap_counted(backing.clone(), Default::default());

        let manager = Arc::new(RwLock::new(manager_with_c2().await));
        manager
            .read()
            .await
            .set_strategy_enabled(&C2_ID.to_string(), false)
            .await;

        let shutdown = ShutdownManager::new(8);
        let trading_control = Arc::new(TradingControl::new(guarded.clone()));
        let operator = FileStrategyOperator::new(
            strategies_path.to_string_lossy().into_owned(),
            Arc::new(RwLock::new(Default::default())),
            manager.clone(),
            trading_control,
            shutdown.clone(),
        )
        .with_persist_paths(
            snapshot_path.to_string_lossy().into_owned(),
            restart_flag_path.to_string_lossy().into_owned(),
        );

        operator.request_restart("cancel_working").await.unwrap();
        cancel_working_orders_on_stop(ExecutionMode::Shadow, &guarded, &[Pair::new("BTC", "USDT")])
            .await;

        assert!(
            mutations.lock().unwrap().is_empty(),
            "RequestRestart Shadow stop must not mutate the venue: {:?}",
            mutations.lock().unwrap()
        );

        let loaded = RuntimeSnapshot::load(&snapshot_path)
            .unwrap()
            .expect("snapshot written by RequestRestart");
        assert_eq!(
            loaded.strategies,
            vec![SnapshotStrategy {
                strategy_id: "BTCPriceMALongFlatV1:BTC/USDT".into(),
                enabled: false,
            }]
        );

        let restored = manager_with_c2().await;
        assert!(
            !restored.is_strategy_enabled(&C2_ID.to_string()).await,
            "add_strategy must start disabled so startup enablement is the sole path"
        );
        let loaded_again =
            load_and_apply_strategy_enablement(&restored, &[sample_config(true)], &snapshot_path)
                .await
                .unwrap()
                .expect("snapshot still present for restore");
        assert_eq!(loaded_again.strategies, loaded.strategies);
        assert!(
            !restored.is_strategy_enabled(&C2_ID.to_string()).await,
            "persisted enabled=false must survive initially_enabled=true"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn clean_start_load_path_honors_initially_enabled_without_snapshot() {
        let dir = std::env::temp_dir().join(format!(
            "ts-clean-start-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("runtime-snapshot.json");
        assert!(!missing.exists());

        let manager = manager_with_c2().await;
        assert!(
            !manager.is_strategy_enabled(&C2_ID.to_string()).await,
            "add_strategy defaults to disabled"
        );
        let snap = load_and_apply_strategy_enablement(&manager, &[sample_config(true)], &missing)
            .await
            .unwrap();
        assert!(snap.is_none(), "missing snapshot must be a cold start");
        assert!(
            manager.is_strategy_enabled(&C2_ID.to_string()).await,
            "production startup with no snapshot must honor initially_enabled=true"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn initially_enabled_applies_when_snapshot_absent() {
        let manager = manager_with_c2().await;
        apply_strategy_enablement(&manager, &[sample_config(true)], None).await;
        assert!(manager.is_strategy_enabled(&C2_ID.to_string()).await);
    }
}
