//! Example: Cancel all orders for a trading pair on Bybit
//!
//! This example demonstrates how to:
//! 1. Create a Bybit client
//! 2. Cancel all open orders for a specific trading pair via WebSocket RPC
//!
//! **Note**: This requires valid Bybit API keys in `config/bot.json` (exchange_engine.config)
//! **Warning**: This will cancel ALL open orders for the configured trading pair

use exchange_engine::exchanges::bybit::{client::BybitClient, config::BybitConfig};
use std::{fs, sync::Arc};
use tokio::sync::RwLock;
use tracing::{error, info};
use trading_core::traits::TradingClient;

fn load_bybit_config() -> Result<BybitConfig, Box<dyn std::error::Error>> {
    // Load from bot.json which has the new structure
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
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let bybit_config = load_bybit_config()?;
    // Use default channel config for examples
    let channels = trading_core::config::ChannelConfig::default();
    let bybit_client = Arc::new(RwLock::new(BybitClient::new(
        bybit_config.clone(),
        channels.websocket_inbound_capacity,
        channels.websocket_ping_capacity,
        channels.websocket_outbound_rpc_capacity,
        channels.shutdown_capacity,
    )));

    // Get the first pair from config
    let test_pair = bybit_config.pairs.first().ok_or("No pairs configured")?;
    info!("Canceling all orders for pair: {}", test_pair);

    // Cancel all orders for the pair
    {
        let client = bybit_client.read().await;
        match client.cancel_all_orders(test_pair).await {
            Ok(_) => info!("Cancel-all request sent successfully!"),
            Err(e) => error!("Failed to send cancel-all request: {e}"),
        }
    }

    Ok(())
}
