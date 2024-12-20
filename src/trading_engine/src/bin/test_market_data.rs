use core_logic::Exchange;
use market_data_engine::{
    config::{ExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
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
    let binance_config = load_binance_config("config/market_data.json")?;

    let market_data_config = MarketDataConfig {
        exchange: Exchange::Binance,
        exchange_config: ExchangeConfig::Binance(binance_config),
    };

    let driver = MarketDataDriver::new(Exchange::Binance, market_data_config);

    if let Err(e) = driver.run(Method::Orderbooks).await {
        error!("Error running market data driver: {}", e);
    }

    Ok(())
}
