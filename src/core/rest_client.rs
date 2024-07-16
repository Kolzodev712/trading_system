use super::error::KolzoError;
use async_trait::async_trait;
use reqwest::{
    header::{HeaderName, HeaderValue},
    Client, Response, StatusCode,
};
use std::{collections::HashMap, process};
use tracing::error;

/// Trait for handling responses.
///
/// This trait provides a method for handling the response of an HTTP request.
#[async_trait]
pub trait ResponseHandler {
    async fn handle_response(
        &self,
        rest_url: &str,
        response: &mut Response,
    ) -> Result<(), KolzoError>;
}
/// Makes a GET request to the specified REST URL, with optional custom headers and query parameters.
///
/// # Arguments
///
/// * `rest_url` - The full REST URL for the request.
/// * `headers` - Optional custom headers as a `HashMap` of key-value pairs. If `None`, default headers are used.
/// * `params` - Optional query parameters as a `HashMap` of key-value pairs. If `None`, no query parameters are included.
/// * `handler` - The response handler to process the response.
///
/// # Returns
///
/// Returns a `Result` containing the `Response` if the request is successful and the response is handled, or a `KolzoError` if there was a failure.
///
/// # Errors
///
/// This function can return a `KolzoError` if the request fails or encounters an issue during processing.
pub async fn make_get_request<H>(
    rest_url: &str,
    headers: Option<HashMap<&str, &str>>,
    params: Option<HashMap<&str, &str>>,
    handler: H,
) -> Result<Response, KolzoError>
where
    H: ResponseHandler + Send + Sync,
{
    let client = Client::new();
    let mut request_builder = client.get(rest_url);

    if let Some(headers) = headers {
        for (key, value) in headers {
            let header_name = HeaderName::from_bytes(key.as_bytes())?;
            let header_value = HeaderValue::from_bytes(value.as_bytes())?;
            request_builder = request_builder.header(header_name, header_value);
        }
    }

    if let Some(params) = params {
        request_builder = request_builder.query(&params);
    }

    let mut response = request_builder.send().await?;

    match response.status() {
        StatusCode::OK => {
            handler.handle_response(rest_url, &mut response).await?;
            Ok(response)
        }
        status => {
            error!(
                "ERROR: {} RETURNED | {:?} | {}",
                rest_url,
                status,
                response.text().await?,
            );
            process::exit(1); // Set exit code to 1 to indicate an error
        }
    }
}
