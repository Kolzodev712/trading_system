//! Example: Cancel an order by ID on Binance
//!
//! This example demonstrates how to:
//! 1. Connect to Binance WebSocket
//! 2. Cancel a specific order by its ID
//!
//! **Note**: This requires valid Binance API keys in `config/binance_config.json`

use core_logic::traits::PrivateDataClient;
use exchange_engine::exchanges::binance::{
    common_types::{BinanceOrderType, BinanceSymbol},
    config::BinanceConfig,
    websocket::types::rpc::cancel_order::BinanceWsCancelOrderReqParams,
    ws_client::BinanceClient,
};
use smol_str::ToSmolStr;
use std::{fs, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

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
    let binance_client = Arc::new(RwLock::new(BinanceClient::new(cfg, 100)));

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

    // 3) wait for the in‐memory session to be populated
    loop {
        if binance_client
            .read()
            .await
            .get_connection_session()
            .await
            .is_some()
        {
            info!("WebSocket session ready");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // 4) build and send your cancel‐order RPC
    let params = BinanceWsCancelOrderReqParams::new(
        BinanceSymbol::new("BTC", "USDT").convert_to_smolstr(),
        "myClientOrderId".to_smolstr(),
        BinanceOrderType::Limit,
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
