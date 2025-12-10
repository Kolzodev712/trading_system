use exchange_engine::exchanges::binance::config::BinanceConfig;
use exchange_engine::{config::ExchangeEngineConfig, run};
use indicator_engine::indicators::{IndicatorConfig, PriceSource};
use indicator_engine::IndicatorService;
use market_data_engine::{
    config::{ExchangeConfig as MDExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
};
use runner::config::BotConfig;
use runner::shutdown::ShutdownManager;
use runner::{level_runner, strategy_runner, SharedServices};
use smol_str::SmolStr;
use std::sync::Arc;
use tracing::{error, info, warn};
use trading_core::{
    channels::indicator_update_channel, persistence::mongo::client::MongoClient,
    types::common::Pair,
};

mod shutdown;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Trading mode: "strategies" or "levels"
    #[arg(short, long, default_value = "levels")]
    mode: String,

    /// Path to strategies config file (required if mode is "strategies")
    #[arg(short, long, default_value = "config/strategies.json")]
    strategies_config: String,

    /// Path to levels config file (required if mode is "levels")
    #[arg(short, long, default_value = "config/levels.json")]
    levels_config: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Load general bot configuration
    let bot_config = BotConfig::load_from_file("config/bot.json")?;
    bot_config.validate()?;

    // Initialize logging
    let log_level = match bot_config.system.log_level.as_str() {
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

    info!("=== Starting Trading System ===");
    info!("Mode: {}", args.mode);
    info!("Exchange: {:?}", bot_config.exchange.exchange);
    info!(
        "MongoDB persistence: {}",
        if bot_config.mongo.enabled {
            format!(
                "enabled ({}/{})",
                bot_config.mongo.connection_string, bot_config.mongo.database
            )
        } else {
            "disabled".to_string()
        }
    );

    // Initialize MongoDB client if enabled
    let mongo_client: Option<Arc<MongoClient>> = if bot_config.mongo.enabled {
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
    } else {
        None
    };

    // Create shutdown manager
    let shutdown_manager = ShutdownManager::new(bot_config.channels.shutdown_capacity);

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
    let binance_config_json = bot_config
        .exchange
        .exchange_specific
        .get("binance")
        .ok_or("Binance exchange configuration is required")?;

    let binance_config: BinanceConfig = serde_json::from_value(binance_config_json.clone())
        .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;

    let symbol: Pair = binance_config.symbol.clone().into();

    // 1. Start Market Data Engine
    info!(
        "Starting Market Data Engine (exchange: {:?}, symbol: {})...",
        bot_config.exchange.exchange, symbol
    );
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
    let exchange_enum = bot_config.exchange.exchange.clone();
    // Create market data channel
    let (market_data_tx, market_data_rx) =
        trading_core::channels::market_data::market_data_update_channel(
            bot_config.channels.market_data_capacity,
        );

    let market_data_config = MarketDataConfig {
        redis_url: None, // Channels are used for internal communication
        exchange: exchange_enum,
        exchange_config: MDExchangeConfig::Binance(market_data_binance_config),
    };
    let mut market_data_driver = MarketDataDriver::new(
        market_data_config,
        market_data_tx,
        bot_config.channels.clone(),
    )?;

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

    // Wait for market data to start publishing
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    info!(
        "Market Data Engine: WebSocket subscribed to orderbook stream for {}",
        symbol
    );

    // 1.5. Initialize Indicator Service
    info!(
        "Initializing Indicator Service (MongoDB persistence: {})...",
        if mongo_client.is_some() {
            "enabled"
        } else {
            "disabled"
        }
    );
    // Create a separate receiver for the indicator service (fan-out from same channel)
    let indicator_market_data_rx = market_data_rx.resubscribe();
    let (indicator_tx, _indicator_rx) =
        indicator_update_channel(bot_config.channels.indicator_update_capacity);

    // Initialize Indicator Service with persistence if enabled
    let mut indicator_service = if let Some(client) = &mongo_client {
        let persistence = Arc::new(indicator_engine::persistence::IndicatorPersistence::new(
            client.as_ref().clone(),
        ));
        IndicatorService::new().with_persistence(persistence)
    } else {
        IndicatorService::new()
    };

    indicator_service.initialize(
        indicator_market_data_rx,
        indicator_tx,
        1, // Publish every 1 second
    );

    // Load indicators from MongoDB if enabled
    if mongo_client.is_some() {
        if let Err(e) = indicator_service.load_from_mongo().await {
            warn!("Failed to load indicators from MongoDB: {e}");
        }
    }

    // Register default indicators (can be configured later)
    let pair_str = symbol.convert_to_smolstr();
    let sma_config = IndicatorConfig {
        id: SmolStr::new(format!("sma_20_{}", pair_str)),
        name: SmolStr::new("SMA"),
        pair: pair_str.clone(),
        period: 20,
        source: PriceSource::Close,
    };
    if let Err(e) = indicator_service.register_indicator(sma_config).await {
        warn!("Failed to register SMA indicator: {e}");
    }

    let ema_config = IndicatorConfig {
        id: SmolStr::new(format!("ema_12_{}", pair_str)),
        name: SmolStr::new("EMA"),
        pair: pair_str,
        period: 12,
        source: PriceSource::Close,
    };
    if let Err(e) = indicator_service.register_indicator(ema_config).await {
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

    // 2. Create Exchange Engine (exchange-agnostic runner)
    // Create exchange engine config
    let exchange_engine_config = ExchangeEngineConfig {
        exchange: trading_core::types::common::Exchange::Binance,
        exchange_config: exchange_engine::config::ExchangeConfig::Binance(binance_config.clone()),
    };

    // Start exchange engine with shutdown signal
    let shutdown_rx_exchange = shutdown_manager.subscribe();
    let reconciliation_enabled =
        bot_config.features.enable_reconciliation && bot_config.reconciliation.enabled;
    let exchange_reconciliation_interval_ms = bot_config.reconciliation.exchange_level_interval_ms;
    let exchange_reconciliation_interval_secs = exchange_reconciliation_interval_ms / 1000;

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
    let (exchange_state_manager, exchange_update_rx, trading_client) = run::run(
        exchange_engine_config,
        shutdown_rx_exchange,
        exchange_reconciliation_interval_secs,
        reconciliation_enabled,
        bot_config.channels.clone(),
    )
    .await?;

    // Create TradingStateManager (subscribes to ExchangeStateManager)
    info!("Creating Trading State Manager...");
    let trading_state_manager = Arc::new(trading_engine::state::manager::TradingStateManager::new(
        exchange_update_rx.resubscribe(),
        exchange_state_manager.clone(),
    ));

    info!("Exchange Engine: Bootstrap reconciliation completed");

    // Start TradingStateManager subscriber in background
    let trading_state_manager_for_subscriber = trading_state_manager.clone();
    let mut shutdown_rx_trading_state = shutdown_manager.subscribe();
    let _trading_state_subscriber_handle = tokio::spawn(async move {
        tokio::select! {
            result = trading_state_manager_for_subscriber.start_subscriber() => {
                if let Err(e) = result {
                    error!("TradingStateManager subscriber error: {e}");
                }
            }
            _ = shutdown_rx_trading_state.recv() => {
                info!("TradingStateManager subscriber shutting down");
            }
        }
    });

    // Note: Execution reports are published via ExchangeStateManager's exchange_update channel
    // Strategies should subscribe to ExchangeUpdate::Order from exchange_update_rx
    // We create a resubscribe for strategies to use
    let exec_report_rx = exchange_update_rx.resubscribe();

    // Exchange engine is ready:
    // - Bootstrap reconciliation succeeded (REST API confirmed working)
    // - WebSocket connection is handled by exchange-specific driver in background
    // - State manager is ready to receive updates

    // Create a dummy trading_ws_handle since the driver manages its own task
    let trading_ws_handle = tokio::spawn(async {});

    // Route based on mode
    let services = SharedServices {
        trading_client,
        exec_report_rx,
        market_data_rx,
        shutdown_manager,
        market_data_handle,
        trading_ws_handle,
        indicator_handle,
        trading_state_manager,
    };

    match args.mode.as_str() {
        "strategies" => {
            // Initialize MongoDB persistence for strategies if enabled
            let strategy_persistence = if let Some(client) = &mongo_client {
                Some(Arc::new(
                    trading_engine::strategies::persistence::StrategyPersistence::new(
                        client.as_ref().clone(),
                        bot_config.mongo.bot_id.clone(),
                    ),
                ))
            } else {
                None
            };
            strategy_runner(
                bot_config.clone(),
                args.strategies_config,
                services,
                strategy_persistence,
            )
            .await?;
        }
        "levels" => {
            // Initialize MongoDB persistence for levels if enabled
            let level_persistence = if let Some(client) = &mongo_client {
                Some(Arc::new(
                    trading_engine::levels::persistence::LevelPersistence::new(
                        client.as_ref().clone(),
                        bot_config.mongo.bot_id.clone(),
                    ),
                ))
            } else {
                None
            };
            level_runner(
                bot_config.clone(),
                args.levels_config,
                services,
                level_persistence,
            )
            .await?;
        }
        _ => {
            return Err(
                format!("Unknown mode: {}. Use 'strategies' or 'levels'", args.mode).into(),
            );
        }
    }

    Ok(())
}
