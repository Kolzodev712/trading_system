//! Bybit REST API client
//!
//! Unified client for both Spot and Futures trading.
//! Routes requests based on InstrumentType in OrderRequest.

use super::super::config::BybitConfig;
use async_trait::async_trait;
use reqwest::Client;
use smol_str::SmolStr;
use std::collections::HashMap;
use tracing::warn;
use trading_core::{error::EngineError, rest::RestExchangeHandler};

/// Base URL for Bybit REST API
const BYBIT_REST_URL: &str = "https://api.bybit.com/v5/";
const BYBIT_TESTNET_URL: &str = "https://api-testnet.bybit.com/v5/";

/// Bybit REST API handler
#[derive(Debug, Clone)]
pub struct BybitHandler;

#[async_trait]
impl RestExchangeHandler for BybitHandler {
    async fn build_url(&self, base_url: &str, endpoint: &str) -> SmolStr {
        let full_url = format!("{}{}", base_url, endpoint);
        full_url.into()
    }

    async fn handle_response(
        &self,
        _rest_url: &str,
        _status: reqwest::StatusCode,
        _text: &str,
    ) -> Result<(), EngineError> {
        // TODO: Implement Bybit-specific response handling
        // Bybit wraps responses in a result object, need to handle retCode/retMsg
        Ok(())
    }

    async fn get_params(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        // TODO: Implement Bybit-specific parameter extraction
        Ok(None)
    }

    fn get_body(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        // TODO: Implement Bybit-specific body serialization
        Ok(None)
    }
}

/// Bybit HTTP client for REST API calls
#[derive(Debug, Clone)]
pub struct BybitHttpClient {
    pub base_url: SmolStr,
    pub handler: BybitHandler,
    pub client: Client,
    // TODO: Add authenticator when implementing
    // pub authenticator: BybitAuthenticator,
    // TODO: Add rate limiter when implementing
    // pub rate_limiter: Arc<BybitRateLimiter>,
}

impl BybitHttpClient {
    pub fn new(config: BybitConfig) -> Self {
        let base_url = if config.testnet {
            BYBIT_TESTNET_URL.into()
        } else {
            BYBIT_REST_URL.into()
        };

        BybitHttpClient {
            base_url,
            handler: BybitHandler,
            client: Client::new(),
            // TODO: Initialize authenticator and rate limiter
        }
    }

    /// Make a REST API call
    ///
    /// TODO: Implement actual API call logic
    pub async fn call<Req, Res>(
        &self,
        _endpoint: &str,
        _method: reqwest::Method,
        _params: Option<Req>,
        _body: Option<Req>,
        _custom_headers: Option<HashMap<String, String>>,
        _signed: bool,
    ) -> Result<Option<Res>, EngineError>
    where
        Req: serde::Serialize + std::fmt::Debug + Sync,
        Res: for<'de> serde::Deserialize<'de> + std::fmt::Debug + Send + Default,
    {
        warn!("Bybit REST API calls not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit REST API not yet implemented".into(),
        ))
    }
}
