//! Bybit Exchange Engine External Integration Tests
//!
//! Tests external trading API calls to Bybit.
//! Tests are marked with #[ignore] by default and require actual API access.
//! Run with: cargo test --test integration integration::external::exchange_engine::exchanges::bybit -- --ignored
//!
//! NOTE: These tests require valid Bybit API credentials and will make real API calls.
//! Use testnet credentials or a test account to avoid affecting real trading.
//! Set BYBIT_API_KEY and BYBIT_API_SECRET environment variables.

use crate::helpers::external::{ExchangeClientBuilder, TestExchangeConfig};
use rust_decimal_macros::dec;
use trading_core::types::{
    common::{Exchange, Instrument, InstrumentType, OrderType, Pair, Side},
    exchange::OrderRequest,
};

fn bybit_credentials() -> (String, String) {
    (
        std::env::var("BYBIT_API_KEY").unwrap_or_default(),
        std::env::var("BYBIT_API_SECRET").unwrap_or_default(),
    )
}

/// Test fetching account balances
#[tokio::test]
#[ignore]
async fn test_fetch_balance() {
    let (api_key, api_secret) = bybit_credentials();
    let config = TestExchangeConfig {
        exchange: Exchange::Bybit,
        pairs: vec![Pair::new("BTC", "USDT")],
        api_key,
        secret_key: api_secret,
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    let balances = client_guard.fetch_balance().await;
    assert!(
        balances.is_ok(),
        "Failed to fetch balances: {:?}",
        balances.err()
    );

    let balances = balances.unwrap();
    assert!(
        !balances.is_empty(),
        "Should have at least one balance entry"
    );

    for balance in &balances {
        assert!(
            !balance.symbol.to_string().is_empty(),
            "Balance symbol should not be empty"
        );
        assert!(
            balance.total >= balance.free + balance.used,
            "Balance total should be >= free + used"
        );
    }
}

/// Test placing a test order (doesn't execute)
#[tokio::test]
#[ignore]
async fn test_test_order() {
    let (api_key, api_secret) = bybit_credentials();
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Bybit,
        pairs: vec![pair.clone()],
        api_key,
        secret_key: api_secret,
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    let order_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: dec!(10000.0), // Very low price (won't execute)
        amount: dec!(0.001),
        timestamp: None,
        exchange_params: Default::default(),
    };

    let result = client_guard.test_open_order(order_request).await;
    assert!(
        result.is_ok(),
        "Test order should succeed: {:?}",
        result.err()
    );
}

/// Test placing a real order (will execute if price is right)
#[tokio::test]
#[ignore]
async fn test_place_order() {
    let (api_key, api_secret) = bybit_credentials();
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Bybit,
        pairs: vec![pair.clone()],
        api_key,
        secret_key: api_secret,
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    let mut order_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: dec!(10000.0),
        amount: dec!(0.001),
        timestamp: None,
        exchange_params: Default::default(),
    };
    order_request
        .exchange_params
        .insert("timeInForce".into(), "GTC".into());

    let result = client_guard.open_order(order_request).await;
    assert!(result.is_ok(), "Failed to place order: {:?}", result.err());

    let order = result.unwrap();
    assert!(
        !order.order_id.as_str().is_empty(),
        "Order ID should not be empty"
    );
    assert_eq!(order.instrument.pair, pair);
    assert_eq!(order.side, Side::Buy);

    // Clean up
    let _ = client_guard.cancel_order_by_id(order.order_id).await;
}

/// Test fetching open orders
#[tokio::test]
#[ignore]
async fn test_fetch_open_orders() {
    let (api_key, api_secret) = bybit_credentials();
    let config = TestExchangeConfig {
        exchange: Exchange::Bybit,
        pairs: vec![Pair::new("BTC", "USDT")],
        api_key,
        secret_key: api_secret,
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    let orders = client_guard.fetch_open_orders().await;
    assert!(
        orders.is_ok(),
        "Failed to fetch open orders: {:?}",
        orders.err()
    );
}

/// Test fetching trades
#[tokio::test]
#[ignore]
async fn test_fetch_trades() {
    let (api_key, api_secret) = bybit_credentials();
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Bybit,
        pairs: vec![pair.clone()],
        api_key,
        secret_key: api_secret,
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    let trades = client_guard.fetch_trades(Some(&pair)).await;
    assert!(trades.is_ok(), "Failed to fetch trades: {:?}", trades.err());

    let trades = trades.unwrap();
    for trade in &trades {
        assert_eq!(trade.pair, pair);
        assert!(
            trade.quantity > dec!(0),
            "Trade quantity should be positive"
        );
    }
}
