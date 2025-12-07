use super::super::{config::BinanceConfig, utils::deserialize_or_none};
use crate::exchanges::binance::sign::BinanceAuthenticator;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Debug,
};
use tracing::debug;
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
}

impl BinanceHttpClient {
    pub fn new(config: BinanceConfig) -> Self {
        BinanceHttpClient {
            base_url: BINANCE_SPOT_URL.into(),
            handler: BinanceHandler,
            authenticator: BinanceAuthenticator {
                api_key: config.rest.api_key,
                secret_key: config.rest.secret_key,
            },
            client: Client::new(),
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

        let response = request_builder.send().await?;
        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            return Err(EngineError::HttpCustomError(SmolStr::new(format!(
                "Request failed with status {} and body: {}",
                status, text
            ))));
        }

        let mapped_response = deserialize_or_none::<Res>(&text)?;

        Ok(mapped_response)
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
