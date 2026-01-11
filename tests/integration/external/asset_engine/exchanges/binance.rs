//! Binance Asset Engine External Integration Tests
//!
//! Tests external API calls to Binance for asset data retrieval.
//! Tests are marked with #[ignore] by default and require actual API access.
//! Run with: cargo test --test integration integration::external::asset_engine::exchanges::binance -- --ignored

use crate::helpers::external::{AssetDataClientBuilder, TestExchangeConfig};
use asset_engine::AssetService;
use trading_core::types::common::{Exchange, Pair};

/// Test fetching exchange info from Binance
#[tokio::test]
#[ignore]
async fn test_fetch_exchange_info() {
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![Pair::new("BTC", "USDT")],
        api_key: String::new(), // Public endpoint, no auth needed
        secret_key: String::new(),
    };

    let client = AssetDataClientBuilder::new(config.clone()).build();

    // Test direct client call
    let exchange_info = client.fetch_exchange_info().await;
    assert!(
        exchange_info.is_ok(),
        "Failed to fetch exchange info: {:?}",
        exchange_info.err()
    );

    let info = exchange_info.unwrap();
    assert!(
        !info.symbols.is_empty(),
        "Exchange info should contain symbols"
    );
    assert!(
        !info.rate_limits.is_empty(),
        "Exchange info should contain rate limits"
    );
}

/// Test fetching market data for specific pairs
#[tokio::test]
#[ignore]
async fn test_fetch_market_data() {
    let pairs = vec![Pair::new("BTC", "USDT"), Pair::new("ETH", "USDT")];
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: pairs.clone(),
        api_key: String::new(),
        secret_key: String::new(),
    };

    let client = AssetDataClientBuilder::new(config).build();
    let service = AssetService::new();

    service
        .register_exchange(Exchange::Binance, client)
        .await
        .unwrap();

    let result = service.fetch_market_data(Exchange::Binance, &pairs).await;
    assert!(
        result.is_ok(),
        "Failed to fetch market data: {:?}",
        result.err()
    );

    let market_data = result.unwrap();
    assert_eq!(
        market_data.symbol_info.len(),
        pairs.len(),
        "Should have symbol info for all requested pairs"
    );
    assert_eq!(
        market_data.trading_rules.len(),
        pairs.len(),
        "Should have trading rules for all requested pairs"
    );

    // Verify data is not empty
    for pair in &pairs {
        assert!(
            market_data.symbol_info.contains_key(pair),
            "Missing symbol info for {:?}",
            pair
        );
        assert!(
            market_data.trading_rules.contains_key(pair),
            "Missing trading rules for {:?}",
            pair
        );
    }
}

/// Test getting trading rules for a pair
#[tokio::test]
#[ignore]
async fn test_get_trading_rules() {
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![pair.clone()],
        api_key: String::new(),
        secret_key: String::new(),
    };

    let client = AssetDataClientBuilder::new(config).build();
    let service = AssetService::new();

    service
        .register_exchange(Exchange::Binance, client)
        .await
        .unwrap();

    // First fetch market data to populate cache
    service
        .fetch_market_data(Exchange::Binance, std::slice::from_ref(&pair))
        .await
        .unwrap();

    // Then get trading rules
    let trading_rules = service.get_trading_rules(Exchange::Binance, &pair).await;
    assert!(
        trading_rules.is_some(),
        "Should have trading rules for {:?}",
        pair
    );

    let rules = trading_rules.unwrap();
    assert!(
        rules.min_order_size.is_some(),
        "Trading rules should have min_order_size"
    );
    assert!(
        rules.max_order_size.is_some(),
        "Trading rules should have max_order_size"
    );
    assert!(
        rules.tick_size.is_some(),
        "Trading rules should have tick_size"
    );
}

/// Test getting symbol info
#[tokio::test]
#[ignore]
async fn test_get_symbol_info() {
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![pair.clone()],
        api_key: String::new(),
        secret_key: String::new(),
    };

    let client = AssetDataClientBuilder::new(config).build();
    let service = AssetService::new();

    service
        .register_exchange(Exchange::Binance, client)
        .await
        .unwrap();

    // First fetch market data to populate cache
    service
        .fetch_market_data(Exchange::Binance, std::slice::from_ref(&pair))
        .await
        .unwrap();

    // Then get symbol info
    let symbol_info = service.get_symbol_info(Exchange::Binance, &pair).await;
    assert!(
        symbol_info.is_some(),
        "Should have symbol info for {:?}",
        pair
    );

    let info = symbol_info.unwrap();
    assert_eq!(info.pair, pair, "Symbol info pair should match");
    assert_eq!(
        info.status,
        trading_core::types::asset::SymbolStatus::Trading
    );
}
