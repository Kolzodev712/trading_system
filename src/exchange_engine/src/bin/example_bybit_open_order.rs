//! Example: Place an order on Bybit via WebSocket RPC
//!
//! This example demonstrates how to:
//! 1. Create a Bybit client
//! 2. Place a test order via WebSocket RPC
//! 3. Handle order responses
//!
//! **Note**: This requires valid Bybit API keys in `config/bot.json` (exchange_engine.config)
//! **Warning**: This will place a real test order on Bybit

use exchange_engine::exchanges::bybit::{client::BybitClient, config::BybitConfig};
use rust_decimal::Decimal;
use std::{fs, sync::Arc};
use tokio::sync::RwLock;
use tracing::{error, info};
use trading_core::types::{
    common::{Instrument, OrderType, Side},
    exchange::OrderRequest,
};
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

    let cfg = load_bybit_config()?;
    // Use default channel config for examples
    let channels = trading_core::config::ChannelConfig::default();
    let bybit_client = Arc::new(RwLock::new(BybitClient::new(
        cfg.clone(),
        channels.websocket_inbound_capacity,
        channels.websocket_ping_capacity,
        channels.websocket_outbound_rpc_capacity,
        channels.shutdown_capacity,
    )));

    // Get the first pair from config for testing
    let test_pair = cfg.pairs.first().ok_or("No pairs configured")?;

    // Use a test price (1% below a reasonable market price)
    // For ETH/USDC, using 3000 as example market price
    let market_price = Decimal::new(300000, 2); // 3000.00 USDC
    let test_price = market_price * Decimal::new(99, 2) / Decimal::new(100, 2);
    let test_quantity = Decimal::new(1, 2); // 0.01 ETH (small quantity for testing)

    info!(
        "Market price: {market_price}, Using test price: {test_price}, Quantity: {test_quantity}"
    );

    // Create order request
    let order_request = OrderRequest {
        instrument: Instrument::new(cfg.instrument_type.clone(), test_pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: test_price,
        amount: test_quantity,
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };

    {
        let client = bybit_client.read().await;
        match client.test_open_order(order_request.clone()).await {
            Ok(_) => info!("Test order sent successfully!"),
            Err(e) => error!("Failed to send test order: {e}"),
        }
    }

    // For a real order (commented out for safety):
    // {
    //     let client = bybit_client.read().await;
    //     match client.open_order(order_request).await {
    //         Ok(order) => info!("Order placed successfully! Order ID: {}", order.order_id),
    //         Err(e) => error!("Failed to place order: {e}"),
    //     }
    // }

    Ok(())
}
