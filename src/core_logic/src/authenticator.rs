use super::error::KolzoError;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait RestAuthenticator {
    fn generate_signature(&self, query_string: &str) -> Option<String>;
    fn build_signed_params(
        &self,
        params: HashMap<String, String>,
        recv_window: Option<u64>,
        timestamp: Option<u64>,
    ) -> HashMap<String, String>;
    fn get_headers(&self) -> Option<HashMap<String, String>>;
}

#[async_trait]
pub trait WsAuthenticator: Send + Sync {
    async fn authenticate(&self, ws_url: &str) -> Result<String, KolzoError>;
    fn sign_subscription_message(&self, message: &str) -> Option<String>;
    fn get_auth_headers(&self) -> Option<HashMap<String, String>>;
}
