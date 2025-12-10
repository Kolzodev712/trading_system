//! Binance public REST client for fetching exchange information (unsigned calls)
use super::types::BinanceExchangeInfo;
use reqwest::Client;
use tracing::{error, info};
use trading_core::{
    error::EngineError,
    types::asset::{RateLimit, RateLimitType},
};

/// Binance public REST client (no authentication required)
#[derive(Debug)]
pub struct BinancePublicClient {
    base_url: String,
    http_client: Client,
}

impl BinancePublicClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http_client: Client::new(),
        }
    }

    /// Fetch exchange info (public endpoint, no auth)
    pub async fn fetch_exchange_info(&self) -> Result<BinanceExchangeInfo, EngineError> {
        let url = format!("{}/api/v3/exchangeInfo", self.base_url);
        info!("Fetching exchange info from {}", url);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("HTTP error: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("Exchange info request failed: {} - {}", status, text);
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Exchange info request failed: {} - {}",
                status,
                text
            )));
        }

        let exchange_info: BinanceExchangeInfo = response
            .json()
            .await
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("JSON parse error: {}", e)))?;

        info!(
            "Fetched exchange info: {} symbols, {} rate limits",
            exchange_info.symbols.len(),
            exchange_info.rate_limits.len()
        );

        Ok(exchange_info)
    }

    /// Convert Binance rate limits to common format
    pub fn parse_rate_limits(
        binance_limits: &[crate::exchanges::binance::types::BinanceRateLimit],
    ) -> Vec<RateLimit> {
        binance_limits
            .iter()
            .map(|limit| RateLimit {
                rate_limit_type: match limit.rate_limit_type.as_str() {
                    "REQUEST_WEIGHT" => RateLimitType::RequestWeight,
                    "ORDERS" => RateLimitType::Orders,
                    "RAW_REQUESTS" => RateLimitType::RawRequests,
                    _ => RateLimitType::RawRequests, // Default fallback
                },
                interval: limit.interval.clone(),
                limit: limit.limit,
            })
            .collect()
    }
}
