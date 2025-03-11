use anyhow::{Context, Result};
use futures_util::stream::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinanceWSMessage {
    pub method: String,
    pub params: Vec<String>,
    pub id: String,
}

// Function to create a listenKey by calling the REST API
async fn create_listen_key(api_key: &str) -> Result<String> {
    let client = Client::new();
    let url = "https://api.binance.com/api/v3/userDataStream";
    let res = client
        .post(url)
        .header("X-MBX-APIKEY", api_key)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let listen_key = res["listenKey"]
        .as_str()
        .context("Missing listenKey in response")?
        .to_string();

    Ok(listen_key)
}

// Function to authenticate and listen to WebSocket stream
async fn authenticate_ws(listen_key: &str) -> Result<()> {
    let ws_url = format!("wss://stream.binance.com:9443/ws/{}", listen_key);
    let (ws_stream, _) = connect_async(&ws_url).await?;
    let (_, mut read) = ws_stream.split();

    // Listen for messages
    while let Some(message) = read.next().await {
        match message {
            Ok(Message::Text(text)) => {
                debug!("Received WebSocket message: {}", text);
                // Here you can process the balance and other updates
            }
            Err(e) => {
                error!("Error during WebSocket communication: {}", e);
            }
            _ => {}
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let api_key = "BerLhwYn5pSuG2XFy8KkCfO9HUklWiPvuAUtgAqeLFJx9T0VgZdnaUDj5yyBHknZ";

    // Step 1: Create listenKey by making the REST API call
    let listen_key = create_listen_key(api_key).await?;

    // Step 2: Authenticate using listenKey and start WebSocket stream
    authenticate_ws(&listen_key).await?;

    Ok(())
}
