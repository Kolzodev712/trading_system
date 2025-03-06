use core_logic::{
    data::base_data::{OrderType, Side},
    Exchange,
};
use exchange_engine::{
    config::{ExchangeConfig, ExchangeEngineConfig},
    driver::{ExchangeEngineDriver, Method},
    exchanges::binance::{
        config::BinanceConfig,
        types::{common::BinanceTimeInForce, open_order::BinanceOrderRequest},
    },
};
use rust_decimal::Decimal;
use std::fs;
use tokio;
use tracing::error;

fn load_binance_config(file_path: &str) -> Result<BinanceConfig, Box<dyn std::error::Error>> {
    let config_data = fs::read_to_string(file_path)?;

    let config: BinanceConfig = serde_json::from_str(&config_data)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let binance_config = load_binance_config("config/binance_config.json")?;

    let exchange_config = ExchangeEngineConfig {
        exchange: Exchange::Binance,
        exchange_config: ExchangeConfig::Binance(binance_config.clone()),
    };

    let driver = ExchangeEngineDriver::new(exchange_config)?;

    let test_order = BinanceOrderRequest {
        symbol: binance_config.symbol.convert_to_smolstr(),
        side: Side::Buy.into(),
        order_type: OrderType::Limit.into(),
        quantity: Some(Decimal::new(10, 2)),
        price: Some(Decimal::new(90000, 0)),
        time_in_force: Some(BinanceTimeInForce::Gtc),
        timestamp: None,
    };

    if let Err(e) = driver.run(Method::TestOpenOrder(test_order.into())).await {
        error!("Failed to place test order: {}", e);
    }

    Ok(())
}
