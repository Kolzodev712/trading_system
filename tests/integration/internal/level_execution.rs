//! Level execution and resolver tests

use indicator_engine::data::IndicatorValue;
use indicator_engine::LevelPricingRegistry;
use rust_decimal_macros::dec;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use trading_core::types::common::Pair;
use trading_core::types::market_data::Orderbook;
use trading_engine::levels::config::PriceConfig;
use trading_engine::levels::resolver::IndicatorResolver;

fn test_resolver() -> IndicatorResolver {
    IndicatorResolver::new(
        None,
        trading_core::types::common::Exchange::Binance,
        Arc::new(parking_lot::RwLock::new(LevelPricingRegistry::new())),
        dec!(0.01),
        dec!(0.001),
    )
}

#[tokio::test]
async fn test_indicator_resolver_fixed_price() {
    let resolver = test_resolver();

    let indicators: HashMap<SmolStr, IndicatorValue> = HashMap::new();
    let pair = Pair::new("BTC", "USDT");

    let price_config = PriceConfig::Fixed(dec!(50000));

    let result = resolver
        .resolve_price(&price_config, "test_level", &indicators, None, &pair)
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), dec!(50000));
}

#[tokio::test]
async fn test_indicator_resolver_orderbook_price() {
    let resolver = test_resolver();

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
        .resolve_price(
            &price_config,
            "test_level",
            &indicators,
            Some(&orderbook),
            &pair,
        )
        .await;
    assert!(result.is_ok());
    // Mid price should be around 50050
    let resolved = result.unwrap();
    assert!(resolved >= dec!(50000) && resolved <= dec!(50100));
}

#[tokio::test]
async fn test_indicator_resolver_binding_price() {
    use indicator_engine::level_pricing::{
        LevelPricingRegistry, LevelPricingRule, PricingOffsetType,
    };
    use rust_decimal::Decimal;
    use std::str::FromStr;

    let mut registry = LevelPricingRegistry::new();
    registry
        .insert(LevelPricingRule {
            level_id: "buy_level_1".to_string(),
            indicator_ref: "sma_20".to_string(),
            offset_type: PricingOffsetType::PercentBelow,
            offset_value: Decimal::from_str("0.5").unwrap(),
        })
        .unwrap();

    let resolver = IndicatorResolver::new(
        None,
        trading_core::types::common::Exchange::Binance,
        Arc::new(parking_lot::RwLock::new(registry)),
        dec!(0.01),
        dec!(0.001),
    );

    let mut indicators: HashMap<SmolStr, IndicatorValue> = HashMap::new();
    indicators.insert(
        SmolStr::new("sma_20_BTCUSDT"),
        IndicatorValue {
            indicator_id: SmolStr::new("sma_20_BTCUSDT"),
            indicator_name: SmolStr::new("SMA"),
            pair: SmolStr::new("BTCUSDT"),
            value: dec!(100),
            timestamp: 0,
            period: 20,
            is_composite: false,
        },
    );

    let pair = Pair::new("BTC", "USDT");
    let price = resolver
        .resolve_price(
            &PriceConfig::Binding,
            "buy_level_1",
            &indicators,
            None,
            &pair,
        )
        .await
        .unwrap();
    // 0.5% below 100 → 99.5
    assert_eq!(price, dec!(99.5));
}
