use super::types::{BinanceRestEndpoint, BinanceSymbol};
use async_trait::async_trait;
use core_logic::{
    data::DataType,
    error::KolzoError,
    rest_client::{make_rest_request, HttpMethod, RestExchangeHandler},
};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

pub const BINANCE_URL: &str = "https://api.binance.com/api/v3/";

pub struct BinanceExchangeHandler {
    data_type: DataType,
    symbol: BinanceSymbol,
    limit: Option<u32>,
}

impl BinanceExchangeHandler {
    pub fn new(data_type: DataType, symbol: BinanceSymbol, limit: Option<u32>) -> Self {
        BinanceExchangeHandler {
            data_type,
            symbol,
            limit,
        }
    }

    pub async fn get<T>(&self) -> Result<T, KolzoError>
    where
        T: DeserializeOwned,
    {
        let endpoint = match self.data_type {
            DataType::Orderbook => BinanceRestEndpoint::Orderbook {
                symbol: self.symbol.clone(),
                limit: self.limit,
            },
            DataType::Trade => BinanceRestEndpoint::Trade {
                symbol: self.symbol.clone(),
            },
        };

        let text = make_rest_request(
            BINANCE_URL,
            endpoint.get_endpoint(),
            HttpMethod::GET,
            Some(self),
        )
        .await?;
        let result: T = serde_json::from_str(&text).map_err(KolzoError::JsonError)?;
        Ok(result)
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

    fn get_headers(&self) -> Option<HashMap<String, String>> {
        None
    }

    fn get_params(&self) -> Option<HashMap<String, String>> {
        let endpoint = match self.data_type {
            DataType::Orderbook => BinanceRestEndpoint::Orderbook {
                symbol: self.symbol.clone(),
                limit: self.limit,
            },
            DataType::Trade => BinanceRestEndpoint::Trade {
                symbol: self.symbol.clone(),
            },
        };
        Some(endpoint.get_params())
    }

    fn get_body(&self) -> Option<String> {
        None
    }
}
