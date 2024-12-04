use super::{super::config::BinanceConfig, sign::BinanceAuthenticator};
use async_trait::async_trait;
use core_logic::{
    authenticator::RestAuthenticator, error::EngineError, rest_client::RestExchangeHandler,
    utils::generate_string_params,
};
use reqwest::{Client, Method};
use serde::Serialize;
use smol_str::SmolStr;
use std::{collections::HashMap, fmt::Debug, time::Instant};
use tokio::{sync::Mutex, time::Duration};

const BINANCE_SPOT_URL: &str = "https://api.binance.com/api/v3/";

#[derive(Debug)]
pub struct BinanceHttpClient {
    pub base_url: SmolStr,
    pub handler: BinanceHandler,
    pub authenticator: BinanceAuthenticator,
    pub client: Client,
    pub server_time: Mutex<Option<(u64, Instant)>>,
}

impl BinanceHttpClient {
    pub fn new(config: BinanceConfig) -> Self {
        BinanceHttpClient {
            base_url: BINANCE_SPOT_URL.into(),
            handler: BinanceHandler,
            authenticator: BinanceAuthenticator {
                api_key: config.api_key,
                secret_key: config.secret_key,
            },
            client: Client::new(),
            server_time: Mutex::new(None),
        }
    }

    pub async fn get_server_time(&self) -> Result<u64, EngineError> {
        let mut cache = self.server_time.lock().await;
        let now = Instant::now();

        if let Some((cached_time, timestamp)) = *cache {
            if now.duration_since(timestamp) < Duration::from_secs(60) {
                return Ok(cached_time);
            }
        }

        let response: HashMap<String, u64> = self
            .unsigned_call::<(), HashMap<String, u64>>("/api/v3/time", None)
            .await?;
        let server_time = response
            .get("serverTime")
            .ok_or_else(|| EngineError::HttpCustomError("Missing serverTime in response".into()))?;

        *cache = Some((*server_time, now));
        Ok(*server_time)
    }

    async fn prepare_params(
        &self,
        params: Option<HashMap<SmolStr, SmolStr>>,
        signed: bool,
    ) -> Result<HashMap<SmolStr, SmolStr>, EngineError> {
        let mut final_params = params.unwrap_or_default();

        if signed {
            let server_time = self.get_server_time().await?;
            final_params.insert("recvWindow".into(), "60000".into());
            final_params.insert("timestamp".into(), server_time.to_string().into());

            let signed_params = self.authenticator.build_signed_params(final_params).await?;
            Ok(signed_params)
        } else {
            Ok(final_params)
        }
    }

    pub async fn unsigned_call<T, R>(
        &self,
        endpoint: &str,
        params: Option<HashMap<SmolStr, SmolStr>>,
    ) -> Result<R, EngineError>
    where
        R: for<'de> serde::Deserialize<'de>,
    {
        let url = self
            .handler
            .build_full_url(&self.base_url, endpoint, None)
            .await;

        // Directly use `params` without calling `prepare_params`
        let mut final_params = params.unwrap_or_default();
        let query_string = generate_string_params(&mut final_params);

        let response = self
            .client
            .get(url.to_string())
            .query(&query_string)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            return Err(EngineError::HttpCustomError(format!(
                "Request failed with status {} and body: {}",
                status, text
            )));
        }

        let mapped_response =
            serde_json::from_str::<R>(&text).map_err(|e| EngineError::JsonError(e))?;

        Ok(mapped_response)
    }

    pub async fn signed_call<T>(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<HashMap<SmolStr, SmolStr>>,
        body: Option<&T>,
    ) -> Result<SmolStr, EngineError>
    where
        T: Serialize + ?Sized + Debug,
    {
        let url = self
            .handler
            .build_full_url(&self.base_url, endpoint, Some(&self.authenticator))
            .await;

        let mut final_params = self.prepare_params(params, true).await?;

        if let Some(payload) = body {
            let serialized_payload =
                serde_json::to_string(payload).map_err(|e| EngineError::JsonError(e))?;

            final_params.insert("payload".into(), serialized_payload.into());
        }

        let query_string = generate_string_params(&mut final_params);

        let mut request_builder = self.client.request(method.clone(), url.to_string());

        if let Some(headers) = self.handler.get_headers(Some(&self.authenticator))? {
            for (key, value) in headers {
                request_builder = request_builder.header(key.as_str(), value.as_str());
            }
        }

        let response = request_builder.query(&query_string).send().await?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            return Err(EngineError::HttpCustomError(format!(
                "Request failed with status {} and body: {}",
                status, text
            )));
        }

        Ok(text.into())
    }
}

#[derive(Clone, Debug)]
pub struct BinanceHandler;

#[async_trait]
impl RestExchangeHandler for BinanceHandler {
    async fn build_full_url(
        &self,
        base_url: &str,
        endpoint: &str,
        _authenticator: Option<&dyn RestAuthenticator>,
    ) -> SmolStr {
        // No additional URL customization for Binance
        let full_url = format!("{}{}", base_url, endpoint);
        println!("Full URL: {}", full_url);
        full_url.into()
    }

    async fn handle_response(
        &self,
        rest_url: &str,
        status: reqwest::StatusCode,
        text: &str,
    ) -> Result<(), EngineError> {
        if !status.is_success() {
            Err({
                EngineError::HttpCustomError(format!(
                    "Request to {} failed with status {} and body: {}",
                    rest_url, status, text
                ))
            })
        } else {
            Ok(())
        }
    }

    fn get_headers(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        if let Some(auth) = authenticator {
            auth.get_headers()
        } else {
            Ok(None)
        }
    }

    async fn get_params(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        let params = HashMap::new();
        if let Some(auth) = authenticator {
            let signed_params = auth.build_signed_params(params).await?;
            Ok(Some(signed_params))
        } else {
            Ok(Some(params))
        }
    }

    fn get_body(&self) -> Option<SmolStr> {
        // Binance typically doesn't require a body for most requests
        None
    }

    fn authenticate(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        if let Some(auth) = authenticator {
            auth.get_headers()
        } else {
            Ok(None)
        }
    }
}
