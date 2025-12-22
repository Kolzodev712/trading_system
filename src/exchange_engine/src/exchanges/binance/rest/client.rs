use super::super::{config::BinanceConfig, utils::deserialize_or_none};
use crate::exchanges::binance::{
    rate_limit::{BinanceRateLimitType, BinanceRateLimiter},
    sign::BinanceAuthenticator,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smol_str::SmolStr;
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    sync::Arc,
};
use tracing::{debug, warn};
use trading_core::{
    authenticator::ExchangeAuthenticator,
    error::EngineError,
    rest::{HttpMethod, RestExchangeHandler},
};

const BINANCE_SPOT_URL: &str = "https://api.binance.com/api/v3/";

#[derive(Debug, Clone)]
pub struct BinanceHttpClient {
    pub base_url: SmolStr,
    pub handler: BinanceHandler,
    pub authenticator: BinanceAuthenticator,
    pub client: Client,
    pub rate_limiter: Arc<BinanceRateLimiter>,
}

impl BinanceHttpClient {
    pub fn new(config: BinanceConfig) -> Self {
        let rate_limiter = Arc::new(BinanceRateLimiter::new());
        // Initialize with default limits
        let limiter_clone = rate_limiter.clone();
        tokio::spawn(async move {
            limiter_clone.initialize_defaults().await;
        });

        BinanceHttpClient {
            base_url: BINANCE_SPOT_URL.into(),
            handler: BinanceHandler,
            authenticator: BinanceAuthenticator {
                api_key: config.rest.api_key,
                secret_key: config.rest.secret_key,
            },
            client: Client::new(),
            rate_limiter,
        }
    }

    pub async fn call<Req, Res>(
        &self,
        endpoint: &str,
        method: HttpMethod,
        params: Option<Req>,
        _body: Option<Req>,
        custom_headers: Option<HashMap<String, String>>,
        signed: bool,
    ) -> Result<Option<Res>, EngineError>
    where
        Req: Serialize + Sized + Debug + Sync,
        Res: for<'de> Deserialize<'de> + Debug + Send + Default,
    {
        let url = self.handler.build_url(&self.base_url, endpoint).await;
        let mut request_builder = self.client.request(method.into(), url.as_str());

        if signed {
            if let Some(headers) = self.authenticator.get_headers()? {
                request_builder = headers.iter().fold(request_builder, |builder, (k, v)| {
                    builder.header(k.as_str(), v.as_str())
                });
            }
        }

        if let Some(headers) = custom_headers {
            for (key, value) in headers {
                request_builder = request_builder.header(key, value);
            }
        }

        let mut query_params: BTreeMap<String, String> = if let Some(params) = params {
            serde_qs::from_str::<BTreeMap<String, String>>(
                &serde_qs::to_string(&params).map_err(EngineError::JsonQueryStringError)?,
            )?
        } else {
            debug!("Passing empty params list. Probaly request has no params. FYI");
            BTreeMap::new()
        };

        if signed {
            let mut smol_query_params: BTreeMap<SmolStr, SmolStr> = query_params
                .into_iter()
                .map(|(k, v)| (SmolStr::new(k), SmolStr::new(v)))
                .collect();

            let signed_params = self
                .authenticator
                .build_signed_params(smol_query_params.clone())?;

            smol_query_params.extend(signed_params);

            query_params = smol_query_params
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
        }

        if !query_params.is_empty() {
            request_builder = request_builder.query(&query_params);
        }

        // Check rate limits before making request
        // REST API uses REQUEST_WEIGHT (default weight is 1, but can vary by endpoint)
        let request_weight = self.get_request_weight(endpoint);
        self.rate_limiter
            .wait_if_needed(BinanceRateLimitType::RequestWeight, request_weight)
            .await;

        let response = request_builder.send().await?;
        let status = response.status();

        // Extract headers before consuming response body
        let used_weight_header = response
            .headers()
            .get("x-mbx-used-weight-1m")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<i64>().ok());

        let text = response.text().await?;

        // Handle 429 (Too Many Requests) with retryAfter
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            // Try to extract retryAfter from response for logging
            if let Ok(json) = serde_json::from_str::<Value>(&text) {
                if let Some(retry_after) =
                    json.get("retryAfter").and_then(|v| v.as_u64()).or_else(|| {
                        json.get("error")
                            .and_then(|e| e.get("data"))
                            .and_then(|d| d.get("retryAfter"))
                            .and_then(|v| v.as_u64())
                    })
                {
                    warn!(
                        "Rate limited (429) - Request to {} failed. Retry after {}ms. Body: {}",
                        endpoint, retry_after, text
                    );
                } else {
                    warn!(
                        "Rate limited (429) - Request to {} failed. Body: {}",
                        endpoint, text
                    );
                }
            }

            return Err(EngineError::HttpCustomError(SmolStr::new(format!(
                "Rate limited (429) - Request to {} failed with body: {}",
                endpoint, text
            ))));
        }

        if !status.is_success() {
            return Err(EngineError::HttpCustomError(SmolStr::new(format!(
                "Request failed with status {} and body: {}",
                status, text
            ))));
        }

        // Record the request after successful response
        self.rate_limiter
            .record(BinanceRateLimitType::RequestWeight, request_weight)
            .await;

        // Log rate limit usage from headers if available
        if let Some(used_weight) = used_weight_header {
            // Update rate limiter with current usage (limit is typically 6000)
            // We don't know the exact limit from headers, so we keep using defaults
            debug!("Current REQUEST_WEIGHT usage: {}", used_weight);
        }

        let mapped_response = deserialize_or_none::<Res>(&text)?;

        Ok(mapped_response)
    }

    /// Get request weight for an endpoint
    /// Default is 1, but some endpoints have different weights
    fn get_request_weight(&self, endpoint: &str) -> u64 {
        // Binance REST API weights (from documentation)
        match endpoint {
            "exchangeInfo" => 10,
            "klines" => 1,
            "ticker/24hr" => 40, // per symbol, or 1 if no symbol
            "ticker/price" => 1,
            "depth" => {
                // Weight depends on limit parameter
                // Default is 1, but can be up to 50 for limit=5000
                1 // We'll use 1 as default, can be enhanced to parse params
            }
            _ => 1, // Default weight
        }
    }
}

#[derive(Clone, Debug)]
pub struct BinanceHandler;

#[async_trait]
impl RestExchangeHandler for BinanceHandler {
    async fn build_url(&self, base_url: &str, endpoint: &str) -> SmolStr {
        let full_url = format!("{}{}", base_url, endpoint);
        full_url.into()
    }

    async fn handle_response(
        &self,
        rest_url: &str,
        status: reqwest::StatusCode,
        text: &str,
    ) -> Result<(), EngineError> {
        if !status.is_success() {
            Err(EngineError::HttpCustomError(SmolStr::new(format!(
                "Request to {} failed with status {} and body: {}",
                rest_url, status, text
            ))))
        } else {
            Ok(())
        }
    }

    async fn get_params(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        let params = HashMap::new();
        Ok(Some(params))
    }

    fn get_body(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        // Binance typically doesn't require a body for most requests
        Ok(None)
    }
}
