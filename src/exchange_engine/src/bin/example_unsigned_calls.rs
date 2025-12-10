//! Example: Make unsigned REST API calls to Binance
//!
//! This example demonstrates how to:
//! 1. Use Binance REST client for public endpoints
//! 2. Fetch server time without authentication
//!
//! **Note**: This does NOT require API keys (uses public endpoints)
//! **Note**: Orderbook data should come from Market Data Engine via WebSocket, not REST API

use exchange_engine::exchanges::binance::{
    config::BinanceConfig, rest::client::BinanceHttpClient, rest_client::BinanceRestClient,
};
use std::fs;

fn load_binance_config(file_path: &str) -> Result<BinanceConfig, Box<dyn std::error::Error>> {
    let config_data = fs::read_to_string(file_path)?;

    let config: BinanceConfig = serde_json::from_str(&config_data)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_binance_config("config/binance_config.json")?;

    let client = BinanceHttpClient::new(config.clone());
    let driver = BinanceRestClient::new(client);

    // Note: Orderbook fetching has been removed from exchange engine.
    // Market data (orderbooks, trades) should come from Market Data Engine via WebSocket.
    // The WebSocket stream automatically provides orderbook snapshots, so REST fetching is redundant.

    match driver.get_time().await {
        Ok(response) => {
            println!("Time: {}", response.server_time);
        }
        Err(e) => {
            eprintln!("Failed to get time: {}", e);
        }
    }

    Ok(())
}
