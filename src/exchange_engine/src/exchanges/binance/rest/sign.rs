//! Binance REST API Authentication (HMAC-SHA256)

use anyhow::anyhow;
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
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

#[async_trait]
impl ExchangeAuthenticator for BinanceAuthenticator {
    fn generate_signature(&self, query_string: &str) -> Result<Option<SmolStr>, EngineError> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.secret_key.expose().as_bytes()).map_err(|e| {
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
        headers.insert(
            "X-MBX-APIKEY".into(),
            self.api_key.expose_smol_str().clone(),
        );
        Ok(Some(headers))
    }
}
