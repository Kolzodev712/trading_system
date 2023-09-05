use crate::parser::{Config, FileConfigParser};
use crate::rest_requests::make_get_request;
use serde_json::Value;
use std::collections::HashMap;

struct URLs {
    base: String,
    endpoints: String,
}

/// Retrieves active trading pairs from the Binance exchange.
///
/// # Returns
///
/// Returns a `Result` containing a `Vec<String>` with the active trading pairs if the request is successful,
/// or an error if there was a failure.
///
/// # Errors
///
/// This function can return errors if:
/// - The necessary endpoint and base URL data cannot be retrieved from the configuration file.
/// - The request to the Binance API fails.
/// - The response body cannot be parsed as JSON.
///
/// # Panics
///
/// Panics if the required keys are not found in the configuration file.
///
/// # Example
///
/// ```rust
/// match get_binance_active_pairs() {
///     Ok(pairs) => {
///         println!("Active Trading Pairs: {:?}", pairs);
///     }
///     Err(err) => {
///         println!("Error: {}", err);
///     }
/// }
/// ```
pub async fn get_binance_active_pairs() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let file_parser = FileConfigParser::new('=');
    let config = Config::new(file_parser, "config.cfg").unwrap();

    let endpoint_data = config
        .get("binance_active_pair_endpoint")
        .unwrap_or_else(|| String::new());

    let base_data = config.get("binance_base").unwrap_or_else(|| String::new());

    // Create the Struct with the endpoint name and the hashmap with endpoints:
    let exchange_data = URLs {
        base: base_data,
        endpoints: endpoint_data,
    };

    // Create the headers HashMap if there are any, otherwise leave it as None
    let mut headers: Option<HashMap<&str, &str>> = Some(HashMap::new());
    headers
        .as_mut()
        .unwrap()
        .insert("Accept", "application/json");
    headers
        .as_mut()
        .unwrap()
        .insert("Content-Type", "application/json");

    // Create the params HashMap if there are any, otherwise leave it as None
    let mut params = HashMap::new();
    params.insert("permissions", "SPOT");

    // Call the make_get_request function and fill in all arguments
    match make_get_request(
        &exchange_data.endpoints,
        &exchange_data.base,
        headers,
        Some(params),
    )
    .await
    {
        Ok(response) => {
            println!("Response Status: {}", response.status());
            let _response_body = match response.text().await {
                Ok(body) => body,
                Err(err) => {
                    // Handle the error
                    println!("Error: {}", err);
                    return Err(err.into());
                }
            };

            let exchange_response: Value = serde_json::from_str(&_response_body)?; // this is the exchange response in json format if you need it

            let pairs = if let Some(array) = exchange_response["symbols"].as_array().cloned() {
                array
                    .iter()
                    .filter_map(|pair| {
                        let base_asset = pair["baseAsset"].as_str().unwrap_or_default();
                        let quote_asset = pair["quoteAsset"].as_str().unwrap_or_default();
                        if !base_asset.is_empty() && !quote_asset.is_empty() {
                            Some(format!("{}_{}", base_asset, quote_asset))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<String>>()
            } else {
                Vec::new()
            };

            Ok(pairs)
        }
        Err(err) => {
            // Handle the error
            println!("Error: {}", err);
            Err(err.into())
        }
    }
}

/// Retrieves trades data for a specific trading pair from the Binance exchange.
///
/// # Arguments
///
/// * `base_asset` - The base asset of the trading pair.
/// * `quote_asset` - The quote asset of the trading pair.
///
/// # Returns
///
/// Returns a `Result` indicating success or failure.
///
/// # Errors
///
/// This function can return errors if:
/// - The necessary endpoint and base URL data cannot be retrieved from the configuration file.
/// - The request to the Binance API fails.
/// - The response body cannot be parsed as JSON.
///
/// # Panics
///
/// Panics if the required keys are not found in the configuration file.
///
/// # Example
///
/// ```rust
/// match get_binance_trades("BTC", "USDT") {
///     Ok(()) => {
///         println!("Successfully retrieved trades data for BTC/USDT");
///     }
///     Err(err) => {
///         println!("Error: {}", err);
///     }
/// }
/// ```
pub async fn get_binance_trades(
    base_asset: &str,
    quote_asset: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Function is going to be used to get trades data from binance.

    let file_parser = FileConfigParser::new('=');
    let config = Config::new(file_parser, "config.cfg").unwrap();

    let endpoint_data = config
        .get("binance_trades_endpoint")
        .unwrap_or_else(|| String::new());

    let base_data = config.get("binance_base").unwrap_or_else(|| String::new());

    // Create the Struct with the endpoint name and the hashmap with endpoints:
    let exchange_data = URLs {
        base: base_data,
        endpoints: endpoint_data,
    };

    // Create the headers HashMap if there are any, otherwise leave it as None
    let mut headers: Option<HashMap<&str, &str>> = Some(HashMap::new());
    headers
        .as_mut()
        .unwrap()
        .insert("Accept", "application/json");
    headers
        .as_mut()
        .unwrap()
        .insert("Content-Type", "application/json");

    // Create the params HashMap if there are any, otherwise leave it as None
    let param_string = format!(
        "{}{}",
        base_asset.to_uppercase(),
        quote_asset.to_uppercase()
    );
    let mut params = HashMap::<&str, &str>::new();
    params.insert("symbol", &param_string);

    // Call the make_get_request function and fill in all arguments
    match make_get_request(
        &exchange_data.endpoints,
        &exchange_data.base,
        headers,
        Some(params),
    )
    .await
    {
        Ok(response) => {
            println!("Response Status: {}", response.status());
            let _response_body = match response.text().await {
                Ok(body) => body,
                Err(err) => {
                    // Handle the error
                    println!("Error: {}", err);
                    return Err(err.into());
                }
            };
            // println!("{:?}", _response_body);

            Ok(())
        }
        Err(err) => {
            // Handle the error
            println!("Error: {}", err);
            Err(err.into())
        }
    }
}

/// Retrieves order book data for a specific trading pair from the Binance exchange.
///
/// # Arguments
///
/// * `base_asset` - The base asset of the trading pair.
/// * `quote_asset` - The quote asset of the trading pair.
///
/// # Returns
///
/// Returns a `Result` indicating success or failure.
///
/// # Errors
///
/// This function can return errors if:
/// - The necessary endpoint and base URL data cannot be retrieved from the configuration file.
/// - The request to the Binance API fails.
/// - The response body cannot be parsed as JSON.
///
/// # Panics
///
/// Panics if the required keys are not found in the configuration file.
///
/// # Example
///
/// ```rust
/// match get_binance_orderbooks("BTC", "USDT") {
///     Ok(()) => {
///         println!("Successfully retrieved order book data for BTC/USDT");
///     }
///     Err(err) => {
///         println!("Error: {}", err);
///     }
/// }
/// ```
pub async fn get_binance_orderbooks(
    base_asset: &str,
    quote_asset: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Function is going to be used to get ordebook data from binance.

    let file_parser = FileConfigParser::new('=');
    let config = Config::new(file_parser, "config.cfg").unwrap();

    let endpoint_data = config
        .get("binance_orderbooks_endpoint")
        .unwrap_or_else(|| String::new());

    let base_data = config.get("binance_base").unwrap_or_else(|| String::new());

    // Create the Struct with the endpoint name and the hashmap with endpoints:
    let exchange_data = URLs {
        base: base_data,
        endpoints: endpoint_data,
    };

    // Create the headers HashMap if there are any, otherwise leave it as None
    let mut headers: Option<HashMap<&str, &str>> = Some(HashMap::new());
    headers
        .as_mut()
        .unwrap()
        .insert("Accept", "application/json");
    headers
        .as_mut()
        .unwrap()
        .insert("Content-Type", "application/json");

    // Create the params HashMap if there are any, otherwise leave it as None
    let param_string = format!(
        "{}{}",
        base_asset.to_uppercase(),
        quote_asset.to_uppercase()
    );
    let mut params = HashMap::<&str, &str>::new();
    params.insert("symbol", &param_string);

    // Call the make_get_request function and fill in all arguments
    match make_get_request(
        &exchange_data.endpoints,
        &exchange_data.base,
        headers,
        Some(params),
    )
    .await
    {
        Ok(response) => {
            println!("Response Status: {}", response.status());
            let _response_body = match response.text().await {
                Ok(body) => body,
                Err(err) => {
                    // Handle the error
                    println!("Error: {}", err);
                    return Err(err.into());
                }
            };
            println!("{:?}", _response_body);
            Ok(())
        }
        Err(err) => {
            // Handle the error
            println!("Error: {}", err);
            Err(err.into())
        }
    }
}
