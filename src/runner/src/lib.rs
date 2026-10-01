pub mod cex;
pub mod config;
pub mod dex;
pub mod launch;
pub mod platform;

pub use gateway::grpc;

use asset_engine::AssetService;
use indicator_engine::{IndicatorService, LevelPricingRegistry};
use tokio::sync::Mutex;

use crate::config::mode_config::ResolvedModeConfig;
use crate::config::BotConfig;
use crate::platform::shutdown::ShutdownManager;
use crate::platform::venue::VenueInterface;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;
use trading_core::persistence::mongo::client::MongoClient;
use trading_core::types::common::Exchange;
use trading_core::venue::cex::traits::TradingClient;
use trading_core::venue::cex::types::exchange::OrderRequest;
use trading_core::venue::VenueMode;
use trading_engine::state::manager::TradingStateManager;
use trading_engine::strategies::ExtraStrategyFactory;

/// Shared services needed for both modes
pub struct SharedServices {
    pub venue: VenueInterface,
    pub market_data_rx: trading_core::channels::market_data::MarketDataUpdateReceiver,
    pub indicator_rx: trading_core::channels::indicators::IndicatorUpdateReceiver,
    pub indicator_service: Arc<Mutex<IndicatorService>>,
    /// Clone of indicator service level bindings (avoids locking `indicator_service` while its run loop holds the mutex).
    pub level_pricing: Arc<parking_lot::RwLock<LevelPricingRegistry>>,
    pub indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
    pub dex_indicator_engine: Option<Arc<trading_engine::dex::DexIndicatorEngine>>,
    pub shutdown_manager: ShutdownManager,
    pub market_data_handle: JoinHandle<()>,
    pub trading_ws_handle: JoinHandle<()>,
    pub indicator_handle: JoinHandle<()>,
    pub trading_state_manager: Option<Arc<trading_engine::state::manager::TradingStateManager>>,
    pub market_data_state_manager: Arc<market_data_engine::state::manager::MarketDataStateManager>,
    pub asset_service: Option<Arc<AssetService>>,
    pub trading_control: Option<Arc<trading_core::grpc::TradingControl>>,
    /// Tracing exporter handles (for /metrics endpoint when present).
    pub tracing_handles: Option<trading_trace::ExporterHandles>,
}

