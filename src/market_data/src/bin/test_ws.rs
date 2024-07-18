use core_logic::error::KolzoError;
use market_data::exchanges::binance::binance_ws::BinanceWSClient;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), KolzoError> {
    let (tx, mut rx) = mpsc::channel(100);
    let ws_url = "wss://stream.binance.com:9443/ws";

    let client = BinanceWSClient::new(ws_url);
    client.run(tx).await?;

    while let Some(message) = rx.recv().await {
        println!("Received message: {}", message);
    }

    Ok(())
}
