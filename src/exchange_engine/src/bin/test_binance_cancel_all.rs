use exchange_engine::exchanges::binance::{
    client::BinanceClient, config::BinanceConfig, rest::client::BinanceHttpClient,
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

    match driver.cancel_all().await {
        Ok(response) => {
            response.iter().for_each(|inner_response| {
                println!(
                    "Cancel All Orders for pair: {} was successful: {:?}",
                    inner_response.symbol, inner_response
                );
            });
        }
        Err(e) => {
            eprintln!("Failed to cancel all orders: {:?}", e);
        }
    }

    Ok(())
}
