use crate::parser::{Config, FileConfigParser};
use crate::ws_requests::make_ws_request;

use anyhow::Error as AnyhowError;
use tokio::net::TcpStream;

pub async fn binance_orderbook_ws_stream(
    base: &str,
    quote: &str,
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

    make_ws_request(&base_data, &endpoint_data, base, quote, "depth")
        .await
        .map_err(|e| AnyhowError::msg(e.to_string()))
}

pub async fn binance_trades_ws_stream(
    base: &str,
    quote: &str,
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

    make_ws_request(&base_data, &endpoint_data, base, quote, "trades")
        .await
        .map_err(|e| AnyhowError::msg(e.to_string()))
}
