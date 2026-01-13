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
    pairs: Vec<Pair>,
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
    if pairs.is_empty() {
        return Err(EngineError::CustomError(anyhow::anyhow!(
            "No pairs provided to exchange engine"
        )));
    }

    // 1. Establish channels to send data to other services
    let (exchange_update_tx, exchange_update_rx) =
        exchange_update_channel(channels.exchange_update_capacity);

    // Use provided pairs for symbol registry (pairs come from bot config)
    let symbol_registry = Arc::new(SymbolRegistry::from_pairs(pairs.clone()));

    // 2. Create trading client first (needed for state manager)
    // Create ONE BinanceClient instance that will be shared between driver and trading_client
    // Since BinanceClient contains Arc<BinanceRpcClient>, cloning it shares the same RPC client instance
    let (trading_client, binance_client_for_driver, _binance_config_for_driver) =
        match &config.exchange_config {
            ExchangeConfig::Binance(binance_config) => {
                let client = crate::exchanges::binance::websocket::ws_client::BinanceClient::new(
                    binance_config.clone(),
                    channels.websocket_inbound_capacity,
                    channels.websocket_ping_capacity,
                    channels.websocket_outbound_rpc_capacity,
                    channels.shutdown_capacity,
                );
                // Clone the client - since it contains Arc<BinanceRpcClient>, both clones share the same RPC client
                let client_for_driver = client.clone();
                let trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>> =
                    Arc::new(tokio::sync::RwLock::new(client));
                (trading_client, client_for_driver, binance_config.clone())
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
    let configured_pairs = pairs.clone();

    // Perform bootstrap reconciliation
    let bootstrap_result = state_manager.bootstrap_reconcile(&configured_pairs).await?;
    tracing::info!(
        "Bootstrap reconciliation: {} balances, {} orders, {} trades fetched{}",
        bootstrap_result.balances_fetched,
        bootstrap_result.orders_fetched,
        bootstrap_result.trades_fetched,
        if bootstrap_result.has_discrepancies() {
            format!(
                ", {} discrepancies resolved",
                bootstrap_result.discrepancies.len()
            )
        } else {
            ", no discrepancies".to_string()
        }
    );

    //    - Start periodic reconciliation at specified interval (REST calls)
    if reconciliation_enabled && reconciliation_interval_secs > 0 {
        tracing::debug!(
            "Starting periodic exchange-level reconciliation (interval: {} seconds)",
            reconciliation_interval_secs
        );
    }
    state_manager.start_reconciliation(configured_pairs);

    // 5. Power up the exchange-specific integration (driver handles protocol/communication)
    // Pass the same BinanceClient instance to the driver that's used in trading_client
    // Since BinanceClient contains Arc<BinanceRpcClient>, both will share the same RPC channel
    let mut driver = match config.exchange_config {
        ExchangeConfig::Binance(_) => crate::exchanges::binance::driver::BinanceDriver::new(
            binance_client_for_driver,
            state_manager.clone(),
            trading_client.clone(),
        )?,
    };

    // Keep client reference for readiness checking before moving driver
    let client_for_readiness = driver.client().clone();

    // 6. Power up the integration: Start the driver's event loop
    // The driver's run() method handles the WebSocket event loop and protocol communication
    // Use the new ExchangeDriver trait methods for better connection management
    let shutdown_rx_for_driver = shutdown_rx;
    let driver_handle = tokio::spawn(async move {
        if let Err(e) = driver.run(shutdown_rx_for_driver).await {
            tracing::error!("Exchange driver error: {}", e);
        }
    });

    // 7. Wait for WebSocket connection to be ready before returning
    // This ensures the RPC channel is available before strategies start placing orders
    // Use the driver's is_connection_ready() method for proper readiness checking
    tracing::info!("Waiting for WebSocket connection to be ready...");
    let start = std::time::Instant::now();
    let timeout = tokio::time::Duration::from_secs(30);

    // Create a temporary driver reference for readiness checking
    // Note: We can't use driver.is_connection_ready() here since driver was moved
    // So we fall back to checking the client directly
    loop {
        // Check if RPC channel is ready AND open (not just set)
        let rpc_ready = {
            let rpc_tx = client_for_readiness.rpc_client.get_rpc_request_tx().await;
            if let Some(tx) = rpc_tx {
                // Channel exists - check if it's actually open
                !tx.is_closed()
            } else {
                false
            }
        };
        if rpc_ready {
            tracing::info!("WebSocket connection ready, RPC channel available and open");
            break;
        }

        if start.elapsed() > timeout {
            tracing::warn!("Timeout waiting for WebSocket connection (30s), proceeding anyway - connection may still be establishing");
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Store driver handle for potential cleanup (though it runs until shutdown)
    // In a production system, you might want to track this for graceful shutdown
    drop(driver_handle);

    Ok((state_manager, exchange_update_rx, trading_client))
}
