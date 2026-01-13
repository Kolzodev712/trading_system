//! Binance exchange driver
//!
//! This driver manages the Binance WebSocket connection and event loop.
//! It integrates with the state manager to update and publish exchange state.

use super::websocket::ws_client::BinanceClient;
use crate::state::manager::ExchangeStateManager;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, oneshot, RwLock};
use tracing::{debug, info, warn};
use trading_core::traits::exchange_driver::ConnectionState;
use trading_core::traits::ExchangeDriver;
use trading_core::{error::EngineError, traits::TradingClient, types::exchange::OrderRequest};

/// Binance exchange driver
///
/// Manages the Binance WebSocket connection, event loop, and state management.
pub struct BinanceDriver {
    client: BinanceClient,
    state_manager: Arc<ExchangeStateManager>,
    trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>,
    connection_state: Arc<RwLock<ConnectionState>>,
    connection_ready_tx: Arc<RwLock<Option<oneshot::Sender<()>>>>,
    last_session_refresh: Arc<RwLock<Option<Instant>>>,
}

impl BinanceDriver {
    /// Create a new Binance driver
    ///
    /// The driver handles protocol-specific communication and WebSocket management.
    /// It uses the state manager (created in run.rs) to update state when receiving messages.
    ///
    /// # Arguments
    /// * `client` - Binance client instance
    /// * `state_manager` - Pre-configured state manager (created in run.rs)
    /// * `trading_client` - Trading client (same one used in state manager)
    ///
    /// # Returns
    /// The driver instance
    pub fn new(
        client: BinanceClient,
        state_manager: Arc<ExchangeStateManager>,
        trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>,
    ) -> Result<Self, EngineError> {
        let (tx, _rx) = oneshot::channel();
        Ok(Self {
            client,
            state_manager,
            trading_client,
            connection_state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            connection_ready_tx: Arc::new(RwLock::new(Some(tx))),
            last_session_refresh: Arc::new(RwLock::new(None)),
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

    /// Set connection state
    async fn set_connection_state(&self, state: ConnectionState) {
        let mut guard = self.connection_state.write().await;
        let old_state = *guard;
        *guard = state;
        drop(guard);

        // Signal ready when transitioning to Connected
        if state == ConnectionState::Connected && old_state != ConnectionState::Connected {
            if let Some(tx) = self.connection_ready_tx.write().await.take() {
                let _ = tx.send(());
            }
        }
    }

    /// Check if session needs refresh (Binance sessions typically last 24 hours)
    async fn check_session_refresh_needed(&self) -> bool {
        // Binance sessions don't have a fixed expiration, but we can refresh periodically
        // Check if it's been more than 12 hours since last refresh
        if let Some(last_refresh) = *self.last_session_refresh.read().await {
            last_refresh.elapsed().as_secs() > 12 * 3600
        } else {
            // Never refreshed, check if connection has been up for a while
            // For now, always return false - session refresh is handled by reconnection
            false
        }
    }
}

#[async_trait]
impl ExchangeDriver for BinanceDriver {
    async fn run(&mut self, mut shutdown_rx: broadcast::Receiver<()>) -> Result<(), EngineError> {
        info!("Starting Binance exchange driver...");

        // Create event loop for coordinating RPC and streaming clients
        let state_manager_for_event_loop =
            Arc::new(tokio::sync::RwLock::new(Some(self.state_manager.clone())));
        let mut event_loop = self.client.create_event_loop(state_manager_for_event_loop);

        // Set state manager on client (for TradingClient methods)
        self.client
            .set_state_manager(self.state_manager.clone())
            .await;

        // Set state manager on event loop
        event_loop
            .set_state_manager(self.state_manager.clone())
            .await;

        // Update state to Connecting
        self.set_connection_state(ConnectionState::Connecting).await;

        // Start event loop in background
        let state_tracker = self.connection_state.clone();
        let _ready_tx = Arc::clone(&self.connection_ready_tx);

        let ws_handle = tokio::spawn(async move {
            // Track connection state in the event loop
            let mut state_guard = state_tracker.write().await;
            *state_guard = ConnectionState::Connecting;
            drop(state_guard);

            match event_loop.run().await {
                Ok(_) => {
                    debug!("Binance event loop ended gracefully");
                    let mut state_guard = state_tracker.write().await;
                    *state_guard = ConnectionState::Disconnected;
                }
                Err(e) => {
                    warn!("Binance event loop error: {}", e);
                    let mut state_guard = state_tracker.write().await;
                    *state_guard = ConnectionState::Failed;
                }
            }
        });

        // Don't block on connection readiness - let it happen asynchronously
        // The connection will signal when ready via set_connection_state
        info!("Binance driver started, connection will be ready when WebSocket is established");

        // Wait for shutdown signal or WebSocket error
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, stopping Binance driver");
                self.set_connection_state(ConnectionState::Disconnected).await;
            }
            result = ws_handle => {
                if let Err(e) = result {
                    warn!("Binance WebSocket task error: {}", e);
                    self.set_connection_state(ConnectionState::Failed).await;
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

    async fn is_connection_ready(&self) -> bool {
        // Check if RPC channel is ready AND open
        let rpc_tx = self.client.rpc_client.get_rpc_request_tx().await;
        if let Some(tx) = rpc_tx {
            !tx.is_closed()
        } else {
            false
        }
    }

    async fn get_connection_state(&self) -> ConnectionState {
        *self.connection_state.read().await
    }

    async fn needs_session_refresh(&self) -> bool {
        self.check_session_refresh_needed().await
    }

    async fn refresh_session(&mut self) -> Result<(), EngineError> {
        // Binance session refresh is handled by reconnection
        // This is a no-op for now, but could trigger a reconnection if needed
        *self.last_session_refresh.write().await = Some(Instant::now());
        Ok(())
    }

    fn connection_ready_signal(&self) -> Option<tokio::sync::oneshot::Receiver<()>> {
        // Return None - we'll use polling instead
        // The ready signal is sent when connection becomes ready
        None
    }
}
