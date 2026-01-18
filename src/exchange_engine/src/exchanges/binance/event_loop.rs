//! Binance event loop
//!
//! Coordinates WebSocket RPC client, streaming client, and connection management.
//! This module handles the main event loop for Binance WebSocket connections.

use crate::exchanges::binance::config::BinanceConfig;
use crate::exchanges::binance::websocket::mapper::BinanceMessageMapper;
use crate::exchanges::binance::websocket::{
    rpc_client::BinanceRpcClient, streaming_client::BinanceStreamingClient,
    types::rpc::BinanceRpcRequest, types::user_data::events::ParsedBinanceMessage,
};
use crate::state::manager::ExchangeStateManager;
use std::sync::Arc;
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    time::{sleep, Duration},
};
use tracing::{debug, info, warn};
use trading_core::error::EngineError;

/// Binance event loop coordinator
///
/// Manages the WebSocket connection, coordinating RPC client and streaming client.
pub struct BinanceEventLoop {
    config: BinanceConfig,
    rpc_client: Arc<BinanceRpcClient>,
    streaming_client: Arc<BinanceStreamingClient>,
    state_manager: Arc<RwLock<Option<Arc<ExchangeStateManager>>>>,
    inbound_capacity: usize,
    ping_capacity: usize,
    outbound_rpc_capacity: usize,
}

impl BinanceEventLoop {
    /// Create a new Binance event loop
    pub fn new(
        config: BinanceConfig,
        rpc_client: Arc<BinanceRpcClient>,
        streaming_client: Arc<BinanceStreamingClient>,
        state_manager: Arc<RwLock<Option<Arc<ExchangeStateManager>>>>,
        inbound_capacity: usize,
        ping_capacity: usize,
        outbound_rpc_capacity: usize,
    ) -> Self {
        Self {
            config,
            rpc_client,
            streaming_client,
            state_manager,
            inbound_capacity,
            ping_capacity,
            outbound_rpc_capacity,
        }
    }

    /// Set the exchange state manager
    pub async fn set_state_manager(&self, state_manager: Arc<ExchangeStateManager>) {
        *self.state_manager.write().await = Some(state_manager.clone());
        // Set on streaming client
        self.streaming_client
            .set_state_manager(state_manager.clone())
            .await;
    }

    /// Run the event loop
    ///
    /// This method manages the WebSocket connection lifecycle, coordinating
    /// RPC client and streaming client operations.
    pub async fn run(&mut self) -> Result<(), EngineError> {
        info!("[BINANCE EVENT LOOP] Starting event loop");

        let mut backoff_iter = 0u32;
        let mut connection_id = 0u64;

        loop {
            connection_id += 1;
            debug!(
                "[BINANCE EVENT LOOP {connection_id}] Connection attempt {connection_id} (backoff_iter={backoff_iter})"
            );

            match self.connect_and_run_unified().await {
                Ok(_) => {
                    debug!(
                        "[BINANCE EVENT LOOP {connection_id}] Connection ended, reconnecting..."
                    );
                }
                Err(e) => {
                    warn!(
                        "[BINANCE EVENT LOOP {connection_id}] Connection error: {e:?}, reconnecting..."
                    );
                }
            }

            // Reset RPC channel on disconnect
            {
                let mut guard = self.rpc_client.rpc_request_tx.lock().await;
                if guard.is_some() {
                    debug!(
                        "[BINANCE EVENT LOOP {connection_id}] Resetting RPC channel (connection dropped)"
                    );
                }
                *guard = None;
            }

            // Exponential backoff
            self.exponential_backoff_wait(backoff_iter).await;
            backoff_iter = backoff_iter.saturating_add(1);
        }
    }

