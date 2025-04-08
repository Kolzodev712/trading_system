use core_logic::Exchange;
use exchange_engine::{
    config::{ExchangeConfig, ExchangeEngineConfig},
    driver::{ExchangeEngineDriver, Method},
    exchanges::binance::config::BinanceConfig,
};
use std::fs;
use tracing::error;

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

    let exchange_config = ExchangeEngineConfig {
        exchange: Exchange::Binance,
        exchange_config: ExchangeConfig::Binance(binance_config.clone()),
    };

    let driver = ExchangeEngineDriver::new(exchange_config)?;

    if let Err(e) = driver.run(Method::FetchBalance).await {
        error!("Failed to place test order: {}", e);
    }

    Ok(())
}
