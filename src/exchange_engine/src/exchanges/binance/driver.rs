//! Binance exchange driver
//!
//! This driver manages the Binance WebSocket connection and event loop.
//! It integrates with the state manager to update and publish exchange state.

use super::ws_client::BinanceClient;
use crate::state::manager::ExchangeStateManager;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info};
use trading_core::config::ChannelConfig;
use trading_core::traits::ExchangeDriver;
use trading_core::{error::EngineError, traits::TradingClient, types::exchange::OrderRequest};

/// Binance exchange driver
///
/// Manages the Binance WebSocket connection, event loop, and state management.
pub struct BinanceDriver {
    client: BinanceClient,
    state_manager: Arc<ExchangeStateManager>,
    trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>,
}

impl BinanceDriver {
    /// Create a new Binance driver
    ///
    /// The driver handles protocol-specific communication and WebSocket management.
    /// It uses the state manager (created in run.rs) to update state when receiving messages.
    ///
    /// # Arguments
    /// * `config` - Binance configuration
    /// * `state_manager` - Pre-configured state manager (created in run.rs)
    /// * `trading_client` - Trading client (same one used in state manager)
    /// * `channels` - Channel configuration for buffer sizes
    ///
    /// # Returns
    /// The driver instance
    pub fn new(
        config: super::config::BinanceConfig,
        state_manager: Arc<ExchangeStateManager>,
        trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>,
        channels: ChannelConfig,
    ) -> Result<Self, EngineError> {
        let client = BinanceClient::new(
            config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        );

        Ok(Self {
            client,
            state_manager,
            trading_client,
        })
    }

    /// Get the state manager
    pub fn state_manager(&self) -> &Arc<ExchangeStateManager> {
        &self.state_manager
    }

    /// Get a reference to the client for readiness checking
    pub fn client(&self) -> &BinanceClient {
        &self.client
    }

    /// Check if the WebSocket connection and RPC channel are ready
    pub async fn is_ready(&self) -> bool {
        // Check if connection session exists
        if self.client.get_connection_session().await.is_none() {
            return false;
        }
        // Check if RPC channel is ready
        let guard = self.client.outbound_rpc_tx.lock().await;
        guard.is_some()
    }

    /// Wait for the WebSocket connection and RPC channel to be ready
    /// Returns true if ready, false if timeout
    pub async fn wait_for_ready(&self, timeout_secs: u64) -> bool {
        let start = std::time::Instant::now();
        let timeout = tokio::time::Duration::from_secs(timeout_secs);

        loop {
            if self.is_ready().await {
                return true;
            }

            if start.elapsed() > timeout {
                return false;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}

#[async_trait]
impl ExchangeDriver for BinanceDriver {
    async fn run(&mut self, mut shutdown_rx: broadcast::Receiver<()>) -> Result<(), EngineError> {
        info!("Starting Binance exchange driver...");

        // Set state manager on client before starting stream
        self.client
            .set_state_manager(self.state_manager.clone())
            .await;

        // Start WebSocket stream in background
        let mut client = self.client.clone();
        let ws_handle = tokio::spawn(async move {
            match client.stream().await {
                Ok(_) => {
                    debug!("Binance WebSocket stream ended gracefully");
                }
                Err(e) => {
                    error!("Binance WebSocket stream error: {}", e);
                }
            }
        });

        // Wait for shutdown signal or WebSocket error
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, stopping Binance driver");
            }
            result = ws_handle => {
                if let Err(e) = result {
                    error!("Binance WebSocket task error: {}", e);
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "WebSocket task failed: {}", e
                    )));
                }
            }
        }

        info!("Binance exchange driver stopped");
        Ok(())
    }

    fn get_trading_client(&self) -> Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>> {
        self.trading_client.clone()
    }
}
