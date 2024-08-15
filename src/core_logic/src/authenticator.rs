use super::error::KolzoError;
use async_trait::async_trait;
use std::collections::HashMap;

/// Trait for authenticators, allowing the use of different signature strategies.
#[async_trait]
pub trait RestAuthenticator: Send + Sync {
    /// Generates a signature for a given query string using the provided strategy.
    /// This method is asynchronous to allow for non-blocking cryptographic operations.
    async fn generate_signature(&self, query_string: &str) -> Result<Option<String>, KolzoError>;

    /// Builds signed parameters, potentially including a timestamp and receive window.
    /// Returns a Result with either a signed HashMap of parameters or an error.
    async fn build_signed_params(
        &self,
        params: HashMap<String, String>,
        recv_window: Option<u64>,
        timestamp: Option<u64>,
    ) -> Result<HashMap<String, String>, KolzoError>;

    /// Retrieves the necessary headers for authenticated requests.
    /// This method is synchronous since it usually involves simple header construction.
    fn get_headers(&self) -> Result<Option<HashMap<String, String>>, KolzoError>;
}
