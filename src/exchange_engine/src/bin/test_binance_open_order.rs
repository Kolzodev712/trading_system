use exchange_engine::exchanges::binance::{
    client::BinanceDriver,
    config::BinanceConfig,
    rest::http_client::BinanceHttpClient,
    types::{
        common::{BinanceOrderType, BinanceSide, BinanceTimeInForce},
        open_order::BinanceOrderRequest,
    },
};
use rust_decimal::Decimal;
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
    let driver = BinanceDriver::new(client, config.clone());

    let time = driver.get_time().await?;

    let test_order = BinanceOrderRequest {
        symbol: config.symbol.convert_to_smolstr(),
        side: BinanceSide::Buy,
        order_type: BinanceOrderType::Limit,
        quantity: Some(Decimal::new(10, 2)),
        price: Some(Decimal::new(90000, 0)),
        time_in_force: Some(BinanceTimeInForce::Gtc),
        timestamp: time.server_time,
    };

    match driver.open_order_test(test_order).await {
        Ok(response) => {
            println!("Test order placed successfully: {:?}", response);
        }
        Err(e) => {
            eprintln!("Failed to place test order: {}", e);
        }
    }

    Ok(())
}
