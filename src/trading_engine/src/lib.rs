use core_logic::Exchange;
use market_data_engine::{
    config::{ExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
    exchanges::binance::{config::BinanceConfig, websocket::types::common::BinanceSymbol},
};

use tracing::error;

pub async fn test_binance_stream() -> Result<(), Box<dyn std::error::Error>> {
    let binance_config = BinanceConfig {
        symbol: BinanceSymbol {
            base: "BTC".into(),
            quote: "USDT".into(),
        },
        limit: 10,
    };

    let market_data_config = MarketDataConfig {
        exchange: Exchange::Binance,
        exchange_config: ExchangeConfig::Binance(binance_config),
    };

    let mut driver = MarketDataDriver::new(market_data_config);

    if let Err(e) = driver.run(Method::Orderbooks).await {
        error!("Error running market data driver: {}", e);
    }

    Ok(())
}
