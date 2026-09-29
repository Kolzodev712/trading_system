//! Bybit REST API client
//!
//! Unified client for both Spot and Futures trading.
//! Routes requests based on InstrumentType in OrderRequest.
//! See: <https://bybit-exchange.github.io/docs/v5/guide>

use crate::exchanges::bybit::authenticator::BybitAuthenticator;
use crate::exchanges::bybit::{
    config::BybitConfig, error::BybitError, rate_limit::BybitRateLimiter,
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
use trading_core::{
    error::EngineError,
    rest::{HttpMethod, RestExchangeHandler},
};

/// Bybit REST API handler
#[derive(Debug, Clone)]
pub struct BybitHandler;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BybitResponseWrapper {
    #[serde(default)]
    ret_code: i32,
    #[serde(default)]
    ret_msg: String,
    result: Option<Value>,
    #[serde(default)]
    _ret_ext_info: Option<Value>,
    #[serde(default)]
    _time: Option<u64>,
}

#[async_trait]
impl RestExchangeHandler for BybitHandler {
    async fn build_url(&self, base_url: &str, endpoint: &str) -> SmolStr {
        let full_url = format!("{base_url}{endpoint}");
        full_url.into()
    }

    async fn handle_response(
        &self,
        rest_url: &str,
        status: reqwest::StatusCode,
        text: &str,
    ) -> Result<(), EngineError> {
        Self::parse_and_validate_response(rest_url, status, text).await?;
        Ok(())
    }

    async fn get_params(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        Ok(None)
    }

    fn get_body(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        Ok(None)
    }
}

impl BybitHandler {
    fn prepare_params<Req: Serialize>(
        params: Option<Req>,
    ) -> Result<BTreeMap<String, String>, EngineError> {
        if let Some(params) = params {
            serde_qs::from_str::<BTreeMap<String, String>>(
                &serde_qs::to_string(&params).map_err(EngineError::JsonQueryStringError)?,
            )
            .map_err(EngineError::JsonQueryStringError)
        } else {
            Ok(BTreeMap::new())
        }
    }

    fn prepare_body<Req: Serialize>(body: Option<Req>) -> Result<Option<String>, EngineError> {
        if let Some(body) = body {
            serde_json::to_string(&body)
                .map_err(EngineError::JsonError)
                .map(Some)
        } else {
            Ok(None)
        }
    }

    fn serialize_query_string(
        query_params: &BTreeMap<String, String>,
    ) -> Result<String, EngineError> {
        if !query_params.is_empty() {
            serde_urlencoded::to_string(query_params).map_err(EngineError::JsonUrlEncodedSerError)
        } else {
            Ok(String::new())
        }
    }

    async fn parse_and_validate_response(
        rest_url: &str,
        status: reqwest::StatusCode,
        text: &str,
    ) -> Result<BybitResponseWrapper, EngineError> {
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(BybitError::TooManyRequests.into());
        }

        if !status.is_success() {
            if let Some(bybit_error) = BybitError::from_http_status(status.as_u16()) {
                return Err(bybit_error.into());
            }
            return Err(EngineError::HttpCustomError(SmolStr::new(format!(
                "Request to {rest_url} failed with status {status} and body: {text}",
            ))));
        }

        // Try to parse as Bybit response wrapper
        // If parsing fails, log the response for debugging
        let wrapper: BybitResponseWrapper = match serde_json::from_str(text) {
            Ok(w) => w,
            Err(e) => {
                // Log the actual response for debugging
                tracing::error!(
                    "Failed to parse Bybit response from {}: {}. Response body: {}",
                    rest_url,
                    e,
                    text
                );
                return Err(EngineError::JsonError(e));
            }
        };

        // Check if ret_code indicates an error (defaults to 0 if missing)
        if wrapper.ret_code != 0 {
            tracing::error!(
                "Bybit API error from {}: retCode={}, retMsg={}, full response: {}",
                rest_url,
                wrapper.ret_code,
                wrapper.ret_msg,
                text
            );
            let bybit_error = BybitError::from_ret_code(
                wrapper.ret_code,
                if wrapper.ret_msg.is_empty() {
                    "Unknown error"
                } else {
                    &wrapper.ret_msg
                },
            );
            return Err(bybit_error.into());
        }

        Ok(wrapper)
    }
}

/// Bybit HTTP client for REST API calls
#[derive(Debug, Clone)]
pub struct BybitHttpClient {
    pub base_url: SmolStr,
    pub handler: BybitHandler,
    pub authenticator: BybitAuthenticator,
    pub client: Client,
    pub rate_limiter: Arc<BybitRateLimiter>,
}

