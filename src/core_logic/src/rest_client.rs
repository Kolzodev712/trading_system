use super::error::KolzoError;
use async_trait::async_trait;
use reqwest::{
    header::{HeaderName, HeaderValue},
    Client, Method, StatusCode,
};
use std::{collections::HashMap, process};
use tracing::error;

#[async_trait]
pub trait RestExchangeHandler {
    fn build_full_url(&self, base_url: &str, endpoint: &str) -> String {
        let mut full_url = format!("{}{}", base_url, endpoint);

        if let Some(params) = self.get_params() {
            let query_string: String = params
                .into_iter()
                .map(|(key, value)| format!("{}={}", key, value))
                .collect::<Vec<String>>()
                .join("&");
            full_url = format!("{}?{}", full_url, query_string);
        }

        full_url
    }

    async fn handle_response(
        &self,
        rest_url: &str,
        status: StatusCode,
        text: &str,
    ) -> Result<(), KolzoError>;

    fn get_headers(&self) -> Option<HashMap<String, String>>;
    fn get_params(&self) -> Option<HashMap<String, String>>;
    fn get_body(&self) -> Option<String>;
}

/// Enum for HTTP methods.
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

/// Makes a REST request to the specified URL, leveraging exchange-specific logic from the handler.
///
/// # Arguments
///
/// * `base_url` - The base URL for the request.
/// * `endpoint` - The specific endpoint for the request.
/// * `method` - The HTTP method for the request.
/// * `handler` - The exchange-specific handler to process the request and response.
///
/// # Returns
///
/// Returns a `Result` containing the `Response` if the request is successful, or a `KolzoError` if there was a failure.
///
/// # Errors
///
/// This function can return a `KolzoError` if the request fails or encounters an issue during processing.
pub async fn make_rest_request<H>(
    base_url: &str,
    endpoint: &str,
    method: HttpMethod,
    handler: Option<&H>,
) -> Result<String, KolzoError>
where
    H: RestExchangeHandler + Send + Sync,
{
    let client = Client::new();
    let method: Method = method.into();
    let full_url = handler.map_or_else(
        || format!("{}{}", base_url, endpoint),
        |h| h.build_full_url(base_url, endpoint),
    );
    let mut request_builder = client.request(method, &full_url);

    if let Some(handler) = handler {
        if let Some(headers) = handler.get_headers() {
            for (key, value) in headers {
                let header_name = HeaderName::from_bytes(key.as_bytes())?;
                let header_value = HeaderValue::from_bytes(value.as_bytes())?;
                request_builder = request_builder.header(header_name, header_value);
            }
        }

        if let Some(body) = handler.get_body() {
            request_builder = request_builder.body(body);
        }
    }

    let response = request_builder.send().await?;
    let status = response.status();
    let text = response
        .text()
        .await
        .unwrap_or_else(|_| "Unknown error".to_string());

    if let Some(handler) = handler {
        handler.handle_response(&full_url, status, &text).await?;
    }

    if status.is_success() {
        Ok(text)
    } else {
        error!("ERROR: {} RETURNED | {:?} | {}", full_url, status, text);
        process::exit(1); // Set exit code to 1 to indicate an error
    }
}
