//! Example: Cancel an order by ID on Binance
//!
//! This example demonstrates how to:
//! 1. Connect to Binance WebSocket
//! 2. Cancel a specific order by its ID
//!
//! **Note**: This requires valid Binance API keys in `config/binance_config.json`

use exchange_engine::exchanges::binance::{
    client::BinanceClient, common_types::BinanceSymbol, config::BinanceConfig,
    websocket::types::rpc::cancel_order::BinanceWsCancelOrderReqParams,
};
use smol_str::ToSmolStr;
use std::{fs, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info};
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

    // 1) build the shared client
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

    // 2) clone a snapshot for streaming, _then_ drop the lock and run
    let mut stream_client = {
        let guard = binance_client.read().await;
        guard.clone()
    };
    let ws_handle = tokio::spawn(async move {
        if let Err(e) = stream_client.stream().await {
            error!("WebSocket streaming error: {e}");
        }
    });

    // 3) wait for the RPC channel to be ready
    loop {
        let rpc_ready = {
            let client = binance_client.read().await;
            client.rpc_client().get_rpc_request_tx().await.is_some()
        };
        if rpc_ready {
            info!("WebSocket RPC channel ready");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // 4) build and send your cancel‐order RPC
    let params = BinanceWsCancelOrderReqParams::new_with_client_order_id(
        BinanceSymbol::new("BTC", "USDT").convert_to_smolstr(),
        "myClientOrderId".to_smolstr(),
    );
    {
        let client = binance_client.read().await;
        debug!("Sending cancel-order request: {params:?}");
        match client.cancel_order_rpc(params).await {
            Ok(_) => info!("Cancel order request sent!"),
            Err(e) => error!("Failed to send cancel order: {e}"),
        }
    }

    // 5) wait for Ctrl-C → fire the shutdown broadcast
    tokio::signal::ctrl_c().await?;
    info!("Ctrl-C received, shutting down WS client");
    {
        let client = binance_client.read().await;
        client.shutdown();
    }

    // 6) finally await the WS task’s exit
    let _ = ws_handle.await;
    info!("Goodbye");
    Ok(())
}
