use super::types::{BinanceDataType, BinancePrivateEndpoint, BinancePublicEndpoint, BinanceSymbol};
use async_trait::async_trait;
use core_logic::{
    authenticator::RestAuthenticator, error::KolzoError, rest_client::RestExchangeHandler,
    utils::current_timestamp,
};
use hmac::{Hmac, Mac};
use reqwest::StatusCode;
use serde::{
    de::DeserializeOwned,
    {Deserialize, Serialize},
};
use sha2::Sha256;
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
    pub api_key: String,
    pub secret_key: String,
}

impl BinanceAuthenticator {
    pub fn new(api_key: String, secret_key: String) -> Self {
        BinanceAuthenticator {
            api_key,
            secret_key,
        }
    }
}

#[async_trait]
impl RestAuthenticator for BinanceAuthenticator {
    async fn generate_signature(&self, query_string: &str) -> Result<Option<String>, KolzoError> {
        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .map_err(|e| KolzoError::CustomError(anyhow::anyhow!(e.to_string())))?;
        mac.update(query_string.as_bytes());
        let result = mac.finalize();
        let signature_bytes = result.into_bytes();
        Ok(Some(hex::encode(signature_bytes)))
    }

    async fn build_signed_params(
        &self,
        mut params: HashMap<String, String>,
        recv_window: Option<u64>,
        timestamp: Option<u64>,
    ) -> Result<HashMap<String, String>, KolzoError> {
        let timestamp = timestamp.unwrap_or_else(current_timestamp);
        params.insert("timestamp".to_string(), timestamp.to_string());

        let recv_window = recv_window.unwrap_or(10000);
        params.insert("recvWindow".to_string(), recv_window.to_string());

        let query_string: String = params
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join("&");

        if let Some(signature) = self.generate_signature(&query_string).await? {
            params.insert("signature".to_string(), signature);
        }

        Ok(params)
    }

    fn get_headers(&self) -> Result<Option<HashMap<String, String>>, KolzoError> {
        let mut headers = HashMap::new();
        headers.insert("X-MBX-APIKEY".to_string(), self.api_key.clone());
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
        api_key: String,
        secret_key: String,
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

    pub async fn get<T>(&self) -> Result<T, KolzoError>
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
        let mut request_builder = client.get(&url);

        if let Some(headers) = self.get_headers(authenticator)? {
            for (key, value) in headers {
                request_builder = request_builder.header(&key, &value);
            }
        }

        let response = request_builder.send().await?;
        let status = response.status();
        let text = response.text().await?;

        self.handle_response(&url, status, &text).await?;

        serde_json::from_str::<T>(&text).map_err(KolzoError::JsonError)
    }
}

#[async_trait]
impl RestExchangeHandler for BinanceExchangeHandler {
    async fn build_full_url(
        &self,
        base_url: &str,
        endpoint: &str,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> String {
        let mut full_url = format!("{}{}", base_url, endpoint);

        if let Ok(Some(params)) = self.get_params(authenticator).await {
            let query_string: String = params
                .into_iter()
                .map(|(key, value)| format!("{}={}", key, value))
                .collect::<Vec<String>>()
                .join("&");
            full_url = format!("{}?{}", full_url, query_string);
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
    ) -> Result<Option<HashMap<String, String>>, KolzoError> {
        if let Some(authenticator) = authenticator {
            authenticator.get_headers()
        } else {
            Ok(None)
        }
    }

    async fn get_params(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<String, String>>, KolzoError> {
        let params = match &self.endpoint {
            BinanceEndpoint::Public(endpoint) => Some(endpoint.get_params()),
            BinanceEndpoint::Private(endpoint) => {
                if let Some(authenticator) = authenticator {
                    Some(
                        authenticator
                            .build_signed_params(
                                endpoint.get_params(),
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

    fn get_body(&self) -> Option<String> {
        None
    }

    fn authenticate(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<String, String>>, KolzoError> {
        if let Some(authenticator) = authenticator {
            authenticator.get_headers()
        } else {
            Ok(None)
        }
    }
}
