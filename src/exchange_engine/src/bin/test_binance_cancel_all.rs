use core_logic::traits::PrivateDataClient;
use exchange_engine::exchanges::binance::{
    common_types::BinanceSymbol, config::BinanceConfig,
    websocket::types::rpc::cancel_all::BinanceWsCancelAllOrdersReqParams, ws_client::BinanceClient,
};
use std::{fs, sync::Arc};
use tokio::sync::RwLock;
use tracing::{error, info};

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

    let binance_config = load_binance_config("config/binance_config.json")?;

    let binance_client = Arc::new(RwLock::new(BinanceClient::new(binance_config.clone(), 100)));
    let binance_client_clone = Arc::clone(&binance_client);

    let ws_task = tokio::spawn(async move {
        let mut client = binance_client_clone.write().await;
        if let Err(e) = client.stream().await {
            error!("WebSocket streaming error: {}", e);
        }
    });

    // ✅ Wait until session is ready
    loop {
        if binance_client
            .read()
            .await
            .get_connection_session()
            .await
            .is_some()
        {
            info!("✅ WebSocket session ready, proceeding...");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // ✅ Build the BinanceWsCancelAllOrdersReqParams
    let cancel_all_params = BinanceWsCancelAllOrdersReqParams::new(
        BinanceSymbol::new("BTC", "USDT").convert_to_smolstr(),
    );

    // ✅ Send the cancel all orders over WS RPC
    {
        let client = binance_client.read().await;
        if let Err(e) = client.cancel_all_orders(cancel_all_params).await {
            error!("❌ Failed to send cancel all orders: {}", e);
        } else {
            println!("✅ Cancel all orders request sent successfully!");
        }
    }

    ws_task.await?;

    Ok(())
}
