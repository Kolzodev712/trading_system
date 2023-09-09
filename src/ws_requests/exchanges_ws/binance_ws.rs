use crate::parser::{Config, FileConfigParser};
use crate::ws_requests::make_ws_request;

use anyhow::Error as AnyhowError;
use tokio::net::TcpStream;

/// Establishes a WebSocket connection to Binance's orderbook stream.
///
/// This function reads from a configuration file to determine the base URL and endpoint for the Binance WebSocket connection.
/// It then makes a WebSocket request for the specified base and quote currencies, focusing on the orderbook (depth) stream.
///
/// # Arguments
///
/// * `base` - The base currency (e.g., "BTC").
/// * `quote` - The quote currency (e.g., "USDT").
/// * `stream` - The desired stream (e.g., "depth", "trade", "kline_1h").
///
/// # Returns
///
/// A `Result` which is `Ok` if it contains a successful WebSocket connection, or an `Err` containing an error message if there was an issue.
pub async fn binance_ws_stream(
    base: &str,
    quote: &str,
    stream: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
    anyhow::Error,
> {
    let file_parser = FileConfigParser::new('=');
    let config =
        Config::new(file_parser, "config.cfg").map_err(|e| AnyhowError::msg(e.to_string()))?;

    let base_data = config
        .get("binance_ws_base")
        .ok_or("Failed to get 'binance_ws_base' from config")
        .map_err(|e| AnyhowError::msg(e))?;

    let endpoint_data = config
        .get("binance_ws_endopint")
        .ok_or("Failed to get 'binance_ws_endpoint' from config")
        .map_err(|e| AnyhowError::msg(e))?;

    make_ws_request(&base_data, &endpoint_data, Some(base), Some(quote), stream)
        .await
        .map_err(|e| AnyhowError::msg(e.to_string()))
}
