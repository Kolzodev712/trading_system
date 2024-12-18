use core_logic::Exchange;
use market_data_engine::{
    config::{ExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
    exchanges::binance::{config::BinanceConfig, websocket::types::common::BinanceSymbol},
};
use tracing::error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let driver = MarketDataDriver::new(Exchange::Binance, market_data_config);

    if let Err(e) = driver.run(Method::Trades).await {
        error!("Error running market data driver: {}", e);
    }

    Ok(())
}
