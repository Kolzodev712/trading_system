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
use tokio::sync::mpsc;
use trading_core::config::ChannelConfig;
use trading_core::venue::cex::traits::ExchangeDriver;
use trading_core::{
    error::EngineError,
    registry::symbol::SymbolRegistry,
    types::{common::Pair, exchange::OrderRequest},
    venue::cex::channels::cex_venue_update_channel,
    venue::cex::traits::TradingClient,
};

/// Run the exchange engine main event loop
///
/// This is the main event loop for the exchange engine. It:
/// 1. Creates channels for sending data to other services (cex_venue_update_channel)
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
        trading_core::venue::cex::channels::CexVenueUpdateReceiver,
        Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>,
    ),
    EngineError,
> {
    if pairs.is_empty() {
        return Err(EngineError::CustomError(smol_str::format_smolstr!(
            "No pairs provided to exchange engine"
        )));
    }

    // 1. Establish channels to send data to other services
    let (exchange_update_tx, exchange_update_rx) =
        cex_venue_update_channel(channels.cex_venue_update_capacity);

    // Use provided pairs for symbol registry (pairs come from bot config)
    let symbol_registry = Arc::new(SymbolRegistry::from_pairs(pairs.clone()));

    // 2. Create trading client first (needed for state manager)
    // Create ONE client instance that will be shared between driver and trading_client
    enum ExchangeClient {
        Binance(crate::exchanges::binance::client::BinanceClient),
        Bybit(crate::exchanges::bybit::client::BybitClient),
        Paper(
            crate::exchanges::k_paper::KPaperConfig,
            Arc<tokio::sync::RwLock<crate::exchanges::k_paper::client::KPaperState>>,
            mpsc::UnboundedReceiver<crate::exchanges::k_paper::messages::KPaperCommand>,
        ),
    }

    let (trading_client, exchange_client) = match &config.exchange_config {
        ExchangeConfig::Paper(paper_config) => {
            let kpaper_config = crate::exchanges::k_paper::KPaperConfig::from(paper_config.clone());
            let (client, command_rx) =
                crate::exchanges::k_paper::KPaperClient::new(kpaper_config.clone());
            let state = client.state();
            let trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>> =
                Arc::new(tokio::sync::RwLock::new(client));
            (
                trading_client,
                ExchangeClient::Paper(kpaper_config, state, command_rx),
            )
        }
        ExchangeConfig::Binance(binance_config) => {
            let client = crate::exchanges::binance::client::BinanceClient::new(
                binance_config.clone(),
                channels.websocket_inbound_capacity,
                channels.websocket_ping_capacity,
                channels.websocket_outbound_rpc_capacity,
                channels.shutdown_capacity,
            );
            let client_for_driver = ExchangeClient::Binance(client.clone());
            let trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>> =
                Arc::new(tokio::sync::RwLock::new(client));
            (trading_client, client_for_driver)
        }
        ExchangeConfig::Bybit(bybit_config) => {
            let client = crate::exchanges::bybit::client::BybitClient::new(
                bybit_config.clone(),
                channels.websocket_inbound_capacity,
                channels.websocket_ping_capacity,
                channels.websocket_outbound_rpc_capacity,
                channels.shutdown_capacity,
            );
            let client_for_driver = ExchangeClient::Bybit(client.clone());
            let trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>> =
                Arc::new(tokio::sync::RwLock::new(client));
            (trading_client, client_for_driver)
        }
    };

    if let ExchangeConfig::Bybit(bybit_config) = &config.exchange_config {
        if !matches!(
            bybit_config.instrument_type,
            trading_core::types::common::InstrumentType::Spot
        ) {
            let client = trading_client.read().await;
            match client
                .set_position_mode(trading_core::types::futures::PositionMode::OneWay)
                .await
            {
                Ok(()) => tracing::info!("Bybit position mode set to one-way"),
                Err(e) => {
                    return Err(EngineError::CustomError(smol_str::format_smolstr!(
                        "startup set_position_mode failed (fail-closed): {e}"
                    )));
                }
            }
            let leverage = bybit_config.leverage.ok_or_else(|| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Bybit futures leverage must be set explicitly; no default is invented"
                ))
            })?;
            for pair in &pairs {
                match client.set_leverage(pair, leverage).await {
                    Ok(info) => {
                        tracing::info!("Bybit leverage for {} set to {}x", pair, info.leverage)
                    }
                    Err(e) => {
                        return Err(EngineError::CustomError(smol_str::format_smolstr!(
                            "startup set_leverage failed for {pair} (fail-closed): {e}"
                        )));
                    }
                }
            }
        }
    }

    if let ExchangeConfig::Paper(paper_config) = &config.exchange_config {
        if !matches!(
            paper_config.instrument_type,
            trading_core::types::common::InstrumentType::Spot
        ) {
            let client = trading_client.read().await;
            client
                .set_position_mode(trading_core::types::futures::PositionMode::OneWay)
                .await
                .map_err(|e| {
                    EngineError::CustomError(smol_str::format_smolstr!(
                        "startup set_position_mode failed (fail-closed): {e}"
                    ))
                })?;
            let leverage = paper_config.leverage.ok_or_else(|| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Paper futures leverage must be set explicitly; no default is invented"
                ))
            })?;
            for pair in &pairs {
                client.set_leverage(pair, leverage).await.map_err(|e| {
                    EngineError::CustomError(smol_str::format_smolstr!(
                        "startup set_leverage failed for {pair} (fail-closed): {e}"
                    ))
                })?;
            }
        }
    }

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
    // Pass the same client instance to the driver that's used in trading_client
    enum ExchangeDriverEnum {
        Binance(crate::exchanges::binance::driver::BinanceDriver),
        Bybit(crate::exchanges::bybit::driver::BybitDriver),
        Paper(crate::exchanges::k_paper::PaperDriver),
    }

    // Paper only: signal when first balance message is received (used in readiness wait below)
    let mut paper_balance_ready_rx: Option<tokio::sync::oneshot::Receiver<()>> = None;

    let driver = match exchange_client {
        ExchangeClient::Binance(binance_client) => {
            ExchangeDriverEnum::Binance(crate::exchanges::binance::driver::BinanceDriver::new(
                binance_client,
                state_manager.clone(),
                trading_client.clone(),
            )?)
        }
        ExchangeClient::Bybit(bybit_client) => {
            ExchangeDriverEnum::Bybit(crate::exchanges::bybit::driver::BybitDriver::new(
                bybit_client,
                state_manager.clone(),
                trading_client.clone(),
            )?)
        }
        ExchangeClient::Paper(kpaper_config, state, command_rx) => {
            let (balance_ready_tx, balance_ready_rx) = tokio::sync::oneshot::channel();
            paper_balance_ready_rx = Some(balance_ready_rx);
            ExchangeDriverEnum::Paper(crate::exchanges::k_paper::PaperDriver::new(
                kpaper_config,
                state,
                command_rx,
                state_manager.clone(),
                trading_client.clone(),
                Some(balance_ready_tx),
            ))
        }
    };

    // Store client reference for readiness checking before moving driver
    let binance_client_for_readiness = match &driver {
        ExchangeDriverEnum::Binance(d) => Some(d.client().clone()),
        ExchangeDriverEnum::Bybit(_) | ExchangeDriverEnum::Paper(_) => None,
    };

    // 6. Power up the integration: Start the driver's event loop
    // The driver's run() method handles the WebSocket event loop and protocol communication
    // Use the new ExchangeDriver trait methods for better connection management
    let shutdown_rx_for_driver = shutdown_rx;
    let driver_handle = tokio::spawn(async move {
        let result = match driver {
            ExchangeDriverEnum::Binance(mut d) => d.run(shutdown_rx_for_driver).await,
            ExchangeDriverEnum::Bybit(mut d) => d.run(shutdown_rx_for_driver).await,
            ExchangeDriverEnum::Paper(mut d) => d.run(shutdown_rx_for_driver).await,
        };
        if let Err(e) = result {
            tracing::error!("Exchange driver error: {}", e);
        }
    });

    // 7. Wait for WebSocket connection to be ready before returning
    // This ensures the RPC channel is available before strategies start placing orders
    // Use the driver's is_connection_ready() method for proper readiness checking
    tracing::info!("Waiting for WebSocket connection to be ready...");
    let start = std::time::Instant::now();
    let timeout = tokio::time::Duration::from_secs(30);

    // Wait for connection to be ready
    // For Binance, we check the RPC channel directly
    // For Bybit/Paper, we give the driver a moment to start
    match &config.exchange_config {
        ExchangeConfig::Binance(_) => {
            if let Some(client) = binance_client_for_readiness {
                // Check if RPC channel is ready AND open (not just set)
                loop {
                    let rpc_tx = client.rpc_client.get_rpc_request_tx().await;
                    if let Some(tx) = rpc_tx {
                        if !tx.is_closed() {
                            tracing::info!(
                                "WebSocket connection ready, RPC channel available and open"
                            );
                            break;
                        }
                    }
                    if start.elapsed() > timeout {
                        tracing::warn!("Timeout waiting for WebSocket connection (30s), proceeding anyway - connection may still be establishing");
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
        ExchangeConfig::Bybit(_) => {
            tracing::info!(
                "Exchange driver starting, connections will be established in background"
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
        ExchangeConfig::Paper(_) => {
            tracing::info!("Paper driver starting, waiting for initial balance snapshot...");
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            if let Some(rx) = paper_balance_ready_rx.take() {
                match tokio::time::timeout(tokio::time::Duration::from_secs(15), rx).await {
                    Ok(Ok(())) => {
                        tracing::info!("Paper: received initial balance snapshot, proceeding");
                    }
                    Ok(Err(_)) => {}
                    Err(_) => {
                        tracing::warn!(
                            "Paper: timeout (15s) waiting for initial balance snapshot, proceeding anyway"
                        );
                    }
                }
            }
        }
    }

    // Store driver handle for potential cleanup (though it runs until shutdown)
    // In a production system, you might want to track this for graceful shutdown
    drop(driver_handle);

    Ok((state_manager, exchange_update_rx, trading_client))
}
