//! Bybit authentication logic
//!
//! REST and WebSocket use different authentication methods:
//! - REST: Uses headers with signature based on {timestamp}{api_key}{recv_window}{query_string_or_json_body}
//! - WebSocket: Uses auth message with signature based on GET/realtime{expires}

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use smol_str::SmolStr;
use std::collections::BTreeMap;
use trading_core::{
    authenticator::ExchangeAuthenticator, error::EngineError, secret::SecretString,
};

#[derive(Clone, Debug)]
pub struct BybitAuthenticator {
    pub api_key: SecretString,
    pub api_secret: SecretString,
    pub recv_window: u64,
}

impl BybitAuthenticator {
    pub fn new(api_key: SecretString, api_secret: SecretString) -> Self {
        Self {
            api_key,
            api_secret,
            recv_window: 5000,
        }
    }

    fn generate_hmac_signature(&self, payload: &str) -> Result<String, EngineError> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.api_secret.expose().as_bytes()).map_err(|e| {
                EngineError::AuthenticationError(anyhow::anyhow!("Invalid secret key: {e}"))
            })?;
        mac.update(payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        Ok(signature)
    }
}

#[async_trait]
impl ExchangeAuthenticator for BybitAuthenticator {
    fn generate_signature(&self, payload: &str) -> Result<Option<SmolStr>, EngineError> {
        let signature = self.generate_hmac_signature(payload)?;
        Ok(Some(signature.into()))
    }

    fn build_signed_params(
        &self,
        params: BTreeMap<SmolStr, SmolStr>,
    ) -> Result<BTreeMap<SmolStr, SmolStr>, EngineError> {
        Ok(params)
    }

    fn get_headers(&self) -> Result<Option<BTreeMap<SmolStr, SmolStr>>, EngineError> {
        let mut headers = BTreeMap::new();
        headers.insert(
            "X-BAPI-API-KEY".into(),
            self.api_key.expose_smol_str().clone(),
        );
        Ok(Some(headers))
    }
}

pub mod rest {
    use super::*;

    pub fn build_signed_headers(
        api_key: &SecretString,
        api_secret: &SecretString,
        timestamp: i64,
        recv_window: u64,
        payload: &str,
    ) -> Result<BTreeMap<String, String>, EngineError> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(api_secret.expose().as_bytes()).map_err(|e| {
                EngineError::AuthenticationError(anyhow::anyhow!("Invalid secret key: {e}"))
            })?;
        mac.update(payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let timestamp_str = timestamp.to_string();
        let recv_window_str = recv_window.to_string();
        let api_key_str = api_key.expose().to_string();

        let mut headers = BTreeMap::new();
        headers.insert("X-BAPI-API-KEY".to_string(), api_key_str);
        headers.insert("X-BAPI-TIMESTAMP".to_string(), timestamp_str);
        headers.insert("X-BAPI-RECV-WINDOW".to_string(), recv_window_str);
        headers.insert("X-BAPI-SIGN".to_string(), signature);

        Ok(headers)
    }
}

pub mod websocket {
    use super::*;

    pub fn generate_auth_signature(
        api_secret: &SecretString,
        expires: i64,
    ) -> Result<String, EngineError> {
        let payload = format!("GET/realtime{}", expires);
        let mut mac =
            Hmac::<Sha256>::new_from_slice(api_secret.expose().as_bytes()).map_err(|e| {
                EngineError::AuthenticationError(anyhow::anyhow!("Invalid secret key: {e}"))
            })?;
        mac.update(payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        Ok(signature)
    }
}