/// Main entry point: creates shutdown manager, builds services, and runs the chosen runner.
pub async fn run(
    bot_config: BotConfig,
    mode: String,
    config_paths: ResolvedModeConfig,
    mongo_client: Option<Arc<MongoClient>>,
    tracing_handles: Option<trading_trace::ExporterHandles>,
    extra_factory: Option<Arc<dyn ExtraStrategyFactory>>,
) -> Result<(), Box<dyn std::error::Error>> {
    bot_config.validate()?;
    bot_config.validate_for_runner(&mode)?;
    if bot_config.venue.mode == VenueMode::Cex {
        let (execution_mode, venue, environment) =
            crate::config::execution_status_from_bot(&bot_config);
        match mode.as_str() {
            "levels" => {
                crate::config::levels_exchange_is_authorized(execution_mode, &venue, &environment)?
            }
            "strategies" => crate::config::strategies_exchange_is_authorized(
                execution_mode,
                &venue,
                &environment,
            )?,
            _ => {}
        }
    }

    let shutdown_manager = ShutdownManager::new(bot_config.channels.shutdown_capacity);
    platform::init::setup_shutdown_handler(shutdown_manager.clone());

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
    let asset_service = platform::init::initialize_asset_service(
        bot_config.exchange_engine.exchange.clone(),
        paper_rest_base,
    )
    .await?;
    let needs_cex_md = matches!(bot_config.venue.mode, VenueMode::Cex | VenueMode::Both);
    let needs_dex_md = matches!(bot_config.venue.mode, VenueMode::Dex | VenueMode::Both);

    let configured_pairs = if needs_cex_md {
        let pairs = bot_config.get_pairs()?;
        if pairs.is_empty() {
            return Err("No pairs configured in bot.json".into());
        }
        platform::init::fetch_asset_data(
            &asset_service,
            bot_config.exchange_engine.exchange.clone(),
            &pairs,
        )
        .await;
        pairs
    } else if needs_dex_md {
        let dex = bot_config
            .dex_engine
            .as_ref()
            .ok_or("dex_engine required when venue.mode is dex")?;
        if dex.chain == dex_engine::config::DexChain::Sui
            && dex.venue == trading_core::venue::VenueId::DeepBook
        {
            let deepbook = dex
                .deepbook_config()
                .map_err(|e| format!("invalid dex_engine config: {e}"))?;
            deepbook
                .pools
                .iter()
                .map(|p| dex_engine::pool_name_to_pair(p.as_str()))
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let (venue, market_data_rx, market_data_handle, market_data_state_manager) = if needs_cex_md {
        if bot_config.exchange_engine.exchange == Exchange::Paper {
            let cex = platform::init::build_cex_handle(&bot_config, &shutdown_manager).await?;
            let (_, market_data_rx, market_data_handle, market_data_state_manager) =
                platform::init::initialize_market_data_engine(&bot_config, &shutdown_manager)?;
            let venue = if bot_config.venue.mode == VenueMode::Both {
                let dex_config = bot_config
                    .dex_engine
                    .clone()
                    .ok_or("dex_engine required when venue.mode is both")?;
                let dex =
                    platform::init::build_dex_handle(&bot_config, dex_config, &shutdown_manager)
                        .await?;
                VenueInterface::Both { cex, dex }
            } else {
                VenueInterface::Cex(cex)
            };
            (
                venue,
                market_data_rx,
                market_data_handle,
                market_data_state_manager,
            )
        } else {
            let (_, market_data_rx, market_data_handle, market_data_state_manager) =
                platform::init::initialize_market_data_engine(&bot_config, &shutdown_manager)?;
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            info!("Market Data Engine: WebSocket subscribed to orderbook stream");
            let venue = platform::init::initialize_venue(&bot_config, &shutdown_manager).await?;
            (
                venue,
                market_data_rx,
                market_data_handle,
                market_data_state_manager,
            )
        }
    } else if needs_dex_md {
        let dex = bot_config
            .dex_engine
            .as_ref()
            .ok_or("dex_engine required when venue.mode is dex")?;
        if dex.chain == dex_engine::config::DexChain::Sui
            && dex.venue == trading_core::venue::VenueId::DeepBook
        {
            let venue = platform::init::initialize_venue(&bot_config, &shutdown_manager).await?;
            let (_, market_data_rx, market_data_handle, market_data_state_manager) =
                platform::init::initialize_dex_market_data_engine(&bot_config, &shutdown_manager)?;
            (
                venue,
                market_data_rx,
                market_data_handle,
                market_data_state_manager,
            )
        } else {
            let venue = platform::init::initialize_venue(&bot_config, &shutdown_manager).await?;
            let (market_data_tx, market_data_rx) =
                trading_core::channels::market_data::market_data_update_channel(
                    bot_config.channels.market_data_capacity,
                );
            let symbol_registry = Arc::new(
                trading_core::registry::symbol::SymbolRegistry::from_pairs(vec![]),
            );
            let market_data_state_manager = Arc::new(
                market_data_engine::state::manager::MarketDataStateManager::new(
                    market_data_tx.clone(),
                    symbol_registry,
                ),
            );
            let market_data_handle = tokio::spawn(async {});
            (
                venue,
                market_data_rx,
                market_data_handle,
                market_data_state_manager,
            )
        }
    } else {
        return Err("invalid venue.mode configuration".into());
    };

    if platform::init::should_spawn_deepbook_sidecar(&bot_config) {
        let _deepbook_md_handle = platform::init::spawn_deepbook_market_data_sidecar(
            &bot_config,
            &shutdown_manager,
            market_data_state_manager.clone(),
        )?;
    }

    if needs_cex_md && bot_config.exchange_engine.exchange != Exchange::Paper {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        info!("Market Data Engine: WebSocket subscribed to orderbook stream");
    }

    let (indicator_tx, indicator_rx) = trading_core::channels::indicators::indicator_update_channel(
        bot_config.channels.indicator_update_capacity,
    );
    let (
        (indicator_service, indicator_handle, indicator_calculator, level_pricing),
        dex_indicator_engine,
    ) = if bot_config.venue.mode == VenueMode::Dex {
        platform::init::initialize_dex_indicator_service(
            config_paths
                .dex_indicators
                .as_deref()
                .unwrap_or("config/dex/indicators.json"),
            market_data_rx.resubscribe(),
            &shutdown_manager,
        )?
    } else {
        let mongo = mongo_client
            .as_ref()
            .ok_or("MongoDB is required for this runner mode")?;
        let (svc, handle, calc, pricing) = platform::init::initialize_indicator_service(
            &bot_config,
            mongo,
            market_data_rx.resubscribe(),
            indicator_tx,
            &shutdown_manager,
            configured_pairs.as_slice(),
        )
        .await?;
        ((svc, handle, calc, pricing), None)
    };

    let trading_state_manager = match &venue {
        VenueInterface::Cex(cex) | VenueInterface::Both { cex, .. } => {
            let instrument_type = bot_config
                .get_instrument_type()
                .unwrap_or(trading_core::types::common::InstrumentType::Spot);
            let ledger = Arc::new(
                trading_engine::state::ExecutionLedger::open(&bot_config.execution_ledger_path)
                    .map_err(|err| format!("execution ledger: {err}"))?,
            );
            let tsm = Arc::new(
                TradingStateManager::new_with_instrument_type(
                    cex.cex_update_rx.resubscribe(),
                    cex.exchange_state_manager.clone(),
                    instrument_type,
                )
                .with_execution_ledger(ledger),
            );
            platform::init::start_trading_state_subscriber(tsm.clone(), shutdown_manager.clone());
            Some(tsm)
        }
        VenueInterface::Dex { .. } => None,
    };

    let services = SharedServices {
        venue,
        market_data_rx: market_data_rx.resubscribe(),
        indicator_rx,
        indicator_service,
        level_pricing,
        indicator_calculator,
        shutdown_manager,
        market_data_handle,
        trading_ws_handle: tokio::spawn(async {}),
        indicator_handle,
        dex_indicator_engine,
        trading_state_manager,
        market_data_state_manager,
        asset_service: Some(asset_service),
        trading_control: None,
        tracing_handles,
    };

    match mode.as_str() {
        "strategies" => {
            cex::strategy_runner_with(
                bot_config,
                config_paths
                    .strategies
                    .unwrap_or_else(|| "config/cex/strategies.json".to_string()),
                services,
                extra_factory,
            )
            .await?;
        }
        "levels" => {
            let _ = extra_factory;
            let level_persistence =
                platform::init::create_level_persistence(mongo_client.as_ref(), &bot_config);
            cex::level_runner(
                bot_config,
                config_paths
                    .levels
                    .unwrap_or_else(|| "config/cex/levels.json".to_string()),
                services,
                level_persistence,
            )
            .await?;
        }
        "dex" => {
            let _ = extra_factory;
            if !matches!(services.venue.mode(), VenueMode::Dex | VenueMode::Both) {
                return Err("dex mode requires venue.mode \"dex\" or \"both\"".into());
            }
            dex::run_levels(
                bot_config,
                config_paths
                    .dex_levels
                    .unwrap_or_else(|| "config/dex/levels.json".to_string()),
                services,
            )
            .await?;
        }
        "dex-strategies" => {
            let _ = extra_factory;
            if !matches!(services.venue.mode(), VenueMode::Dex | VenueMode::Both) {
                return Err("dex-strategies mode requires venue.mode \"dex\" or \"both\"".into());
            }
            dex::run_strategies(
                bot_config,
                config_paths
                    .dex_strategies
                    .unwrap_or_else(|| "config/dex/strategies.json".to_string()),
                services,
            )
            .await?;
        }
        _ => {
            let _ = extra_factory;
            return Err(format!(
                "Unknown mode: {mode}. Use 'levels', 'strategies', 'dex', or 'dex-strategies'"
            )
            .into());
        }
    }

    trading_trace::shutdown_exporters();
    Ok(())
}

impl SharedServices {
    pub fn trading_client(&self) -> Result<Arc<RwLock<dyn TradingClient<OrderRequest>>>, String> {
        self.venue
            .trading_client()
            .ok_or_else(|| "levels/strategies require venue.mode \"cex\"".to_string())
    }

    pub fn exchange_state_manager(
        &self,
    ) -> Result<Arc<exchange_engine::state::manager::ExchangeStateManager>, String> {
        self.venue
            .exchange_state_manager()
            .ok_or_else(|| "CEX exchange state requires venue.mode \"cex\"".to_string())
    }

    pub fn exec_report_rx(
        &self,
    ) -> Result<trading_core::venue::cex::channels::CexVenueUpdateReceiver, String> {
        self.venue
            .cex_update_rx()
            .ok_or_else(|| "CEX execution reports require venue.mode \"cex\"".to_string())
    }
}

// --- Backward-compatible re-exports (prefer `cex::`, `dex::`, `platform::`, `config::`) ---

pub use cex::{level_runner, strategy_runner, strategy_runner_with};
pub use config::mode_config;
pub use dex::{run as dex_runner, run_levels, run_strategies, DexRunConfig};
pub use launch::launch;
pub use platform::build_metrics_provider;
pub use platform::commands;
pub use platform::event_bridge;
pub use platform::health as platform_health;
pub use platform::init;
pub use platform::shutdown;
pub use platform::venue;

pub use platform::init::{
    build_cex_handle, build_dex_handle, create_level_persistence, create_shutdown_manager,
    fetch_asset_data, initialize_asset_service, initialize_exchange_engine,
    initialize_indicator_service, initialize_market_data_engine, initialize_mongodb,
    initialize_venue, setup_shutdown_handler, start_trading_state_subscriber,
};
