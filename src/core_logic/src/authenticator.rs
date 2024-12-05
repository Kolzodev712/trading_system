use super::error::EngineError;
use async_trait::async_trait;
use smol_str::SmolStr;
use std::collections::BTreeMap;

/// Trait for authenticators, allowing the use of different signature strategies.
#[async_trait]
pub trait RestAuthenticator: Send + Sync {
    /// Generates a signature for a given query string using the provided strategy.
    /// This method is asynchronous to allow for non-blocking cryptographic operations.
    async fn generate_signature(&self, query_string: &str) -> Result<Option<SmolStr>, EngineError>;

    /// Builds signed parameters
    /// Returns a Result with either a signed HashMap of parameters or an error.
    async fn build_signed_params(
        &self,
        params: BTreeMap<SmolStr, SmolStr>,
    ) -> Result<BTreeMap<SmolStr, SmolStr>, EngineError>;

    /// Retrieves the necessary headers for authenticated requests.
    /// This method is synchronous since it usually involves simple header construction.
    fn get_headers(&self) -> Result<Option<BTreeMap<SmolStr, SmolStr>>, EngineError>;
}
