//! Hooks for authenticated WebSocket streams
use crate::error::EngineError;
use async_trait::async_trait;
use smol_str::SmolStr;

/// Trait for adding authentication to WebSocket connections
#[async_trait]
pub trait AuthenticatedWebSocketHook: Send + Sync {
    /// Adds authentication to the WebSocket connection
    async fn add_auth(&self, ws_url: &str) -> Result<String, EngineError>;

    /// Gets the API key (if needed)
    fn api_key(&self) -> Option<SmolStr>;

    /// Gets the API secret (if needed)
    fn api_secret(&self) -> Option<SmolStr>;
}
