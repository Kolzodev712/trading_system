//! Level execution and resolver tests

use indicator_engine::data::IndicatorValue;
use rust_decimal_macros::dec;
use smol_str::SmolStr;
use std::collections::HashMap;
use trading_core::types::common::Pair;
use trading_core::types::market_data::Orderbook;
use trading_engine::levels::config::PriceConfig;
use trading_engine::levels::resolver::IndicatorResolver;

#[tokio::test]
async fn test_indicator_resolver_fixed_price() {
    let resolver = IndicatorResolver::new(
        None,
        trading_core::types::common::Exchange::Binance,
        dec!(0.01),
        dec!(0.001),
    );

    let indicators: HashMap<SmolStr, IndicatorValue> = HashMap::new();
    let pair = Pair::new("BTC", "USDT");

    let price_config = PriceConfig::Fixed(dec!(50000));

    let result = resolver
        .resolve_price(&price_config, &indicators, None, &pair)
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), dec!(50000));
}

#[tokio::test]
async fn test_indicator_resolver_orderbook_price() {
    let resolver = IndicatorResolver::new(
        None,
        trading_core::types::common::Exchange::Binance,
        dec!(0.01),
        dec!(0.001),
    );

    let indicators: HashMap<SmolStr, IndicatorValue> = HashMap::new();
    let pair = Pair::new("BTC", "USDT");

    let orderbook = Orderbook {
        pair: pair.clone(),
        bids: vec![(dec!(50000), dec!(1.0))],
        asks: vec![(dec!(50100), dec!(1.0))],
        timestamp: 1000,
    };

    let price_config = PriceConfig::MidPrice {
        offset_percent: dec!(0),
    };

    let result = resolver
        .resolve_price(&price_config, &indicators, Some(&orderbook), &pair)
        .await;
    assert!(result.is_ok());
    // Mid price should be around 50050
    let resolved = result.unwrap();
    assert!(resolved >= dec!(50000) && resolved <= dec!(50100));
}
