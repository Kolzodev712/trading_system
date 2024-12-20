use core_logic::{
    data::base_data::{OrderType, Side},
    Exchange,
};
use exchange_engine::{
    config::{ExchangeConfig, ExchangeEngineConfig},
    driver::{ExchangeEngineDriver, Method},
    exchanges::binance::{
        config::BinanceConfig,
        types::{
            common::{BinanceSymbol, BinanceTimeInForce},
            open_order::BinanceOrderRequest,
        },
    },
};
use rust_decimal::Decimal;
use tokio;
use tracing::error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let binance_config = BinanceConfig {
        symbol: BinanceSymbol {
            base: "BTC".into(),
            quote: "USDT".into(),
        },
        api_key: "YQkiaZqcvbLdtqJ2V50xzkq2y44vX8HLCxALeQeSn35eyd0UpCdG6hhHS68L6GFI".into(),
        secret_key: "IVhqo9ZdafbgdxGmXex6tpx77JqmTKwp57dadWv06VIk4Ce4WNQBQRM3cKqkXMCn".into(),
        limit: None,
    };

    let exchange_config = ExchangeEngineConfig {
        exchange: Exchange::Binance,
        exchange_config: ExchangeConfig::Binance(binance_config.clone()),
    };

    let driver = ExchangeEngineDriver::new(Exchange::Binance, exchange_config);

    let test_order = BinanceOrderRequest {
        symbol: binance_config.symbol.convert_to_smolstr(),
        side: Side::Buy.into(),
        order_type: OrderType::Limit.into(),
        quantity: Some(Decimal::new(10, 2)),
        price: Some(Decimal::new(90000, 0)),
        time_in_force: Some(BinanceTimeInForce::Gtc),
        timestamp: None,
    };

    if let Err(e) = driver.run(Method::OpenOrder(test_order)).await {
        println!("SHIIT");
        error!("Failed to place test order: {}", e);
    }

    Ok(())
}
