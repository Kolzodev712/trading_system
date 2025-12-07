//! Hooks for authenticated REST requests
use crate::error::EngineError;
use async_trait::async_trait;
use reqwest::RequestBuilder;
use smol_str::SmolStr;

/// Trait for adding authentication to REST requests
#[async_trait]
pub trait AuthenticatedRestHook: Send + Sync {
    /// Adds authentication headers/signature to the request
    async fn add_auth(&self, request: RequestBuilder) -> Result<RequestBuilder, EngineError>;

    /// Gets the API key (if needed)
    fn api_key(&self) -> Option<SmolStr>;

    /// Gets the API secret (if needed)
    fn api_secret(&self) -> Option<SmolStr>;
}
