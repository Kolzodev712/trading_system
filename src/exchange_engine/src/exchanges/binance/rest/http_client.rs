use super::{
    super::config::BinanceConfig, super::utils::fetch_server_time, sign::BinanceAuthenticator,
};
use async_trait::async_trait;
use core_logic::{
    authenticator::RestAuthenticator, error::EngineError, rest_client::RestExchangeHandler,
    utils::generate_string_params,
};
use reqwest::{Client, Method};
use serde::Serialize;
use smol_str::SmolStr;
use std::collections::HashMap;

const BINANCE_SPOT_URL: &str = "https://api.binance.com/api/v3/";

#[derive(Clone, Debug)]
pub struct BinanceHttpClient {
    pub base_url: SmolStr,
    pub handler: BinanceHandler,
    pub authenticator: BinanceAuthenticator,
    pub client: Client,
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
        }
    }

    pub async fn call<T>(
        &self,
        endpoint: &str,
        method: Method,
        mut params: Option<HashMap<SmolStr, SmolStr>>,
        body: Option<&T>,
    ) -> Result<SmolStr, EngineError>
    where
        T: Serialize + ?Sized + std::fmt::Debug,
    {
        if let Some(payload) = body {
            let serialized_payload =
                serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());

            let serialized_payload: HashMap<String, String> =
                serde_json::from_str(&serialized_payload).unwrap_or_default();

            let params_mut = params.get_or_insert_with(HashMap::new);
            for (key, value) in serialized_payload {
                params_mut.insert(SmolStr::new(key), SmolStr::new(value));
            }
        }

        let url = self
            .handler
            .build_full_url(&self.base_url, endpoint, Some(&self.authenticator))
            .await;

        let signed_params = self
            .handler
            .get_params(Some(&self.authenticator))
            .await?
            .unwrap_or_default();

        if let Some(ref mut query_params) = params {
            query_params.extend(signed_params);
        } else {
            params = Some(signed_params);
        }

        let mut request_builder = self.client.request(method.clone(), url.to_string());

        if let Some(headers) = self.handler.get_headers(Some(&self.authenticator))? {
            for (key, value) in headers {
                request_builder = request_builder.header(key.as_str(), value.as_str());
            }
        }

        request_builder =
            request_builder.query(&generate_string_params(&mut params.unwrap_or_default()));

        let response = request_builder.send().await?;
        let status = response.status();
        let text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".into());

        self.handler.handle_response(&url, status, &text).await?;
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
        let mut params = HashMap::new();
        params.insert("recvWindow".into(), "60000".into());

        if let Some(auth) = authenticator {
            let signed_params = auth
                .build_signed_params(params, Some(60000), Some(fetch_server_time().await?))
                .await?;
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
