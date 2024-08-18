use super::{authenticator::RestAuthenticator, error::KolzoError};
use async_trait::async_trait;
use reqwest::{
    header::{HeaderName, HeaderValue},
    Client, Method, StatusCode,
};
use smol_str::SmolStr;
use std::{collections::HashMap, process};
use tracing::error;

#[async_trait]
pub trait RestExchangeHandler: Send + Sync {
    /// Builds the full URL for the request, including the base URL, endpoint, and any query parameters.
    ///
    /// # Arguments
    ///
    /// * `base_url` - The base URL for the API.
    /// * `endpoint` - The specific endpoint for the request.
    /// * `authenticator` - An optional authenticator used to sign the request and provide headers.
    ///
    /// # Returns
    ///
    /// Returns the full URL as a `String`.
    async fn build_full_url(
        &self,
        base_url: &str,
        endpoint: &str,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> SmolStr;

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
    ) -> Result<(), KolzoError>;

    /// Retrieves any additional headers needed for the request.
    ///
    /// # Arguments
    ///
    /// * `authenticator` - An optional authenticator that may provide additional headers.
    ///
    /// # Returns
    ///
    /// Returns an `Option<HashMap<SmolStr, SmolStr>>` containing the headers, or `None` if no additional headers are needed.
    fn get_headers(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<SmolStr, SmolStr>>, KolzoError>;

    /// Retrieves any additional parameters needed for the request.
    ///
    /// # Arguments
    ///
    /// * `authenticator` - An optional authenticator that may provide additional parameters.
    ///
    /// # Returns
    ///
    /// Returns an `Option<HashMap<SmolStr, SmolStr>>` containing the parameters, or `None` if no additional parameters are needed.
    async fn get_params(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<SmolStr, SmolStr>>, KolzoError>;

    /// Retrieves the request body, if any.
    ///
    /// # Returns
    ///
    /// Returns an `Option<SmolStr>` containing the body as a string, or `None` if there is no body.
    fn get_body(&self) -> Option<SmolStr>;

    /// Authenticates the request by adding any necessary authentication headers.
    ///
    /// # Arguments
    ///
    /// * `authenticator` - An optional authenticator used to provide authentication headers.
    ///
    /// # Returns
    ///
    /// Returns an `Option<HashMap<SmolStr, SmolStr>>` containing the authentication headers, or `None` if no authentication is needed.
    fn authenticate(
        &self,
        authenticator: Option<&dyn RestAuthenticator>,
    ) -> Result<Option<HashMap<SmolStr, SmolStr>>, KolzoError>;
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

/// Makes a REST request to the specified URL, leveraging exchange-specific logic from the handler.
///
/// This function constructs a full URL, prepares the request with optional headers and body content,
/// sends the request, and handles the response based on the exchange-specific handler.
///
/// # Arguments
///
/// * `base_url` - The base URL for the request, such as "https://api.exchange.com".
/// * `endpoint` - The specific endpoint for the request, such as "/v1/order".
/// * `method` - The HTTP method for the request, such as GET, POST, etc.
/// * `handler` - An optional handler that implements `RestExchangeHandler`, which defines
///   exchange-specific behavior for the request.
/// * `authenticator` - An optional authenticator used to sign the request and provide headers.
///
/// # Returns
///
/// Returns a `Result` containing the `SmolStr` response if the request is successful, or a `KolzoError`
/// if there was a failure during the request or response handling.
///
/// # Errors
///
/// This function can return a `KolzoError` in cases such as:
/// - Network failures or timeouts when sending the request.
/// - Invalid headers or parameters provided by the handler or authenticator.
/// - Non-success HTTP status codes returned from the API.
///
/// # Example
///
/// ```rust
/// let base_url = "https://api.exchange.com";
/// let endpoint = "/v1/order";
/// let method = HttpMethod::POST;
/// let response = create_rest_request(
///     base_url,
///     endpoint,
///     method,
///     Some(&my_exchange_handler),
///     Some(&my_authenticator)
/// ).await?;
/// println!("Response: {}", response);
/// ```
pub async fn create_rest_request<H>(
    base_url: SmolStr,
    endpoint: SmolStr,
    method: HttpMethod,
    handler: Option<&H>,
    authenticator: Option<&dyn RestAuthenticator>,
) -> Result<SmolStr, KolzoError>
where
    H: RestExchangeHandler + Send + Sync,
{
    let client = Client::new();
    let method: Method = method.into();
    let full_url = if let Some(handler) = handler {
        handler
            .build_full_url(&base_url, &endpoint, authenticator)
            .await
    } else {
        format!("{}{}", base_url, endpoint).into()
    };
    let mut request_builder = client.request(method, full_url.to_string());

    if let Some(handler) = handler {
        if let Ok(Some(auth_headers)) = handler.authenticate(authenticator) {
            for (key, value) in auth_headers {
                let header_name = HeaderName::from_bytes(key.as_bytes())
                    .map_err(|e| KolzoError::InvalidHeaderName(e))?;
                let header_value = HeaderValue::from_bytes(value.as_bytes())
                    .map_err(|e| KolzoError::InvalidHeaderValue(e))?;
                request_builder = request_builder.header(header_name, header_value);
            }
        }

        if let Ok(Some(headers)) = handler.get_headers(authenticator) {
            for (key, value) in headers {
                let header_name = HeaderName::from_bytes(key.as_bytes())
                    .map_err(|e| KolzoError::InvalidHeaderName(e))?;
                let header_value = HeaderValue::from_bytes(value.as_bytes())
                    .map_err(|e| KolzoError::InvalidHeaderValue(e))?;
                request_builder = request_builder.header(header_name, header_value);
            }
        }

        if let Some(body) = handler.get_body() {
            request_builder = request_builder.body(body.to_string());
        }
    }

    let response = request_builder.send().await?;
    let status = response.status();
    let text = response
        .text()
        .await
        .unwrap_or_else(|_| "Unknown error".into());

    if let Some(handler) = handler {
        handler.handle_response(&full_url, status, &text).await?;
    }

    if status.is_success() {
        Ok(text.into())
    } else {
        error!("ERROR: {} RETURNED | {:?} | {}", full_url, status, text);
        process::exit(1); // Set exit code to 1 to indicate an error
    }
}
