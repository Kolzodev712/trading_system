//! Initialization helpers: MongoDB, asset/market_data/indicator/exchange engines, persistence, shutdown.

use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::config::BotConfig;
use crate::shutdown::ShutdownManager;

use asset_engine::AssetService;
use indicator_engine::IndicatorService;
use market_data_engine::{
    config::{ExchangeConfig as MDExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
};
use trading_core::persistence::mongo::client::MongoClient;
use trading_core::traits::TradingClient;
use trading_core::types::common::{Exchange, Pair};
use trading_core::types::exchange::OrderRequest;
use trading_engine::state::manager::TradingStateManager;

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

    let pairs = bot_config.get_pairs()?;
    if pairs.is_empty() {
        return Err("No pairs configured in bot.json".into());
    }

    let market_data_config = match bot_config.exchange_engine.exchange {
        Exchange::Binance => {
            let binance_config: exchange_engine::exchanges::binance::config::BinanceConfig =
                serde_json::from_value(bot_config.exchange_engine.config.clone())
                    .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;

            MarketDataConfig {
                redis_url: None,
                exchange: bot_config.exchange_engine.exchange.clone(),
                exchange_config: MDExchangeConfig::Binance(MDBinanceConfig {
                    pairs: pairs.clone(),
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
                    pairs: pairs.clone(),
                    limit: config_json
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(50),
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
        Arc<parking_lot::RwLock<indicator_engine::LevelPricingRegistry>>,
    ),
    Box<dyn std::error::Error>,
> {
    info!("Initializing Indicator Service (MongoDB persistence: enabled)...");

    let indicator_market_data_rx = market_data_rx.resubscribe();

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

    let mongo_count = match indicator_service.lock().await.load_from_mongo().await {
        Ok(count) => {
            info!("Loaded {} indicator(s) from MongoDB", count);
            count
        }
        Err(e) => {
            warn!(
                "Failed to load indicators from MongoDB: {}. Continuing with config file fallback.",
                e
            );
            0
        }
    };

    let config_path = std::path::Path::new("config/indicators.json");
    let file_count = if mongo_count > 0 {
        info!(
            "Skipping indicator registration from {} (MongoDB is source of truth for indicators)",
            config_path.display()
        );
        let pricing_count = {
            let svc = indicator_service.lock().await;
            if svc.level_pricing_registry().read().is_empty() {
                match svc.load_level_pricing_from_config(config_path) {
                    Ok(n) => {
                        info!(
                            "Loaded {} level pricing binding(s) from {}",
                            n,
                            config_path.display()
                        );
                        n
                    }
                    Err(indicator_engine::service::ServiceError::ConfigNotFound(_)) => 0,
                    Err(e) => {
                        warn!("Failed to load level pricing from file: {}", e);
                        0
                    }
                }
            } else {
                svc.level_pricing_registry().read().len()
            }
        };
        let _ = pricing_count;
        0
    } else {
        match indicator_service.lock().await.load_from_config(config_path) {
            Ok(count) => {
                info!(
                    "Loaded {} indicator(s) and level pricing from {}",
                    count,
                    config_path.display()
                );
                count
            }
            Err(indicator_engine::service::ServiceError::ConfigNotFound(path)) => {
                warn!(
                    "Indicator config file not found at {}. Generate with: cd python && uv run indicator-generate --pairs <PAIRS> --output ../config/indicators.json",
                    path
                );
                0
            }
            Err(e) => {
                warn!("Failed to load indicators from config file: {}", e);
                0
            }
        }
    };

    if mongo_count == 0 && file_count == 0 {
        warn!(
            "No indicators loaded from MongoDB or {}. Levels with price Binding will not resolve until indicators are configured.",
            config_path.display()
        );
    }

    let _ = configured_pairs;

    let total_indicators = indicator_service.lock().await.get_indicator_count();
    info!(
        "Total indicators registered: {} (across {} pair(s))",
        total_indicators,
        configured_pairs.len()
    );

    let (indicator_calculator, level_pricing) = {
        let service = indicator_service.lock().await;
        (service.calculator(), service.level_pricing_registry())
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
        level_pricing,
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
        Arc<RwLock<dyn TradingClient<OrderRequest>>>,
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

    let (exchange_engine_config, pairs) = match bot_config.exchange_engine.exchange {
        Exchange::Binance => {
            let binance_config: exchange_engine::exchanges::binance::config::BinanceConfig =
                serde_json::from_value(bot_config.exchange_engine.config.clone())
                    .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;

            if binance_config.pairs.is_empty() {
                return Err("No pairs configured in binance config".into());
            }

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
