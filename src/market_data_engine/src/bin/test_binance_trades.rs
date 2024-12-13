use market_data_engine::exchanges::binance::{client::BinanceClient, config::BinanceConfig};
use std::fs;
use tracing::error;

fn load_binance_config(file_path: &str) -> Result<BinanceConfig, Box<dyn std::error::Error>> {
    let config_data = fs::read_to_string(file_path)?;

    let config: BinanceConfig = serde_json::from_str(&config_data)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_binance_config("config/binance_config.json")?;
    let client = BinanceClient::new(config);

    if let Err(e) = client.trades().await {
        error!("Error in orderbook stream: {}", e);
    }

    Ok(())
}