    async fn connect_and_run_unified(&mut self) -> Result<(), EngineError> {
        debug!("[BINANCE EVENT LOOP] Starting unified connection");

        // Create channels for unified connection
        let (rpc_request_tx, rpc_request_rx) =
            mpsc::channel::<BinanceRpcRequest>(self.outbound_rpc_capacity);
        let (rpc_response_tx, rpc_response_rx) =
            mpsc::channel::<BinanceRpcResponse>(self.outbound_rpc_capacity);
        let (event_tx, event_rx) = mpsc::channel::<ParsedBinanceMessage>(self.inbound_capacity);
        let (ping_tx, ping_rx) = mpsc::channel::<Vec<u8>>(self.ping_capacity);

        // Connect and authenticate ONCE - use streaming client's ws_client
        debug!("[BINANCE EVENT LOOP] Connecting and authenticating");
        let mut ws = self
            .streaming_client
            .ws_client
            .connect_and_authenticate()
            .await?;
        info!("[BINANCE EVENT LOOP] Connected and authenticated");

        // Subscribe to user stream
        debug!("[BINANCE EVENT LOOP] Subscribing to userDataStream");
        self.streaming_client
            .ws_client
            .subscribe_to_user_stream(&mut ws)
            .await?;
        info!("[BINANCE EVENT LOOP] Subscribed to userDataStream");

        // Now split the stream and handle both RPC and events
        debug!("[BINANCE EVENT LOOP] Splitting stream for unified handling");
        let mut session = self
            .streaming_client
            .ws_client
            .split_stream(
                ws,
                rpc_request_tx.clone(),
                rpc_request_rx,
                rpc_response_tx.clone(),
                event_tx.clone(),
                ping_tx,
                ping_rx,
                BinanceMessageMapper,
            )
            .await?;
        debug!("[BINANCE EVENT LOOP] Stream split successful - writer task spawned");

        // Wait for writer task to be ready (proper signaling instead of arbitrary sleep)
        if let Some(writer_ready_rx) = session.writer_ready_rx.take() {
            match tokio::time::timeout(tokio::time::Duration::from_secs(5), writer_ready_rx).await {
                Ok(Ok(_)) => {
                    debug!("[BINANCE EVENT LOOP] Writer task is ready");
                }
                Ok(Err(_)) => {
                    warn!("[BINANCE EVENT LOOP] Writer ready signal channel closed");
                }
                Err(_) => {
                    warn!("[BINANCE EVENT LOOP] Timeout waiting for writer ready signal (5s)");
                }
            }
        }

        // Mark RPC channel as ready (writer task is confirmed running)
        {
            let mut guard = self.rpc_client.rpc_request_tx.lock().await;
            *guard = Some(rpc_request_tx.clone());
            debug!("[BINANCE EVENT LOOP] RPC channel marked as ready");
        }

        // Spawn event handler
        // Convert RwLock to Mutex for compatibility with run_event_handler
        let state_manager_clone = self.state_manager.clone();
        let event_handler_handle = tokio::spawn(async move {
            // Convert RwLock to Mutex by reading the value and wrapping in Mutex
            let state_mgr_arc = Arc::new(Mutex::new({
                let guard = state_manager_clone.read().await;
                (*guard).clone()
            }));
            BinanceStreamingClient::run_event_handler(event_rx, state_mgr_arc).await;
        });

        // Spawn RPC response handler
        let rpc_client_clone = Arc::clone(&self.rpc_client);
        let rpc_handler_handle = tokio::spawn(async move {
            if let Err(e) = rpc_client_clone
                .run_rpc_response_handler(rpc_response_rx)
                .await
            {
                warn!("[BINANCE EVENT LOOP] RPC response handler error: {e:?}");
            }
        });

        // Extract handles from session
        let mut reader_handle = session.reader_handle.take().expect("Reader handle missing");
        let mut writer_handle = session.writer_handle.take().expect("Writer handle missing");

        info!("[BINANCE EVENT LOOP] Connection established and handlers running");

        // Wait for reader or writer to exit (connection dropped)
        tokio::select! {
            result = &mut reader_handle => {
                warn!("[BINANCE EVENT LOOP] Reader task exited: {result:?}");
            }
            result = &mut writer_handle => {
                warn!("[BINANCE EVENT LOOP] Writer task exited: {result:?}");
            }
            _ = event_handler_handle => {
                warn!("[BINANCE EVENT LOOP] Event handler task exited");
            }
            _ = rpc_handler_handle => {
                warn!("[BINANCE EVENT LOOP] RPC handler task exited");
            }
        }

        debug!("[BINANCE EVENT LOOP] Connection dropped, exiting unified connection");
        // Channel will be reset by the caller (run() loop)
        Ok(())
    }

    async fn exponential_backoff_wait(&self, iter: u32) {
        let base = Duration::from_secs(self.config.base_delay.unwrap_or(2));
        let max = Duration::from_secs(self.config.max_delay.unwrap_or(60));
        let delay = (base * 2u32.saturating_pow(iter)).min(max);
        // Simple jitter using system time
        let jitter_ms = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            % 1000) as u64;
        let jitter = Duration::from_millis(jitter_ms);
        let to_sleep = (delay + jitter).min(max);
        debug!(
            "[BINANCE EVENT LOOP] Waiting {:?} before reconnecting",
            to_sleep
        );
        sleep(to_sleep).await;
    }
}

use crate::exchanges::binance::websocket::types::rpc::BinanceRpcResponse;
