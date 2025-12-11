//! Test binary for running the full trading system in test mode
//!
//! This binary runs the entire trading system in a test/debug mode where:
//! - No actual trades are placed (uses test_open_order)
//! - All services are initialized and verified
//! - System runs for a configurable duration
//! - All components are tested end-to-end
//!
//! Usage:
//!   cargo run --bin test_runner -- --mode levels --duration 30

use runner::config::BotConfig;
use runner::shutdown::ShutdownManager;
use runner::{
    collect_configured_pairs, fetch_asset_data, initialize_asset_service,
    initialize_exchange_engine, initialize_indicator_service, initialize_market_data_engine,
    initialize_mongodb, setup_shutdown_handler,
};
use runner::{level_runner, strategy_runner, SharedServices};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use trading_core::persistence::mongo::client::MongoClient;
use trading_engine::state::manager::TradingStateManager;

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

    /// Test duration in seconds (0 = run until Ctrl-C)
    #[arg(short, long, default_value = "60")]
    duration: u64,

    /// Enable verbose debug logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Load and validate bot configuration
    let bot_config = BotConfig::load_from_file("config/bot.json")?;
    bot_config.validate()?;

    // Initialize logging with debug level for test mode
    let log_level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        match bot_config.system.log_level.as_str() {
            "trace" => tracing::Level::TRACE,
            "debug" => tracing::Level::DEBUG,
            "info" => tracing::Level::INFO,
            "warn" => tracing::Level::WARN,
            "error" => tracing::Level::ERROR,
            _ => tracing::Level::INFO,
        }
    };

    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(log_level)
            .finish(),
    );

    // Log startup info
    info!("=== Starting Trading System in TEST MODE ===");
    info!("Mode: {}", args.mode);
    info!("Duration: {} seconds", args.duration);
    info!("Exchange: {:?}", bot_config.exchange.exchange);
    info!("⚠️  TEST MODE: No actual trades will be placed");
    warn!("This is a test run - all orders will use test_open_order()");

    // Initialize MongoDB - REQUIRED for trading system
    let mongo_client = initialize_mongodb(&bot_config).await?;
    info!("✓ MongoDB connected");

    // Create shutdown manager and setup Ctrl-C handler
    let shutdown_manager = ShutdownManager::new(bot_config.channels.shutdown_capacity);
    setup_shutdown_handler(shutdown_manager.clone());
    info!("✓ Shutdown manager initialized");

    // Initialize Asset Service
    let asset_service = initialize_asset_service(bot_config.exchange.exchange.clone()).await?;
    info!("✓ Asset service initialized");

    // Collect all configured pairs and fetch asset data
    let configured_pairs = collect_configured_pairs(
        &bot_config,
        &args.mode,
        &args.levels_config,
        &args.strategies_config,
    )?;
    info!("✓ Collected {} configured pairs", configured_pairs.len());

    fetch_asset_data(
        &asset_service,
        bot_config.exchange.exchange.clone(),
        &configured_pairs,
    )
    .await;
    info!("✓ Asset data fetched");

    // Initialize Market Data Engine
    let (_, market_data_rx, market_data_handle) =
        initialize_market_data_engine(&bot_config, &shutdown_manager)?;
    info!("✓ Market data engine initialized");

    // Wait for market data to start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    info!("✓ Market Data Engine: WebSocket subscribed to orderbook stream");

    // Create indicator update channel (shared between service and runners)
    let (indicator_tx, indicator_rx) = trading_core::channels::indicators::indicator_update_channel(
        bot_config.channels.indicator_update_capacity,
    );

    // Initialize Indicator Service (requires MongoDB)
    let (indicator_service, indicator_handle, indicator_calculator) = initialize_indicator_service(
        &bot_config,
        &mongo_client,
        market_data_rx.resubscribe(),
        indicator_tx,
        &shutdown_manager,
    )
    .await?;
    info!("✓ Indicator service initialized");

    // Initialize Exchange Engine
    let (exchange_state_manager, exchange_update_rx, trading_client) =
        initialize_exchange_engine(&bot_config, &shutdown_manager).await?;
    info!("✓ Exchange engine initialized");

    // Create Trading State Manager
    let trading_state_manager = Arc::new(TradingStateManager::new(
        exchange_update_rx.resubscribe(),
        exchange_state_manager.clone(),
    ));
    info!("✓ Trading state manager initialized");

    // Start Trading State Manager subscriber
    start_trading_state_subscriber(trading_state_manager.clone(), shutdown_manager.clone());
    info!("✓ Trading state subscriber started");

    // Create SharedServices
    let services = SharedServices {
        trading_client,
        exec_report_rx: exchange_update_rx.resubscribe(),
        market_data_rx: market_data_rx.resubscribe(),
        indicator_rx,
        indicator_service,
        indicator_calculator,
        shutdown_manager: shutdown_manager.clone(),
        market_data_handle,
        trading_ws_handle: tokio::spawn(async {}), // Dummy handle
        indicator_handle,
        trading_state_manager,
        exchange_state_manager,
        asset_service: Some(asset_service),
        trading_control: None, // Created in runner functions
    };

    info!("✓ All services initialized successfully");
    info!("=== Starting {} runner ===", args.mode);

    // Start the appropriate runner in a background task
    let runner_handle = match args.mode.as_str() {
        "strategies" => {
            let strategy_persistence =
                create_strategy_persistence(Some(&mongo_client), &bot_config);
            let bot_config_clone = bot_config.clone();
            let strategies_config_clone = args.strategies_config.clone();
            tokio::spawn(async move {
                if let Err(e) = strategy_runner(
                    bot_config_clone,
                    strategies_config_clone,
                    services,
                    strategy_persistence,
                )
                .await
                {
                    error!("Strategy runner error: {}", e);
                }
            })
        }
        "levels" => {
            let level_persistence = create_level_persistence(Some(&mongo_client), &bot_config);
            let bot_config_clone = bot_config.clone();
            let levels_config_clone = args.levels_config.clone();
            tokio::spawn(async move {
                if let Err(e) = level_runner(
                    bot_config_clone,
                    levels_config_clone,
                    services,
                    level_persistence,
                )
                .await
                {
                    error!("Level runner error: {}", e);
                }
            })
        }
        _ => {
            return Err(
                format!("Unknown mode: {}. Use 'strategies' or 'levels'", args.mode).into(),
            );
        }
    };

    // Run for specified duration or until shutdown
    if args.duration > 0 {
        info!("Running test for {} seconds...", args.duration);
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(args.duration)) => {
                info!("Test duration reached, shutting down...");
                shutdown_manager.signal_shutdown();
            }
            _ = shutdown_manager.wait_for_shutdown() => {
                info!("Shutdown signal received");
            }
        }
    } else {
        info!("Running until Ctrl-C...");
        shutdown_manager.wait_for_shutdown().await;
    }

    // Wait for runner to finish
    info!("Waiting for runner to shutdown gracefully...");
    let _ = tokio::time::timeout(Duration::from_secs(10), runner_handle).await;

    info!("=== Test run completed ===");
    Ok(())
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

/// Create level persistence if MongoDB is enabled
fn create_level_persistence(
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

/// Create strategy persistence if MongoDB is enabled
fn create_strategy_persistence(
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
