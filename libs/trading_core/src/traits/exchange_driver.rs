//! Exchange driver trait
//!
//! Defines the interface that all exchange implementations must provide
//! for running their event loops and managing WebSocket connections.

use crate::error::EngineError;
use crate::traits::TradingClient;
use crate::types::exchange::OrderRequest;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Trait for exchange-specific drivers
///
/// Each exchange implementation provides a driver that:
/// - Manages WebSocket connections
/// - Handles exchange-specific event loops
/// - Handles protocol-specific communication
/// - Handles graceful shutdown
#[async_trait]
pub trait ExchangeDriver: Send + Sync {
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
}
