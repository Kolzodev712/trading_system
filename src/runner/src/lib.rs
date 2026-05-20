pub mod config;
pub mod shutdown;

pub use gateway::grpc;

use asset_engine::AssetService;
use indicator_engine::{IndicatorService, LevelPricingRegistry};
use tokio::sync::Mutex;

use crate::config::BotConfig;
use crate::shutdown::ShutdownManager;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use trading_core::persistence::mongo::client::MongoClient;
use trading_core::types::common::Exchange;
use trading_core::venue::cex::traits::TradingClient;
use trading_core::venue::cex::types::exchange::OrderRequest;
use trading_core::venue::VenueMode;

use crate::venue::VenueInterface;
use trading_engine::state::manager::TradingStateManager;
use trading_networking::commands::MpscCommandRouter;
use trading_networking::gateway::backpressure::BackpressurePolicy;
use trading_networking::gateway::command_gateway::serve_zenoh_commands;
use trading_networking::gateway::event_gateway::EventGateway;
use trading_networking::gateway::transports::zenoh::{
    open_default_session, ZenohEventTransport, ZenohKeyspace,
};

pub mod commands;
pub mod dex_runner;
pub mod init;
pub mod level_runner;
pub mod strategy_runner;
pub mod venue;

pub(crate) async fn maybe_start_zenoh_gateway(
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

/// Shared services needed for both modes
pub struct SharedServices {
    pub venue: venue::VenueInterface,
    pub market_data_rx: trading_core::channels::market_data::MarketDataUpdateReceiver,
    pub indicator_rx: trading_core::channels::indicators::IndicatorUpdateReceiver,
    pub indicator_service: Arc<Mutex<IndicatorService>>,
    /// Clone of indicator service level bindings (avoids locking `indicator_service` while its run loop holds the mutex).
    pub level_pricing: Arc<parking_lot::RwLock<LevelPricingRegistry>>,
    pub indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
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
    init::setup_shutdown_handler(shutdown_manager.clone());

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
    let asset_service = init::initialize_asset_service(
        bot_config.exchange_engine.exchange.clone(),
        paper_rest_base,
    )
    .await?;
    let needs_cex_md = matches!(bot_config.venue.mode, VenueMode::Cex | VenueMode::Both);

    let configured_pairs = if needs_cex_md {
        let pairs = bot_config.get_pairs()?;
        if pairs.is_empty() {
            return Err("No pairs configured in bot.json".into());
        }
        init::fetch_asset_data(
            &asset_service,
            bot_config.exchange_engine.exchange.clone(),
            &pairs,
        )
        .await;
        pairs
    } else {
        Vec::new()
    };

    let (venue, market_data_rx, market_data_handle, market_data_state_manager) = if needs_cex_md {
        if bot_config.exchange_engine.exchange == Exchange::Paper {
            let cex = init::build_cex_handle(&bot_config, &shutdown_manager).await?;
            let (_, market_data_rx, market_data_handle, market_data_state_manager) =
                init::initialize_market_data_engine(&bot_config, &shutdown_manager)?;
            let venue = if bot_config.venue.mode == VenueMode::Both {
                let dex_config = bot_config
                    .dex_engine
                    .clone()
                    .ok_or("dex_engine required when venue.mode is both")?;
                let dex =
                    init::build_dex_handle(&bot_config, dex_config, &shutdown_manager).await?;
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
                init::initialize_market_data_engine(&bot_config, &shutdown_manager)?;
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            info!("Market Data Engine: WebSocket subscribed to orderbook stream");
            let venue = init::initialize_venue(&bot_config, &shutdown_manager).await?;
            (
                venue,
                market_data_rx,
                market_data_handle,
                market_data_state_manager,
            )
        }
    } else {
        let venue = init::initialize_venue(&bot_config, &shutdown_manager).await?;
        let (market_data_tx, market_data_rx) =
            trading_core::channels::market_data::market_data_update_channel(
                bot_config.channels.market_data_capacity,
            );
        let symbol_registry = Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
            vec![],
        ));
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
    };

    if needs_cex_md && bot_config.exchange_engine.exchange != Exchange::Paper {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        info!("Market Data Engine: WebSocket subscribed to orderbook stream");
    }

    let (indicator_tx, indicator_rx) = trading_core::channels::indicators::indicator_update_channel(
        bot_config.channels.indicator_update_capacity,
    );
    let (indicator_service, indicator_handle, indicator_calculator, level_pricing) =
        init::initialize_indicator_service(
            &bot_config,
            &mongo_client,
            market_data_rx.resubscribe(),
            indicator_tx,
            &shutdown_manager,
            configured_pairs.as_slice(),
        )
        .await?;

    let trading_state_manager = match &venue {
        VenueInterface::Cex(cex) | VenueInterface::Both { cex, .. } => {
            let tsm = Arc::new(TradingStateManager::new(
                cex.cex_update_rx.resubscribe(),
                cex.exchange_state_manager.clone(),
            ));
            init::start_trading_state_subscriber(tsm.clone(), shutdown_manager.clone());
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
        trading_state_manager,
        market_data_state_manager,
        asset_service: Some(asset_service),
        trading_control: None,
        tracing_handles,
    };

    match mode.as_str() {
        "strategies" => {
            let strategy_persistence =
                init::create_strategy_persistence(Some(&mongo_client), &bot_config);
            strategy_runner(
                bot_config,
                strategies_config,
                services,
                strategy_persistence,
            )
            .await?;
        }
        "levels" => {
            if services.venue.mode() != VenueMode::Cex {
                return Err("levels mode requires venue.mode \"cex\"".into());
            }
            let level_persistence =
                init::create_level_persistence(Some(&mongo_client), &bot_config);
            level_runner(bot_config, levels_config, services, level_persistence).await?;
        }
        "dex" => {
            if !matches!(services.venue.mode(), VenueMode::Dex | VenueMode::Both) {
                return Err("dex mode requires venue.mode \"dex\" or \"both\"".into());
            }
            dex_runner(bot_config, services).await?;
        }
        _ => {
            return Err(
                format!("Unknown mode: {mode}. Use 'strategies', 'levels', or 'dex'").into(),
            );
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

pub use dex_runner::dex_runner;
/// Re-export init helpers and runners so binaries and tests can use `runner::initialize_mongodb` etc.
pub use init::{
    build_cex_handle, build_dex_handle, create_level_persistence, create_shutdown_manager,
    create_strategy_persistence, fetch_asset_data, initialize_asset_service,
    initialize_exchange_engine, initialize_indicator_service, initialize_market_data_engine,
    initialize_mongodb, initialize_venue, setup_shutdown_handler, start_trading_state_subscriber,
};
pub use level_runner::level_runner;
pub use strategy_runner::strategy_runner;
