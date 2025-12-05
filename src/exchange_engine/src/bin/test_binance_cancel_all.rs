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
    let binance_client = Arc::new(RwLock::new(BinanceClient::new(binance_config, 100)));

    // 1) clone a lightweight handle for the WS loop, _then_ drop the lock
    let mut stream_handle = {
        let client_guard = binance_client.read().await;
        client_guard.clone()
    };
    // now spawn without holding any locks
    let ws_task = tokio::spawn(async move {
        if let Err(e) = stream_handle.stream().await {
            error!("WebSocket streaming error: {e}");
        }
    });

    // 2) wait until we have an active ConnectionSession
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
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // 3) build & send the “cancel all” RPC
    let cancel_all_params = BinanceWsCancelAllOrdersReqParams::new(
        BinanceSymbol::new("BTC", "USDT").convert_to_smolstr(),
    );
    {
        let client = binance_client.read().await;
        if let Err(e) = client.cancel_all_orders_rpc(cancel_all_params).await {
            error!("Failed to send cancel-all request: {e}");
        } else {
            info!("Cancel-all request sent");
        }
        // 4) signal the WS loop to shut down
        client.shutdown();
    }

    // 5) wait for the WS task to exit
    ws_task.await?;
    Ok(())
}
