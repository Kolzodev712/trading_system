pub mod config;

use market_data_engine::exchanges::binance::{
    client::BinanceClient, config::BinanceConfig, websocket::types::common::BinanceSymbol,
};
use tracing::error;

pub async fn test_binance_stream() -> Result<(), Box<dyn std::error::Error>> {
    let config = BinanceConfig {
        symbol: BinanceSymbol {
            base: "BTC".into(),
            quote: "USDT".into(),
        },
        limit: 10,
    };

    let mut client = BinanceClient::new(config, 100);

    let computation = 2 + 2;
    println!("Some trading computation: {}", computation);
    if let Err(e) = client.orderbook().await {
        error!("Error in orderbook stream: {}", e);
    }

    Ok(())
}
