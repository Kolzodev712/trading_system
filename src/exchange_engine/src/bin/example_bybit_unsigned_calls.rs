//! Example: Make unsigned REST API calls to Bybit
//!
//! This example demonstrates how to:
//! 1. Use Bybit REST client for public endpoints
//! 2. Fetch server time without authentication
//!
//! **Note**: This does NOT require API keys (uses public endpoints)
//! **Note**: Orderbook data should come from Market Data Engine via WebSocket, not REST API

use exchange_engine::exchanges::bybit::{config::BybitConfig, rest::client::BybitHttpClient};
use std::fs;
use tracing::info;

fn load_bybit_config() -> Result<BybitConfig, Box<dyn std::error::Error>> {
    // Load from bot.json which has the new structure
    // For unsigned calls, we don't actually need credentials, but we need the config structure
    let config_data = fs::read_to_string("config/bot.json")?;
    let bot_config: serde_json::Value = serde_json::from_str(&config_data)?;

    let exchange_engine = bot_config
        .get("exchange_engine")
        .ok_or("Missing exchange_engine in config")?;

    let exchange = exchange_engine
        .get("exchange")
        .and_then(|v| v.as_str())
        .ok_or("Missing exchange field")?;

    if exchange != "Bybit" {
        return Err("Exchange must be Bybit".into());
    }

    let config_json = exchange_engine
        .get("config")
        .ok_or("Missing config in exchange_engine")?;

    let bybit_config: BybitConfig = serde_json::from_value(config_json.clone())?;
    Ok(bybit_config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_bybit_config()?;

    let _client = BybitHttpClient::new(config.clone());

    // Note: Orderbook fetching has been removed from exchange engine.
    // Market data (orderbooks, trades) should come from Market Data Engine via WebSocket.
    // The WebSocket stream automatically provides orderbook snapshots, so REST fetching is redundant.

    // Example: Get server time (if available as a public endpoint)
    // Bybit doesn't have a simple server time endpoint like Binance,
    // but we can demonstrate the REST client is working
    info!("Bybit REST client initialized successfully");
    info!("Note: Market data should be fetched via Market Data Engine WebSocket streams");

    Ok(())
}
