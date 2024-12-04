use anyhow::anyhow;
use core_logic::{
    authenticator::RestAuthenticator, error::EngineError, utils::generate_string_params,
};

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use smol_str::SmolStr;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct BinanceAuthenticator {
    pub api_key: SmolStr,
    pub secret_key: SmolStr,
}

#[async_trait]
impl RestAuthenticator for BinanceAuthenticator {
    async fn generate_signature(&self, query_string: &str) -> Result<Option<SmolStr>, EngineError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret_key.as_bytes()).map_err(|e| {
            EngineError::AuthenticationError(anyhow!(format!("Invalid mac lenght: {e}")))
        })?;
        mac.update(query_string.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        Ok(Some(signature.into()))
    }

    async fn build_signed_params(
        &self,
        mut params: HashMap<SmolStr, SmolStr>,
    ) -> Result<HashMap<SmolStr, SmolStr>, EngineError> {
        let query_string: String = serde_qs::to_string(&generate_string_params(&mut params))
            .map_err(|e| EngineError::JsonQueryStringError(e.into()))?;
        if let Some(signature) = self.generate_signature(&query_string).await? {
            params.insert("signature".into(), signature);
        }
        Ok(params)
    }

    fn get_headers(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError> {
        let mut headers = HashMap::new();
        headers.insert("X-MBX-APIKEY".into(), self.api_key.clone());
        Ok(Some(headers))
    }
}
