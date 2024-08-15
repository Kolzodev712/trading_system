use super::types::{BinanceDataType, BinancePrivateEndpoint, BinancePublicEndpoint, BinanceSymbol};
use async_trait::async_trait;
use core_logic::{
    authenticator::RestAuthenticator,
    error::KolzoError,
    rest_client::{create_rest_request, HttpMethod, RestExchangeHandler},
    utils::current_timestamp,
};
use hmac::{Hmac, Mac};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceAuthenticator {
    pub api_key: Option<String>,
    pub secret_key: Option<String>,
}

impl BinanceAuthenticator {
    pub fn new(api_key: Option<String>, secret_key: Option<String>) -> Self {
        BinanceAuthenticator {
            api_key,
            secret_key,
        }
    }
}

#[async_trait]
impl RestAuthenticator for BinanceAuthenticator {
    async fn generate_signature(&self, query_string: &str) -> Result<Option<String>, KolzoError> {
        if let Some(secret_key) = &self.secret_key {
            let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
                .map_err(|e| KolzoError::CustomError(anyhow::anyhow!(e.to_string())))?;
            mac.update(query_string.as_bytes());
            let result = mac.finalize();
            let signature_bytes = result.into_bytes();
            Ok(Some(hex::encode(signature_bytes)))
        } else {
            Ok(None)
        }
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
        if let Some(ref key) = self.api_key {
            headers.insert("X-MBX-APIKEY".to_string(), key.clone());
        }
        if headers.is_empty() {
            Ok(None)
        } else {
            Ok(Some(headers))
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceExchangeHandler {
    endpoint: BinanceEndpoint,
    authenticator: BinanceAuthenticator,
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
        api_key: Option<String>,
        secret_key: Option<String>,
    ) -> Self {
        let endpoint = match data_type {
            BinanceDataType::Orderbook => {
                BinanceEndpoint::Public(BinancePublicEndpoint::Orderbook { symbol, limit })
            }
            BinanceDataType::Trade => {
                BinanceEndpoint::Public(BinancePublicEndpoint::Trade { symbol })
            }
        };

        let authenticator = BinanceAuthenticator::new(api_key, secret_key);

        BinanceExchangeHandler {
            endpoint,
            authenticator,
            recv_window,
            timestamp,
        }
    }

    // Constructor for private endpoints
    pub fn new_private(
        private_endpoint: BinancePrivateEndpoint,
        recv_window: Option<u64>,
        timestamp: Option<u64>,
        api_key: Option<String>,
        secret_key: Option<String>,
    ) -> Self {
        let authenticator = BinanceAuthenticator::new(api_key, secret_key);

        BinanceExchangeHandler {
            endpoint: BinanceEndpoint::Private(private_endpoint),
            authenticator,
            recv_window,
            timestamp,
        }
    }

    pub async fn get<T>(&self) -> Result<T, KolzoError>
    where
        T: DeserializeOwned,
    {
        let endpoint_path = self.get_endpoint_path();
        let text = create_rest_request(
            BINANCE_URL,
            endpoint_path,
            HttpMethod::GET,
            Some(self),
            Some(&self.authenticator),
        )
        .await?;
        let result: T = serde_json::from_str(&text).map_err(KolzoError::JsonError)?;
        Ok(result)
    }

    fn get_endpoint_path(&self) -> &str {
        match &self.endpoint {
            BinanceEndpoint::Public(endpoint) => endpoint.get_endpoint(),
            BinanceEndpoint::Private(endpoint) => endpoint.get_endpoint(),
        }
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
