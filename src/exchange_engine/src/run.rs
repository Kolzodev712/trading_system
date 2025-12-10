//! Exchange engine main event loop
//!
//! This is the main entry point for the exchange engine. It:
//! - Creates and configures the state manager
//! - Establishes channels for sending data to other services
//! - Configures reconciliation (bootstrap + periodic REST calls)
//! - Powers up the exchange-specific integration (driver)
//! - Runs the main event loop until shutdown

use crate::config::{ExchangeConfig, ExchangeEngineConfig};
use crate::state::manager::ExchangeStateManager;
use std::sync::Arc;
use tokio::sync::broadcast;
use trading_core::config::ChannelConfig;
use trading_core::traits::ExchangeDriver;
use trading_core::{
    channels::exchange::exchange_update_channel,
    error::EngineError,
    registry::symbol::SymbolRegistry,
    traits::TradingClient,
    types::{common::Pair, exchange::OrderRequest},
};

/// Run the exchange engine main event loop
///
/// This is the main event loop for the exchange engine. It:
/// 1. Creates channels for sending data to other services (exchange_update_channel)
/// 2. Creates and configures the state manager
/// 3. Configures reconciliation:
///    - Performs initial bootstrap reconciliation (REST calls)
///    - Starts periodic reconciliation at specified interval (REST calls)
/// 4. Powers up the exchange-specific integration (driver)
/// 5. Runs until shutdown signal is received
///
/// # Arguments
/// * `config` - Exchange engine configuration
/// * `shutdown_rx` - Receiver for shutdown signals
/// * `reconciliation_interval_secs` - Interval for periodic reconciliation REST calls (0 = disabled)
/// * `reconciliation_enabled` - Whether reconciliation is enabled
/// * `channels` - Channel configuration for buffer sizes
///
/// # Returns
/// Tuple of (state_manager, exchange_update_rx, trading_client) for external use
pub async fn run(
    config: ExchangeEngineConfig,
    shutdown_rx: broadcast::Receiver<()>,
    reconciliation_interval_secs: u64,
    reconciliation_enabled: bool,
    channels: ChannelConfig,
) -> Result<
    (
        Arc<ExchangeStateManager>,
        trading_core::channels::exchange::ExchangeUpdateReceiver,
        Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>,
    ),
    EngineError,
> {
    // 1. Establish channels to send data to other services
    let (exchange_update_tx, exchange_update_rx) =
        exchange_update_channel(channels.exchange_update_capacity);

    // Extract primary pair from config for symbol registry
    let primary_pair = get_primary_pair_from_config(&config)?;
    let pairs = vec![primary_pair.clone()];
    let symbol_registry = Arc::new(SymbolRegistry::from_pairs(pairs));

    // 2. Create trading client first (needed for state manager)
    let (trading_client, binance_config_for_driver) = match &config.exchange_config {
        ExchangeConfig::Binance(binance_config) => {
            let client = crate::exchanges::binance::ws_client::BinanceClient::new(
                binance_config.clone(),
                channels.websocket_inbound_capacity,
                channels.websocket_ping_capacity,
                channels.websocket_outbound_rpc_capacity,
                channels.shutdown_capacity,
            );
            let trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>> =
                Arc::new(tokio::sync::RwLock::new(client.clone()));
            (trading_client, binance_config.clone())
        }
    };

    // 3. Create state manager (top-level setup)
    let state_manager = Arc::new(ExchangeStateManager::new(
        exchange_update_tx,
        symbol_registry,
        trading_client.clone(),
        reconciliation_interval_secs,
        reconciliation_enabled,
    ));

    // 4. Configure reconciliation:
    //    - Perform initial bootstrap reconciliation (REST calls to initialize state)
    let configured_pairs = vec![primary_pair.clone()];

    //    - Start periodic reconciliation at specified interval (REST calls)
    tracing::info!(
        "Starting periodic exchange-level reconciliation (interval: {} seconds)",
        reconciliation_interval_secs
    );
    state_manager.start_reconciliation(configured_pairs);

    // 5. Power up the exchange-specific integration (driver handles protocol/communication)
    let mut driver = match config.exchange_config {
        ExchangeConfig::Binance(_) => crate::exchanges::binance::driver::BinanceDriver::new(
            binance_config_for_driver,
            state_manager.clone(),
            trading_client.clone(),
            channels.clone(),
        )?,
    };

    // 6. Power up the integration: Start the driver's event loop
    // The driver's run() method handles the WebSocket event loop and protocol communication
    // This is spawned so run() can return immediately with state_manager and channels
    tokio::spawn(async move {
        if let Err(e) = driver.run(shutdown_rx).await {
            tracing::error!("Exchange driver error: {}", e);
        }
    });

    Ok((state_manager, exchange_update_rx, trading_client))
}

/// Extract the primary trading pair from exchange config
fn get_primary_pair_from_config(config: &ExchangeEngineConfig) -> Result<Pair, EngineError> {
    match &config.exchange_config {
        ExchangeConfig::Binance(binance_config) => Ok(binance_config.symbol.clone().into()),
    }
}
