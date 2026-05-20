//! Example: Cancel an order by ID on Bybit
//!
//! This example demonstrates how to:
//! 1. Create a Bybit client
//! 2. Cancel a specific order by its ID via WebSocket RPC
//!
//! **Note**: This requires valid Bybit API keys in `config/bot.json` (exchange_engine.config)
//! **Usage**: Pass the order ID as a command-line argument: `cargo run --bin example_bybit_cancel_by_id -- <order_id>`

use exchange_engine::exchanges::bybit::{client::BybitClient, config::BybitConfig};
use smol_str::SmolStr;
use std::{env, fs, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use trading_core::types::common::OrderId;
use trading_core::venue::cex::traits::TradingClient;

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

    // Get order ID from command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <order_id>", args[0]);
        eprintln!("Example: {} abc123def456", args[0]);
        std::process::exit(1);
    }
    let order_id_str = &args[1];
    let order_id = OrderId::new(SmolStr::new(order_id_str));

    // 1) build the shared client
    let cfg = load_bybit_config()?;
    // Use default channel config for examples
    let channels = trading_core::config::ChannelConfig::default();
    let bybit_client = Arc::new(RwLock::new(BybitClient::new(
        cfg,
        channels.websocket_inbound_capacity,
        channels.websocket_ping_capacity,
        channels.websocket_outbound_rpc_capacity,
        channels.shutdown_capacity,
    )));

    // 2) build and send your cancel-order RPC
    {
        let client = bybit_client.read().await;
        debug!("Sending cancel-order request for order ID: {:?}", order_id);
        match client.cancel_order_by_id(order_id.clone()).await {
            Ok(_) => info!(
                "Cancel order request sent successfully for order ID: {:?}",
                order_id
            ),
            Err(e) => error!("Failed to send cancel order: {e}"),
        }
    }

    info!("Goodbye");
    Ok(())
}
