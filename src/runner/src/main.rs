use runner::config::BotConfig;
use runner::shutdown::ShutdownManager;
use runner::{
    collect_configured_pairs, fetch_asset_data, initialize_asset_service,
    initialize_exchange_engine, initialize_indicator_service, initialize_market_data_engine,
    initialize_mongodb, setup_shutdown_handler,
};
use runner::{level_runner, strategy_runner, SharedServices};
use std::sync::Arc;
use tracing::{error, info};
use trading_core::persistence::mongo::client::MongoClient;
use trading_engine::state::manager::TradingStateManager;

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

    // Load and validate bot configuration
    let bot_config = BotConfig::load_from_file("config/bot.json")?;
    bot_config.validate()?;

    // Initialize logging
    initialize_logging(&bot_config);

    // Log startup info
    log_startup_info(&bot_config, &args.mode);

    // Initialize MongoDB if enabled
    let mongo_client = initialize_mongodb(&bot_config).await;

    // Create shutdown manager and setup Ctrl-C handler
    let shutdown_manager = ShutdownManager::new(bot_config.channels.shutdown_capacity);
    setup_shutdown_handler(shutdown_manager.clone());

    // Initialize Asset Service
    let asset_service = initialize_asset_service(bot_config.exchange.exchange.clone()).await?;

    // Collect all configured pairs and fetch asset data
    let configured_pairs = collect_configured_pairs(
        &bot_config,
        &args.mode,
        &args.levels_config,
        &args.strategies_config,
    )?;
    fetch_asset_data(
        &asset_service,
        bot_config.exchange.exchange.clone(),
        &configured_pairs,
    )
    .await;

    // Initialize Market Data Engine
    let (_, market_data_rx, market_data_handle) =
        initialize_market_data_engine(&bot_config, &shutdown_manager)?;

    // Wait for market data to start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    info!("Market Data Engine: WebSocket subscribed to orderbook stream");

    // Create indicator update channel (shared between service and runners)
    let (indicator_tx, indicator_rx) =
        trading_core::channels::indicators::indicator_update_channel(
            bot_config.channels.indicator_update_capacity,
        );

    // Initialize Indicator Service
    let indicator_handle = initialize_indicator_service(
        &bot_config,
        mongo_client.as_ref(),
        market_data_rx.resubscribe(),
        indicator_tx,
        &shutdown_manager,
    )
    .await?;

    // Initialize Exchange Engine
    let (exchange_state_manager, exchange_update_rx, trading_client) =
        initialize_exchange_engine(&bot_config, &shutdown_manager).await?;

    // Create Trading State Manager
    let trading_state_manager = Arc::new(TradingStateManager::new(
        exchange_update_rx.resubscribe(),
        exchange_state_manager.clone(),
    ));

    // Start Trading State Manager subscriber
    start_trading_state_subscriber(trading_state_manager.clone(), shutdown_manager.clone());

    // Create SharedServices
    let services = SharedServices {
        trading_client,
        exec_report_rx: exchange_update_rx.resubscribe(),
        market_data_rx: market_data_rx.resubscribe(),
        indicator_rx,
        shutdown_manager,
        market_data_handle,
        trading_ws_handle: tokio::spawn(async {}), // Dummy handle
        indicator_handle,
        trading_state_manager,
        asset_service: Some(asset_service),
    };

    // Route to appropriate runner
    match args.mode.as_str() {
        "strategies" => {
            let strategy_persistence = create_strategy_persistence(&mongo_client, &bot_config);
            strategy_runner(
                bot_config.clone(),
                args.strategies_config,
                services,
                strategy_persistence,
            )
            .await?;
        }
        "levels" => {
            let level_persistence = create_level_persistence(&mongo_client, &bot_config);
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

/// Initialize logging subsystem
fn initialize_logging(bot_config: &BotConfig) {
    let log_level = match bot_config.system.log_level.as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(log_level)
            .finish(),
    );
}

/// Log startup information
fn log_startup_info(bot_config: &BotConfig, mode: &str) {
    info!("=== Starting Trading System ===");
    info!("Mode: {}", mode);
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
}

/// Start Trading State Manager subscriber
fn start_trading_state_subscriber(
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

/// Create strategy persistence if MongoDB is enabled
fn create_strategy_persistence(
    mongo_client: &Option<Arc<MongoClient>>,
    bot_config: &BotConfig,
) -> Option<Arc<trading_engine::strategies::persistence::StrategyPersistence>> {
    mongo_client.as_ref().map(|client| {
        Arc::new(
            trading_engine::strategies::persistence::StrategyPersistence::new(
                client.as_ref().clone(),
                bot_config.mongo.bot_id.clone(),
            ),
        )
    })
}

/// Create level persistence if MongoDB is enabled
fn create_level_persistence(
    mongo_client: &Option<Arc<MongoClient>>,
    bot_config: &BotConfig,
) -> Option<Arc<trading_engine::levels::persistence::LevelPersistence>> {
    mongo_client.as_ref().map(|client| {
        Arc::new(trading_engine::levels::persistence::LevelPersistence::new(
            client.as_ref().clone(),
            bot_config.mongo.bot_id.clone(),
        ))
    })
}
