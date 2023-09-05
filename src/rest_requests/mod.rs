pub mod exchanges;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, Response, StatusCode};
use std::collections::HashMap;
use std::process;

/// Makes a GET request to the specified endpoint and base URL, with optional custom headers and query parameters.
///
/// # Arguments
///
/// * `endpoint` - The endpoint specific to the request.
/// * `base_url` - The base URL for the request.
/// * `headers` - Optional custom headers as a `HashMap` of key-value pairs. If `None`, default headers are used.
/// * `params` - Optional query parameters as a `HashMap` of key-value pairs. If `None`, no query parameters are included.
///
/// # Returns
///
/// Returns a `Result` containing a `reqwest::Response` if the request is successful, or an error if there was a failure.
///
/// # Errors
///
/// This function can return an error if the request fails or encounters an issue during processing.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use reqwest::Response;
///
/// async fn example() -> Result<(), Box<dyn std::error::Error>> {
///     let endpoint = "/api/v1/users";
///     let base_url = "https://api.example.com";
///
///     let mut headers = HashMap::new();
///     headers.insert("Authorization", "Bearer YOUR_TOKEN");
///
///     let params = HashMap::new();
///
///     let response = make_get_request(endpoint, base_url, Some(headers), Some(params)).await?;
///
///     // Process the response...
///
///     Ok(())
/// }
/// ```
pub async fn make_get_request(
    endpoint: &str,
    base_url: &str,
    headers: Option<HashMap<&str, &str>>,
    params: Option<HashMap<&str, &str>>,
) -> Result<Response, Box<dyn std::error::Error>> {
    let full_url = format!("{}{}", base_url, endpoint);

    let client = Client::new();
    let mut request_builder = client.get(&full_url);

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

    let response = request_builder.send().await?;
    if response.status() == StatusCode::OK {
        Ok(response)
    } else {
        println!(
            "ERROR: {} RETURNED | {:?} | {}",
            full_url,
            response.status(),
            response.text().await?,
        );
        process::exit(0);
    }
}
