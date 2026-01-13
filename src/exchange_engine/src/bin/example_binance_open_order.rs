//! Example: Place an order on Binance via WebSocket
//!
//! This example demonstrates how to:
//! 1. Connect to Binance WebSocket
//! 2. Place a test order
//! 3. Handle order responses
//!
//! **Note**: This requires valid Binance API keys in `config/binance_config.json`
//! **Warning**: This will place a real test order on Binance testnet

use exchange_engine::exchanges::binance::{
    common::types::{BinanceOrderType, BinanceSide, BinanceSymbol, BinanceTimeInForce},
    config::BinanceConfig,
    websocket::{types::rpc::open_order::BinanceWsOrderReqParams, ws_client::BinanceClient},
};
use rust_decimal::Decimal;
use std::{fs, sync::Arc};
use tokio::sync::RwLock;
use tracing::{error, info};
use trading_core::traits::PrivateDataClient;

fn load_binance_config(file_path: &str) -> Result<BinanceConfig, Box<dyn std::error::Error>> {
    let config_data = fs::read_to_string(file_path)?;
    let config: BinanceConfig = serde_json::from_str(&config_data)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let cfg = load_binance_config("config/binance_config.json")?;
    // Use default channel config for examples
    let channels = trading_core::config::ChannelConfig::default();
    let binance_client = Arc::new(RwLock::new(BinanceClient::new(
        cfg,
        channels.websocket_inbound_capacity,
        channels.websocket_ping_capacity,
        channels.websocket_outbound_rpc_capacity,
        channels.shutdown_capacity,
    )));

    // -- spawn the WS loop on a *clone* of the client, not on the locked instance --
    let mut ws_client_clone = {
        let r = binance_client.read().await;
        r.clone()
    };
    let ws_handle = tokio::spawn(async move {
        if let Err(e) = ws_client_clone.stream().await {
            error!("WebSocket streaming error: {e}");
        }
    });

    // -- wait until the RPC channel is ready --
    loop {
        let rpc_ready = {
            let client = binance_client.read().await;
            client.rpc_client().get_rpc_request_tx().await.is_some()
        };
        if rpc_ready {
            info!("WebSocket RPC channel ready, proceeding...");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // -- Get current market price --
    // Note: In production, market prices should come from the Market Data Engine via WebSocket.
    // For this example, we'll use a hardcoded price. The WebSocket stream provides orderbook
    // snapshots automatically, so REST orderbook fetching is not needed.
    let market_price = Decimal::new(6000000, 2); // 60,000 USDT (example price)

    // Use 1% below market price for buy order to ensure it's valid
    // Round to 2 decimal places for BTC/USDT price filter
    let test_price_raw = market_price * Decimal::new(99, 2) / Decimal::new(100, 2);
    let test_price = (test_price_raw * Decimal::new(100, 0)).round_dp(0) / Decimal::new(100, 0);
    let test_quantity = Decimal::new(1, 3); // 0.001 BTC (small quantity for testing)

    info!(
        "Market price: {market_price}, Using test price: {test_price}, Quantity: {test_quantity}"
    );

    // -- now we can send RPCs freely --
    let params = BinanceWsOrderReqParams::new(
        BinanceSymbol::new("BTC", "USDT").convert_to_smolstr(),
        BinanceSide::Buy,
        BinanceOrderType::Limit,
        Some(BinanceTimeInForce::Gtc),
        Some(test_price),
        Some(test_quantity),
    );

    {
        let client = binance_client.read().await;
        match client.test_order(params).await {
            Ok(_) => info!("Test order sent successfully!"),
            Err(e) => error!("Failed to send test order: {e}"),
        }
    }

    // wait for CTRL-C or whatever
    tokio::signal::ctrl_c().await?;
    // optionally signal the client to shut down cleanly:
    {
        let client = binance_client.read().await;
        client.shutdown();
    }
    // wait for the websocket task to finish
    ws_handle.await?;

    Ok(())
}