impl BybitHttpClient {
    pub fn new(config: BybitConfig) -> Self {
        let base_url = config.rest_base_url().into();

        let rate_limiter = Arc::new(BybitRateLimiter::new(config.rate_limiter.clone()));

        BybitHttpClient {
            base_url,
            handler: BybitHandler,
            authenticator: BybitAuthenticator::new(
                config.credentials.api_key.clone(),
                config.credentials.api_secret.clone(),
            ),
            client: Client::new(),
            rate_limiter,
        }
    }

    pub async fn call<Req, Res>(
        &self,
        endpoint: &str,
        method: HttpMethod,
        params: Option<Req>,
        body: Option<Req>,
        custom_headers: Option<HashMap<String, String>>,
        signed: bool,
    ) -> Result<Option<Res>, EngineError>
    where
        Req: Serialize + Sized + Debug + Sync,
        Res: for<'de> Deserialize<'de> + Debug + Send,
    {
        let url = self.handler.build_url(&self.base_url, endpoint).await;
        let http_method: reqwest::Method = method.into();
        let mut request_builder = self.client.request(http_method.clone(), url.as_str());

        let query_params = BybitHandler::prepare_params(params)?;
        let json_body_string = BybitHandler::prepare_body(body)?;

        // Log request details for debugging
        tracing::debug!(
            "Bybit REST request: {} {} with params: {:?}, body: {:?}, signed: {}",
            http_method,
            url,
            query_params,
            json_body_string.as_deref(),
            signed
        );

        if signed {
            let timestamp = chrono::Utc::now().timestamp_millis();
            let recv_window = self.authenticator.recv_window;

            let query_string = match http_method {
                reqwest::Method::GET => BybitHandler::serialize_query_string(&query_params)?,
                reqwest::Method::POST | reqwest::Method::PUT | reqwest::Method::DELETE => {
                    json_body_string.clone().unwrap_or_default()
                }
                _ => String::new(),
            };

            let api_key_str = self.authenticator.api_key.expose();
            let payload = if let Some(ref json_body) = json_body_string {
                format!("{timestamp}{api_key_str}{recv_window}{json_body}")
            } else {
                format!("{timestamp}{api_key_str}{recv_window}{query_string}")
            };

            let headers = crate::exchanges::bybit::authenticator::rest::build_signed_headers(
                &self.authenticator.api_key,
                &self.authenticator.api_secret,
                timestamp,
                recv_window,
                &payload,
            )?;

            for (key, value) in &headers {
                request_builder = request_builder.header(key.as_str(), value.as_str());
            }

            if let Some(json_body) = json_body_string {
                request_builder = request_builder
                    .header("Content-Type", "application/json")
                    .body(json_body);
            }
        }

        if let Some(headers) = custom_headers {
            for (key, value) in headers {
                request_builder = request_builder.header(key, value);
            }
        }

        if !query_params.is_empty() && http_method == reqwest::Method::GET {
            request_builder = request_builder.query(&query_params);
        }

        self.rate_limiter.wait_if_needed_endpoint(endpoint).await;
        self.rate_limiter.wait_if_needed_ip_http().await;

        let response = request_builder.send().await?;
        let status = response.status();
        let text = response.text().await?;

        // Log response for debugging
        tracing::debug!(
            "Bybit REST response: {} {} -> status: {}, body: {}",
            http_method,
            url,
            status,
            if text.len() > 500 {
                format!("{}... (truncated)", &text[..500])
            } else {
                text.clone()
            }
        );

        let wrapper =
            BybitHandler::parse_and_validate_response(url.as_str(), status, &text).await?;

        self.rate_limiter.record_endpoint(endpoint).await;
        self.rate_limiter.record_ip_http().await;

        // Try to deserialize the full response directly first (for response types that include retCode/retMsg)
        // If that fails, fall back to extracting just the result field
        match serde_json::from_str::<Res>(&text) {
            Ok(full_response) => Ok(Some(full_response)),
            Err(_) => {
                // Fallback: extract result field and deserialize it
                if let Some(result_value) = wrapper.result {
                    let result_str =
                        serde_json::to_string(&result_value).map_err(EngineError::JsonError)?;
                    let mapped_response: Res =
                        serde_json::from_str(&result_str).map_err(EngineError::JsonError)?;
                    Ok(Some(mapped_response))
                } else {
                    Ok(None)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchanges::bybit::pagination::BYBIT_OPEN_ORDERS_PATH;
    use trading_core::rest::RestExchangeHandler;

    #[tokio::test]
    async fn demo_open_orders_url_is_v5_realtime() {
        let url = BybitHandler
            .build_url("https://api-demo.bybit.com/v5/", BYBIT_OPEN_ORDERS_PATH)
            .await;
        assert_eq!(url.as_str(), "https://api-demo.bybit.com/v5/order/realtime");
    }
}
