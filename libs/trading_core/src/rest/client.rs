//! Generic REST client infrastructure
use crate::error::EngineError;
use async_trait::async_trait;
use reqwest::{Method, StatusCode};
use smol_str::SmolStr;
use std::collections::HashMap;

/// Trait for handling REST API requests
#[async_trait]
pub trait RestExchangeHandler: Send + Sync {
    /// Builds the URL for the request, including the base URL and endpoint.
    async fn build_url(&self, base_url: &str, endpoint: &str) -> SmolStr;

    /// Handles the response from the REST API.
    async fn handle_response(
        &self,
        rest_url: &str,
        status: StatusCode,
        text: &str,
    ) -> Result<(), EngineError>;

    /// Retrieves any additional parameters needed for the request.
    async fn get_params(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError>;

    /// Retrieves the request body, if any.
    fn get_body(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError>;
}

/// HTTP method wrapper
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
}

impl From<HttpMethod> for Method {
    fn from(method: HttpMethod) -> Self {
        match method {
            HttpMethod::GET => Method::GET,
            HttpMethod::POST => Method::POST,
            HttpMethod::PUT => Method::PUT,
            HttpMethod::DELETE => Method::DELETE,
            HttpMethod::PATCH => Method::PATCH,
        }
    }
}
