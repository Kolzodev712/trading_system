use anyhow::anyhow;
use core_logic::{authenticator::ExchangeAuthenticator, error::EngineError};

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use smol_str::SmolStr;
use std::collections::BTreeMap;

use base64::{engine::general_purpose, Engine as _};
use ed25519_dalek::{Signer, SigningKey};

#[derive(Clone, Debug)]
pub struct BinanceAuthenticator {
    pub api_key: SmolStr,
    pub secret_key: SmolStr,
}

#[async_trait]
impl ExchangeAuthenticator for BinanceAuthenticator {
    fn generate_signature(&self, query_string: &str) -> Result<Option<SmolStr>, EngineError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret_key.as_bytes()).map_err(|e| {
            EngineError::AuthenticationError(anyhow!(format!("Invalid mac lenght: {e}")))
        })?;
        mac.update(query_string.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        Ok(Some(signature.into()))
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
        headers.insert("X-MBX-APIKEY".into(), self.api_key.clone());
        Ok(Some(headers))
    }
}

#[derive(Clone, Debug)]
pub struct BinanceWsAuthenticator {
    pub api_key: SmolStr,
    pub secret_key_ed25519: SmolStr,
}

impl BinanceWsAuthenticator {
    pub fn new(api_key: SmolStr, secret_key: SmolStr) -> Self {
        Self {
            api_key,
            // to get this key use info from here <https://www.binance.com/en/support/faq/detail/6b9a63f1e3384cf48a2eedb82767a69a>
            secret_key_ed25519: secret_key,
        }
    }
}

#[async_trait]
impl ExchangeAuthenticator for BinanceWsAuthenticator {
    fn generate_signature(&self, payload: &str) -> Result<Option<SmolStr>, EngineError> {
        let private_key_bytes = general_purpose::STANDARD
            .decode(&*self.secret_key_ed25519)
            .map_err(|e| EngineError::AuthenticationError(anyhow!(e)))?;

        if private_key_bytes.len() != 32 {
            return Err(EngineError::AuthenticationError(anyhow!(
                "Private key must be 32 bytes, got {}",
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
}
