//! Authentication trait for exchanges
use crate::error::EngineError;
use smol_str::SmolStr;
use std::collections::BTreeMap;

/// Trait for authenticators, allowing the use of different signature strategies.
pub trait ExchangeAuthenticator: Send + Sync {
    /// Generates a signature for a given query string using the provided strategy.
    /// This method is asynchronous to allow for non-blocking cryptographic operations.
    fn generate_signature(&self, payload: &str) -> Result<Option<SmolStr>, EngineError>;

    /// Builds signed parameters
    /// Returns a Result with either a signed HashMap of parameters or an error.
    fn build_signed_params(
        &self,
        params: BTreeMap<SmolStr, SmolStr>,
    ) -> Result<BTreeMap<SmolStr, SmolStr>, EngineError>;

    /// Retrieves the necessary headers for authenticated requests.
    /// This method is synchronous since it usually involves simple header construction.
    fn get_headers(&self) -> Result<Option<BTreeMap<SmolStr, SmolStr>>, EngineError> {
        Ok(None)
    }
}
