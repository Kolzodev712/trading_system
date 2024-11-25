use super::types::{BinanceDataType, BinancePrivateEndpoint, BinancePublicEndpoint, BinanceSymbol};
use async_trait::async_trait;
use core_logic::{
    authenticator::RestAuthenticator,
    error::KolzoError,
    rest_client::{HttpMethod, RestExchangeHandler},
    utils::{current_timestamp, deserialize_smol_str, serialize_smol_str},
};
use hmac::{Hmac, Mac};
use reqwest::StatusCode;
use serde::{
    de::DeserializeOwned,
    {Deserialize, Serialize},
};
use sha2::Sha256;
use smol_str::{SmolStr, ToSmolStr};
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

pub const BINANCE_URL: &str = "https://api.binance.com/api/v3/";

/// Enum to handle both public and private endpoints
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinanceEndpoint {
    Public(BinancePublicEndpoint),
    Private(BinancePrivateEndpoint),
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct BinanceAuthenticator {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub api_key: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub secret_key: SmolStr,
}

impl BinanceAuthenticator {
    pub fn new(api_key: SmolStr, secret_key: SmolStr) -> Self {
        BinanceAuthenticator {
            api_key,
            secret_key,
        }
    }
}

#[async_trait]
impl RestAuthenticator for BinanceAuthenticator {
    async fn generate_signature(&self, query_string: &str) -> Result<Option<SmolStr>, KolzoError> {
        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .map_err(|e| KolzoError::CustomError(anyhow::anyhow!(e)))?;
        mac.update(query_string.as_bytes());
        let result = mac.finalize();
        let signature_bytes = result.into_bytes();
        Ok(Some(SmolStr::new(hex::encode(signature_bytes))))
    }

    async fn build_signed_params(
        &self,
        mut params: HashMap<SmolStr, SmolStr>,
        recv_window: Option<u64>,
        timestamp: Option<u64>,
    ) -> Result<HashMap<SmolStr, SmolStr>, KolzoError> {
        let timestamp = timestamp.unwrap_or_else(current_timestamp);
        params.insert(SmolStr::new("timestamp"), timestamp.to_smolstr());

        let recv_window = recv_window.unwrap_or(10000);
        params.insert(SmolStr::new("recvWindow"), recv_window.to_smolstr());

        let query_string: String = params
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join("&");

        if let Some(signature) = self.generate_signature(&query_string).await? {
            params.insert(SmolStr::new("signature"), signature);
        }

        Ok(params)
    }

    fn get_headers(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, KolzoError> {
        let mut headers = HashMap::new();
        headers.insert(SmolStr::new("X-MBX-APIKEY"), self.api_key.clone());
        Ok(Some(headers))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct BinanceExchangeHandler {
    endpoint: BinanceEndpoint,
    authenticator: Option<BinanceAuthenticator>,
    recv_window: Option<u64>,
    timestamp: Option<u64>,
}

impl BinanceExchangeHandler {
    pub fn new_public(
        data_type: BinanceDataType,
        symbol: BinanceSymbol,
        limit: Option<u64>,
        recv_window: Option<u64>,
        timestamp: Option<u64>,
    ) -> Self {
        let endpoint = match data_type {
            BinanceDataType::Orderbook => {
                BinanceEndpoint::Public(BinancePublicEndpoint::Orderbook { symbol, limit })
            }
            BinanceDataType::Trade => {
                BinanceEndpoint::Public(BinancePublicEndpoint::Trade { symbol })
            }
            BinanceDataType::ServerTime => {
                BinanceEndpoint::Public(BinancePublicEndpoint::ServerTime)
            }
        };

        BinanceExchangeHandler {
            endpoint,
            authenticator: None, // No authenticator needed for public endpoints
            recv_window,
            timestamp,
        }
    }

    pub fn new_private(
        private_endpoint: BinancePrivateEndpoint,
        recv_window: Option<u64>,
        timestamp: Option<u64>,
        api_key: SmolStr,
        secret_key: SmolStr,
    ) -> Self {
        let authenticator = BinanceAuthenticator::new(api_key, secret_key);

        BinanceExchangeHandler {
            endpoint: BinanceEndpoint::Private(private_endpoint),
            authenticator: Some(authenticator), // Authenticator required for private endpoints
            recv_window,
            timestamp,
        }
    }

    fn get_endpoint_path(&self) -> &str {
        match &self.endpoint {
            BinanceEndpoint::Public(endpoint) => endpoint.get_endpoint(),
            BinanceEndpoint::Private(endpoint) => endpoint.get_endpoint(),
        }
    }

    async fn request<T>(
        &self,
        method: HttpMethod,
        body: Option<HashMap<SmolStr, SmolStr>>,
    ) -> Result<T, KolzoError>
    where
        T: DeserializeOwned,
    {
        let authenticator: Option<&dyn RestAuthenticator> = self
            .authenticator
            .as_ref()
            .map(|auth| auth as &dyn RestAuthenticator);

        let url = self
            .build_full_url(BINANCE_URL, self.get_endpoint_path(), authenticator)
            .await;

        let client = reqwest::Client::new();
        let mut request_builder = match method {
            HttpMethod::GET => client.get(&*url.to_string()),
            HttpMethod::POST => client.post(&*url.to_string()),
            HttpMethod::PUT => client.put(&*url.to_string()),
            HttpMethod::DELETE => client.delete(&*url.to_string()),
            HttpMethod::PATCH => client.patch(&*url.to_string()),
        };

        if let Some(headers) = self.get_headers(authenticator)? {
            for (key, value) in headers {
                request_builder = request_builder.header(key.as_str(), value.as_str());
            }
        }

        if let Some(body_params) = body {
            let body_params_string: HashMap<String, String> = body_params
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            let body_json =
                serde_json::to_string(&body_params_string).map_err(|e| KolzoError::JsonError(e))?;
            request_builder = request_builder.body(body_json);
        }

        let response = request_builder.send().await?;
        let status = response.status();
        let text = response.text().await?;

        self.handle_response(&url, status, &text).await?;

        serde_json::from_str::<T>(&text).map_err(KolzoError::JsonError)
    }

    pub async fn get<T>(&self) -> Result<T, KolzoError>
    where
        T: DeserializeOwned,
    {
        self.request(HttpMethod::GET, None).await
    }

    pub async fn post<T>(&self, body: Option<HashMap<SmolStr, SmolStr>>) -> Result<T, KolzoError>
    where
        T: DeserializeOwned,
    {
        self.request(HttpMethod::POST, body).await
    }

    pub async fn put<T>(&self, body: Option<HashMap<SmolStr, SmolStr>>) -> Result<T, KolzoError>
    where
        T: DeserializeOwned,
    {
        self.request(HttpMethod::PUT, body).await
    }

    pub async fn delete<T>(&self, body: Option<HashMap<SmolStr, SmolStr>>) -> Result<T, KolzoError>
    where
        T: DeserializeOwned,
    {
        self.request(HttpMethod::DELETE, body).await
    }

    pub async fn patch<T>(&self, body: Option<HashMap<SmolStr, SmolStr>>) -> Result<T, KolzoError>
    where
        T: DeserializeOwned,
    {
        self.request(HttpMethod::PATCH, body).await
    }

    pub async fn server_time(&self) -> Result<u64, KolzoError> {
        let handler = BinanceExchangeHandler {
            endpoint: BinanceEndpoint::Public(BinancePublicEndpoint::ServerTime),
            authenticator: None,
            recv_window: None,
            timestamp: None,
        };

        let response: HashMap<String, u64> = handler.get().await?;
        response.get("serverTime").cloned().ok_or_else(|| {
            KolzoError::CustomError(anyhow::anyhow!("Missing serverTime in response"))
        })
    }
}

#[async_trait]
impl RestExchangeHandler for BinanceExchangeHandler {
    async fn build_full_url(
        &self,
        base_url: &str,
        endpoint: &str,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> SmolStr {
        let mut full_url = format!("{}{}", base_url, endpoint).into();

        if let Ok(Some(params)) = self.get_params(authenticator).await {
            let query_string: String = params
                .into_iter()
                .map(|(key, value)| format!("{}={}", key, value))
                .collect::<Vec<String>>()
                .join("&");
            full_url = format!("{}?{}", full_url, query_string).into();
        }

        full_url
    }

    async fn handle_response(
        &self,
        _rest_url: &str,
        status: StatusCode,
        text: &str,
    ) -> Result<(), KolzoError> {
        if status.is_success() {
            Ok(())
        } else {
            Err(KolzoError::CustomError(anyhow::anyhow!(
                "Error: {} - {}",
                status,
                text
            )))
        }
    }

    fn get_headers(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<SmolStr, SmolStr>>, KolzoError> {
        if let Some(authenticator) = authenticator {
            authenticator.get_headers()
        } else {
            Ok(None)
        }
    }

    async fn get_params(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<SmolStr, SmolStr>>, KolzoError> {
        let params = match &self.endpoint {
            BinanceEndpoint::Public(endpoint) => {
                let string_params = endpoint.get_params();
                Some(
                    string_params
                        .into_iter()
                        .map(|(k, v)| (SmolStr::new(k), SmolStr::new(v)))
                        .collect::<HashMap<SmolStr, SmolStr>>(),
                )
            }
            BinanceEndpoint::Private(endpoint) => {
                if let Some(authenticator) = authenticator {
                    Some(
                        authenticator
                            .build_signed_params(
                                endpoint
                                    .get_params()
                                    .into_iter()
                                    .map(|(k, v)| (SmolStr::new(k), SmolStr::new(v)))
                                    .collect(),
                                self.recv_window,
                                self.timestamp,
                            )
                            .await?,
                    )
                } else {
                    None
                }
            }
        };

        Ok(params)
    }

    fn get_body(&self) -> Option<SmolStr> {
        None
    }

    fn authenticate(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<SmolStr, SmolStr>>, KolzoError> {
        if let Some(authenticator) = authenticator {
            authenticator.get_headers()
        } else {
            Ok(None)
        }
    }
}
