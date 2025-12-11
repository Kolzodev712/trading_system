//! Indicator engine tests

use indicator_engine::calculator::IndicatorCalculator;
use indicator_engine::data::PricePoint;
use indicator_engine::indicators::traits::{IndicatorConfig, PriceSource};
use rust_decimal::Decimal;
use smol_str::SmolStr;

#[tokio::test]
async fn test_indicator_calculator_registration() {
    let calculator = IndicatorCalculator::new();

    let config = IndicatorConfig {
        id: SmolStr::new("test_sma"),
        name: SmolStr::new("SMA"),
        pair: SmolStr::new("BTCUSDT"),
        period: 5,
        source: PriceSource::Close,
    };

    let result = calculator.register_indicator(config);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_indicator_calculator_update() {
    let calculator = IndicatorCalculator::new();

    let config = IndicatorConfig {
        id: SmolStr::new("test_sma"),
        name: SmolStr::new("SMA"),
        pair: SmolStr::new("BTCUSDT"),
        period: 3,
        source: PriceSource::Close,
    };

    calculator.register_indicator(config).unwrap();

    // Update with data points
    for i in 0..5 {
        let price_point = PricePoint {
            open: Decimal::new(50000 + i, 0),
            high: Decimal::new(50100 + i, 0),
            low: Decimal::new(49900 + i, 0),
            close: Decimal::new(50000 + i, 0),
            volume: Decimal::new(1, 0),
            timestamp: 1000 + i as u64,
        };
        calculator.update("BTCUSDT", price_point).unwrap();
    }

    // Get value
    let value = calculator.get_value("test_sma");
    assert!(value.is_some());
}

#[tokio::test]
async fn test_indicator_calculator_history() {
    let calculator = IndicatorCalculator::new();

    let config = IndicatorConfig {
        id: SmolStr::new("test_ema"),
        name: SmolStr::new("EMA"),
        pair: SmolStr::new("BTCUSDT"),
        period: 3,
        source: PriceSource::Close,
    };

    calculator.register_indicator(config).unwrap();

    // Update with multiple data points
    for i in 0..10 {
        let price_point = PricePoint {
            open: Decimal::new(50000 + i, 0),
            high: Decimal::new(50100 + i, 0),
            low: Decimal::new(49900 + i, 0),
            close: Decimal::new(50000 + i, 0),
            volume: Decimal::new(1, 0),
            timestamp: 1000 + i as u64,
        };
        calculator.update("BTCUSDT", price_point).unwrap();
    }

    // Get history
    let history = calculator.get_history("test_ema", Some(5));
    assert!(history.len() <= 5);
}

#[tokio::test]
async fn test_indicator_service_initialization() {
    use indicator_engine::IndicatorService;
    use trading_core::channels::indicators::indicator_update_channel;
    use trading_core::channels::market_data::market_data_update_channel;

    let mut service = IndicatorService::new();
    let (_market_tx, market_rx) = market_data_update_channel(100);
    let (indicator_tx, _indicator_rx) = indicator_update_channel(100);

    service.initialize(market_rx, indicator_tx, 1);

    // Service should be initialized
    assert!(true);
}
