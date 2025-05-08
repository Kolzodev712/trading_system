use core_logic::traits::PrivateDataClient; // you can drop this import now if you like
use exchange_engine::exchanges::binance::{config::BinanceConfig, ws_client::BinanceClient};
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
    let binance_client = Arc::new(RwLock::new(BinanceClient::new(
        binance_config.clone(),
        1024,
    )));

    // 1) Spawn the WS loop:
    let ws_handle = {
        let client = Arc::clone(&binance_client);
        tokio::spawn(async move {
            let mut c = client.write().await;
            if let Err(e) = c.stream().await {
                error!("WebSocket streaming error: {}", e);
            }
        })
    };

    // 2) Spawn a Ctrl-C handler that will signal our stream to shut down:
    {
        let client = Arc::clone(&binance_client);
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl-C handler");
            info!("🛑 Ctrl-C received, shutting down WebSocket");
            // shutdown() just fires the broadcast so the loop in `run_stream_loop` will exit
            client.read().await.shutdown();
        });
    }

    // 3) Wait until the session.logon+subscribe dance completes
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

    // 4) Send one RPC over the live connection:
    {
        let client = binance_client.read().await;
        info!("🟢 Sending account status request...");
        if let Err(e) = client.account_status().await {
            error!("❌ Failed to send account status request: {}", e);
        } else {
            info!("✅ Account status request sent successfully!");
        }
    }

    // 5) Finally, wait for the WS task to actually finish (after shutdown)
    ws_handle.await?;

    Ok(())
}
