use exchange_engine::exchanges::binance::{config::BinanceConfig, ws_client::BinanceClient};
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
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use trading_core::{
    channels::indicator_update_channel, traits::TradingClient, types::common::Pair,
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
    let binance_config_json = bot_config
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
    let exchange_enum = bot_config.exchange.exchange.clone();
    // Create market data channel
    let (market_data_tx, market_data_rx) =
        trading_core::channels::market_data::market_data_update_channel(1000);

    let market_data_config = MarketDataConfig {
        redis_url: None, // Channels are used for internal communication
        exchange: exchange_enum,
        exchange_config: MDExchangeConfig::Binance(market_data_binance_config),
    };
    let mut market_data_driver = MarketDataDriver::new(market_data_config, market_data_tx)?;

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

    // 1.5. Initialize Indicator Service
    info!("Initializing Indicator Service...");
    // Create a separate receiver for the indicator service (fan-out from same channel)
    let indicator_market_data_rx = market_data_rx.resubscribe();
    let (indicator_tx, _indicator_rx) = indicator_update_channel(1000);

    let mut indicator_service = IndicatorService::new();
    indicator_service.initialize(
        indicator_market_data_rx,
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
    let (exec_report_tx, exec_report_rx) = tokio::sync::mpsc::channel(100);
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

    // Route based on mode
    let services = SharedServices {
        trading_client,
        exec_report_rx,
        market_data_rx,
        shutdown_manager,
        market_data_handle,
        trading_ws_handle,
        indicator_handle,
    };

    match args.mode.as_str() {
        "strategies" => {
            strategy_runner(bot_config.clone(), args.strategies_config, services).await?;
        }
        "levels" => {
            level_runner(bot_config.clone(), args.levels_config, services).await?;
        }
        _ => {
            return Err(
                format!("Unknown mode: {}. Use 'strategies' or 'levels'", args.mode).into(),
            );
        }
    }

    Ok(())
}
