//! Bybit exchange driver
//!
//! This driver manages the Bybit WebSocket connection and event loop.
//! It integrates with the state manager to update and publish exchange state.

use crate::exchanges::bybit::{
    client::BybitClient,
    websocket::streaming_client::{BybitStreamingClient, StreamingEvent},
};
use crate::state::manager::ExchangeStateManager;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tracing::{debug, error, info};
use trading_core::traits::exchange_driver::ConnectionState;
use trading_core::traits::ExchangeDriver;
use trading_core::{error::EngineError, traits::TradingClient, types::exchange::OrderRequest};

/// Bybit exchange driver
///
/// Manages the Bybit WebSocket connection, event loop, and state management.
pub struct BybitDriver {
    client: BybitClient,
    state_manager: Arc<ExchangeStateManager>,
    trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>,
    connection_state: Arc<RwLock<ConnectionState>>,
    connection_ready_tx: Arc<RwLock<Option<oneshot::Sender<()>>>>,
}

impl BybitDriver {
    /// Create a new Bybit driver
    ///
    /// The driver handles protocol-specific communication and WebSocket management.
    /// It uses the state manager (created in run.rs) to update state when receiving messages.
    ///
    /// # Arguments
    /// * `client` - Bybit client instance
    /// * `state_manager` - Pre-configured state manager (created in run.rs)
    /// * `trading_client` - Trading client (same one used in state manager)
    ///
    /// # Returns
    /// The driver instance
    pub fn new(
        client: BybitClient,
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
        })
    }

    /// Get the state manager
    pub fn state_manager(&self) -> &Arc<ExchangeStateManager> {
        &self.state_manager
    }

    /// Get a reference to the client for readiness checking
    pub fn client(&self) -> &BybitClient {
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
}

#[async_trait]
impl ExchangeDriver for BybitDriver {
    async fn run(&mut self, mut shutdown_rx: broadcast::Receiver<()>) -> Result<(), EngineError> {
        info!("Starting Bybit exchange driver...");

        // Set state manager on client
        self.client
            .set_state_manager(self.state_manager.clone())
            .await;

        // Update state to Connecting
        self.set_connection_state(ConnectionState::Connecting).await;

        // Create streaming client
        let streaming_client = BybitStreamingClient::new(
            self.client.config.clone(),
            self.client.symbol_registry.clone(),
        );

        // Create channel for streaming events
        let (event_tx, mut event_rx) = mpsc::channel::<StreamingEvent>(1000);

        // Clone shutdown receiver for streaming client
        let shutdown_rx_streaming = shutdown_rx.resubscribe();

        // Start streaming client in background task
        let streaming_handle = tokio::spawn(async move {
            if let Err(e) = streaming_client.run(event_tx, shutdown_rx_streaming).await {
                error!("Streaming client error: {}", e);
            }
        });

        // Start event processing task
        let state_manager_events = self.state_manager.clone();
        let connection_state_events = self.connection_state.clone();
        let event_handle = tokio::spawn(async move {
            let mut connected = false;
            while let Some(event) = event_rx.recv().await {
                match event {
                    StreamingEvent::Position(_pos) => {
                        debug!(
                            "Received position update (positions not yet handled by state manager)"
                        );
                        // Positions are not yet handled by ExchangeStateManager
                    }
                    StreamingEvent::Execution(trade) => {
                        debug!("Received execution update: {:?}", trade);
                        if let Err(e) = state_manager_events.add_trade(trade).await {
                            error!("Failed to add trade: {}", e);
                        }
                    }
                    StreamingEvent::Order(order) => {
                        debug!("Received order update: {:?}", order);
                        if let Err(e) = state_manager_events.update_order(order).await {
                            error!("Failed to update order: {}", e);
                        }
                    }
                    StreamingEvent::Wallet(balances) => {
                        debug!("Received wallet update: {} balances", balances.len());
                        for balance in balances {
                            if let Err(e) = state_manager_events.update_balance(balance).await {
                                error!("Failed to update balance: {}", e);
                            }
                        }
                    }
                }

                // Mark as connected after first event
                if !connected {
                    connected = true;
                    let mut guard = connection_state_events.write().await;
                    *guard = ConnectionState::Connected;
                    drop(guard);
                }
            }
        });

        // Update connection state to Connected after a short delay
        // (streaming client will authenticate and start sending events)
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        self.set_connection_state(ConnectionState::Connected).await;

        info!("Bybit driver started, waiting for shutdown signal");

        // Wait for shutdown signal or task errors
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, stopping Bybit driver");
                self.set_connection_state(ConnectionState::Disconnected).await;
            }
            result = streaming_handle => {
                if let Err(e) = result {
                    error!("Streaming client task error: {}", e);
                    self.set_connection_state(ConnectionState::Failed).await;
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Streaming client task failed: {}", e
                    )));
                }
            }
            result = event_handle => {
                if let Err(e) = result {
                    error!("Event processing task error: {}", e);
                    self.set_connection_state(ConnectionState::Failed).await;
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Event processing task failed: {}", e
                    )));
                }
            }
        }

        info!("Bybit exchange driver stopped");
        Ok(())
    }

    async fn is_connection_ready(&self) -> bool {
        *self.connection_state.read().await == ConnectionState::Connected
    }

    async fn get_connection_state(&self) -> ConnectionState {
        *self.connection_state.read().await
    }

    fn get_trading_client(&self) -> Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>> {
        self.trading_client.clone()
    }
}
