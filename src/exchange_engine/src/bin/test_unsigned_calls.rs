use exchange_engine::exchanges::binance::{
    config::BinanceConfig, rest::client::BinanceHttpClient, rest_client::BinanceClient,
};
use std::fs;

fn load_binance_config(file_path: &str) -> Result<BinanceConfig, Box<dyn std::error::Error>> {
    let config_data = fs::read_to_string(file_path)?;

    let config: BinanceConfig = serde_json::from_str(&config_data)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_binance_config("config/binance_config.json")?;

    let client = BinanceHttpClient::new(config.clone());
    let driver = BinanceClient::new(client, config.clone());

    match driver.get_orderbook().await {
        Ok(response) => {
            println!("Time: {:?}", response);
        }
        Err(e) => {
            eprintln!("Failed to get time: {}", e);
        }
    }

    match driver.get_time().await {
        Ok(response) => {
            println!("Time: {}", response.server_time);
        }
        Err(e) => {
            eprintln!("Failed to get time: {}", e);
        }
    }

    Ok(())
}
