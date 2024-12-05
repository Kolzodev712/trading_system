use super::error::EngineError;
use async_trait::async_trait;
use reqwest::{Method, StatusCode};
use smol_str::SmolStr;
use std::collections::HashMap;

#[async_trait]
pub trait RestExchangeHandler: Send + Sync {
    /// Builds the URL for the request, including the base URL and endpoint.
    ///
    /// # Arguments
    ///
    /// * `base_url` - The base URL for the API.
    /// * `endpoint` - The specific endpoint for the request.
    ///
    /// # Returns
    ///
    /// Returns the URL as a `String`.
    async fn build_url(&self, base_url: &str, endpoint: &str) -> SmolStr;

    /// Handles the response from the REST API.
    ///
    /// # Arguments
    ///
    /// * `rest_url` - The URL that was requested.
    /// * `status` - The HTTP status code of the response.
    /// * `text` - The response body as a string.
    ///
    /// # Returns
    ///
    /// Returns a `Result` which is `Ok(())` if the response was handled successfully or an error if not.
    async fn handle_response(
        &self,
        rest_url: &str,
        status: StatusCode,
        text: &str,
    ) -> Result<(), EngineError>;

    /// Retrieves any additional parameters needed for the request.
    ///
    /// # Returns
    ///
    /// Returns an `Option<HashMap<SmolStr, SmolStr>>` containing the parameters, or `None` if no additional parameters are needed.
    async fn get_params(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError>;

    /// Retrieves the request body, if any.
    ///
    /// # Returns
    ///
    /// Returns an `Option<SmolStr>` containing the body as a string, or `None` if there is no body.
    fn get_body(&self) -> Result<Option<HashMap<SmolStr, SmolStr>>, EngineError>;
}

/// Represents the different HTTP methods that can be used in a REST request.
pub enum HttpMethod {
    /// The HTTP GET method, used to retrieve data.
    GET,
    /// The HTTP POST method, used to submit data to be processed.
    POST,
    /// The HTTP PUT method, used to update existing data.
    PUT,
    /// The HTTP DELETE method, used to delete data.
    DELETE,
    /// The HTTP PATCH method, used to apply partial modifications to data.
    PATCH,
}

impl From<HttpMethod> for Method {
    /// Converts a `HttpMethod` to a `reqwest::Method`.
    ///
    /// # Example
    /// ```
    /// let method = HttpMethod::GET;
    /// let reqwest_method: Method = method.into();
    /// assert_eq!(reqwest_method, Method::GET);
    /// ```
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
