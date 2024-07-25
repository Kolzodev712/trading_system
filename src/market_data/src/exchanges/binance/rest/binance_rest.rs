use async_trait::async_trait;
use core_logic::{
    error::KolzoError,
    rest_client::{make_rest_request, HttpMethod, RestExchangeHandler},
};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

pub struct BinanceExchangeHandler {
    url: String,
}

impl BinanceExchangeHandler {
    pub fn new(url: String) -> Self {
        BinanceExchangeHandler { url }
    }

    pub fn get_url(&self) -> &str {
        &self.url
    }

    pub async fn get<T>(&self) -> Result<T, KolzoError>
    where
        T: DeserializeOwned,
    {
        let text = make_rest_request(&self.url, HttpMethod::GET, Some(self)).await?;
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
        None
    }

    fn get_body(&self) -> Option<String> {
        None
    }
}

// make params adjustable
// make it more flexible
