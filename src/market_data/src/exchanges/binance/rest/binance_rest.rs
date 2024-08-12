use super::types::{BinancePrivateEndpoint, BinancePublicEndpoint, BinanceSymbol};
use async_trait::async_trait;
use core_logic::{
    authenticator::RestAuthenticator,
    data::DataType,
    error::KolzoError,
    rest_client::{make_rest_request, HttpMethod, RestExchangeHandler},
    utils::current_timestamp,
};
use hmac::{Hmac, Mac};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use sha2::Sha256;
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

pub const BINANCE_URL: &str = "https://api.binance.com/api/v3/";

/// Enum to handle both public and private endpoints
pub enum BinanceEndpoint {
    Public(BinancePublicEndpoint),
    Private(BinancePrivateEndpoint),
}

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
    fn generate_signature(&self, query_string: &str) -> Option<String> {
        if let Some(secret_key) = &self.secret_key {
            let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(query_string.as_bytes());
            let result = mac.finalize();
            let signature_bytes = result.into_bytes();
            Some(hex::encode(signature_bytes))
        } else {
            None
        }
    }

    fn build_signed_params(
        &self,
        mut params: HashMap<String, String>,
        recv_window: Option<u64>,
        timestamp: Option<u64>,
    ) -> HashMap<String, String> {
        let timestamp = timestamp.unwrap_or_else(current_timestamp);
        params.insert("timestamp".to_string(), timestamp.to_string());

        let recv_window = recv_window.unwrap_or(10000);
        params.insert("recvWindow".to_string(), recv_window.to_string());

        let query_string: String = params
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join("&");

        if let Some(signature) = self.generate_signature(&query_string) {
            params.insert("signature".to_string(), signature);
        }

        params
    }

    fn get_headers(&self) -> Option<HashMap<String, String>> {
        let mut headers = HashMap::new();
        if let Some(ref key) = self.api_key {
            headers.insert("X-MBX-APIKEY".to_string(), key.clone());
        }
        if headers.is_empty() {
            None
        } else {
            Some(headers)
        }
    }
}

pub struct BinanceExchangeHandler {
    endpoint: BinanceEndpoint,
    authenticator: BinanceAuthenticator,
    recv_window: Option<u64>,
    timestamp: Option<u64>,
}

impl BinanceExchangeHandler {
    pub fn new_public(
        data_type: DataType,
        symbol: BinanceSymbol,
        limit: Option<u32>,
        recv_window: Option<u64>,
        timestamp: Option<u64>,
        api_key: Option<String>,
        secret_key: Option<String>,
    ) -> Self {
        let endpoint = match data_type {
            DataType::Orderbook => {
                BinanceEndpoint::Public(BinancePublicEndpoint::Orderbook { symbol, limit })
            }
            DataType::Trade => BinanceEndpoint::Public(BinancePublicEndpoint::Trade { symbol }),
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
        let text = make_rest_request(
            BINANCE_URL,
            endpoint_path,
            HttpMethod::GET,
            Some(self),
            &self.authenticator,
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
        authenticator: &dyn RestAuthenticator,
    ) -> Option<HashMap<String, String>> {
        authenticator.get_headers()
    }

    fn get_params(&self, authenticator: &dyn RestAuthenticator) -> Option<HashMap<String, String>> {
        let params = match &self.endpoint {
            BinanceEndpoint::Public(endpoint) => endpoint.get_params(),
            BinanceEndpoint::Private(endpoint) => authenticator.build_signed_params(
                endpoint.get_params(),
                self.recv_window,
                self.timestamp,
            ),
        };

        Some(params)
    }

    fn get_body(&self) -> Option<String> {
        None
    }

    fn authenticate(
        &self,
        authenticator: &dyn RestAuthenticator,
    ) -> Option<HashMap<String, String>> {
        authenticator.get_headers()
    }
}
