//! Binance API Authentication (ED25519)
//!
//! Shared authenticator for both REST and WebSocket APIs.
//! Binance supports ED25519 for both REST and WebSocket, and it's the recommended method.

use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use trading_core::{
    authenticator::ExchangeAuthenticator, error::EngineError, secret::SecretString,
};

#[derive(Clone, Debug)]
pub struct BinanceAuthenticator {
    pub api_key: SecretString,
    pub secret_key: SecretString,
}

impl BinanceAuthenticator {
    pub fn new(api_key: SecretString, secret_key: SecretString) -> Self {
        Self {
            api_key,
            // ED25519 private key (base64-encoded, 32 bytes when decoded)
            // See: https://www.binance.com/en/support/faq/detail/6b9a63f1e3384cf48a2eedb82767a69a
            secret_key,
        }
    }
}

#[async_trait]
impl ExchangeAuthenticator for BinanceAuthenticator {
    fn generate_signature(&self, payload: &str) -> Result<Option<SmolStr>, EngineError> {
        let private_key_bytes = general_purpose::STANDARD
            .decode(self.secret_key.expose())
            .map_err(|e| EngineError::AuthenticationError(smol_str::format_smolstr!("{}", e)))?;

        if private_key_bytes.len() != 32 {
            return Err(EngineError::AuthenticationError(smol_str::format_smolstr!(
                "ED25519 private key must be 32 bytes when decoded, got {}",
                private_key_bytes.len()
            )));
        }

        let signing_key = SigningKey::from_bytes(&private_key_bytes.try_into().unwrap());
        let signature = signing_key.sign(payload.as_bytes());

        Ok(Some(
            general_purpose::STANDARD
                .encode(signature.to_bytes())
                .into(),
        ))
    }

    fn build_signed_params(
        &self,
        mut params: BTreeMap<SmolStr, SmolStr>,
    ) -> Result<BTreeMap<SmolStr, SmolStr>, EngineError> {
        let sorted_params: BTreeMap<String, String> = params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let query_string = serde_urlencoded::to_string(&sorted_params)
            .map_err(EngineError::JsonUrlEncodedSerError)?;

        if let Some(signature) = self.generate_signature(&query_string)? {
            params.insert("signature".into(), signature);
        }

        Ok(params)
    }

    fn get_headers(&self) -> Result<Option<BTreeMap<SmolStr, SmolStr>>, EngineError> {
        let mut headers = BTreeMap::new();
        headers.insert(
            "X-MBX-APIKEY".into(),
            self.api_key.expose_smol_str().clone(),
        );
        Ok(Some(headers))
    }
}
