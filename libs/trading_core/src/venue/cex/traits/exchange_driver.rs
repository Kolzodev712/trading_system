//! Exchange driver trait
//!
//! Defines the interface that all exchange implementations must provide
//! for running their event loops and managing WebSocket connections.

use crate::error::EngineError;
use crate::venue::cex::traits::TradingClient;
use crate::venue::cex::types::exchange::OrderRequest;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Connection state for exchange drivers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state, not yet connected
    Disconnected,
    /// Currently establishing connection
    Connecting,
    /// Connected and ready
    Connected,
    /// Connection lost, attempting to reconnect
    Reconnecting,
    /// Connection failed permanently
    Failed,
}

/// Trait for exchange-specific drivers
///
/// Each exchange implementation provides a driver that:
/// - Manages WebSocket connections
/// - Handles exchange-specific event loops
/// - Handles protocol-specific communication
/// - Handles graceful shutdown
#[async_trait]
pub trait ExchangeDriver: Send + Sync + 'static {
    /// Start the exchange driver's event loop
    ///
    /// This should:
    /// - Establish WebSocket connections
    /// - Handle incoming messages
    /// - Run until shutdown signal is received
    ///
    /// # Arguments
    /// * `shutdown_rx` - Receiver for shutdown signals
    ///
    /// # Returns
    /// Ok(()) on graceful shutdown, Error on failure
    async fn run(&mut self, mut shutdown_rx: broadcast::Receiver<()>) -> Result<(), EngineError>;

    /// Get the trading client for this exchange
    ///
    /// Returns a boxed trait object that can be used for trading operations
    fn get_trading_client(&self) -> Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>;

    /// Check if the connection is ready for RPC operations
    ///
    /// This should check if the WebSocket connection is established
    /// and the RPC channel is available and open.
    ///
    /// # Returns
    /// `true` if ready, `false` otherwise
    async fn is_connection_ready(&self) -> bool;

    /// Get the current connection state
    ///
    /// # Returns
    /// Current connection state
    async fn get_connection_state(&self) -> ConnectionState;

    /// Check if session needs to be refreshed
    ///
    /// Some exchanges use session-based authentication that may expire.
    /// This method allows the driver to indicate when a session refresh is needed.
    ///
    /// # Returns
    /// `true` if session refresh is needed, `false` otherwise
    async fn needs_session_refresh(&self) -> bool {
        false
    }

    /// Refresh the session if needed
    ///
    /// This method is called when `needs_session_refresh()` returns `true`.
    /// It should re-authenticate and establish a new session.
    ///
    /// # Returns
    /// Ok(()) on success, Error on failure
    async fn refresh_session(&mut self) -> Result<(), EngineError> {
        // Default implementation does nothing
        Ok(())
    }

    /// Get a signal that fires when connection becomes ready
    ///
    /// This allows the generic event loop to wait for connection readiness
    /// without polling. Returns `None` if the driver doesn't support this.
    ///
    /// # Returns
    /// `Some(oneshot::Receiver)` that will receive a signal when ready, or `None`
    fn connection_ready_signal(&self) -> Option<tokio::sync::oneshot::Receiver<()>> {
        None
    }
}
