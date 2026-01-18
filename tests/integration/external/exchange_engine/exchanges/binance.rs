//! Binance Exchange Engine External Integration Tests
//!
//! Tests external trading API calls to Binance.
//! Tests are marked with #[ignore] by default and require actual API access.
//! Run with: cargo test --test integration integration::external::exchange_engine::exchanges::binance -- --ignored
//!
//! NOTE: These tests require valid Binance API credentials and will make real API calls.
//! Use testnet credentials or a test account to avoid affecting real trading.

use crate::helpers::external::{ExchangeClientBuilder, TestExchangeConfig};
use rust_decimal_macros::dec;
use trading_core::types::{
    common::{Exchange, Instrument, InstrumentType, OrderType, Pair, Side},
    exchange::OrderRequest,
};

/// Test fetching account balances
#[tokio::test]
#[ignore]
async fn test_fetch_balance() {
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![Pair::new("BTC", "USDT")],
        api_key: std::env::var("BINANCE_API_KEY").unwrap_or_default(),
        secret_key: std::env::var("BINANCE_SECRET_KEY").unwrap_or_default(),
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

    // Verify balance structure
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
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![pair.clone()],
        api_key: std::env::var("BINANCE_API_KEY").unwrap_or_default(),
        secret_key: std::env::var("BINANCE_SECRET_KEY").unwrap_or_default(),
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    use trading_core::types::common::Instrument;
    let order_request = OrderRequest {
        instrument: Instrument::new(
            trading_core::types::common::InstrumentType::Spot,
            pair.clone(),
        ),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: dec!(10000.0), // Very low price (won't execute)
        amount: dec!(0.001),  // Small amount
        timestamp: None,
        exchange_params: Default::default(),
    };

    // Test order (doesn't actually place)
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
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![pair.clone()],
        api_key: std::env::var("BINANCE_API_KEY").unwrap_or_default(),
        secret_key: std::env::var("BINANCE_SECRET_KEY").unwrap_or_default(),
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    // Use a very low price that won't execute immediately
    let mut order_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: dec!(10000.0), // Adjust to a price that won't execute
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
    assert_eq!(
        order.instrument.symbol().to_string(),
        pair.convert_to_smolstr().as_str(),
        "Order symbol should match pair"
    );
    assert_eq!(order.side, Side::Buy, "Order side should match");

    // Clean up: cancel the order
    let _ = client_guard.cancel_order_by_id(order.order_id).await;
}

/// Test canceling an order
#[tokio::test]
#[ignore]
async fn test_cancel_order() {
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![pair.clone()],
        api_key: std::env::var("BINANCE_API_KEY").unwrap_or_default(),
        secret_key: std::env::var("BINANCE_SECRET_KEY").unwrap_or_default(),
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    // Place an order first
    let mut order_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: dec!(10000.0), // Won't execute
        amount: dec!(0.001),
        timestamp: None,
        exchange_params: Default::default(),
    };
    order_request
        .exchange_params
        .insert("timeInForce".into(), "GTC".into());

    let order = client_guard
        .open_order(order_request)
        .await
        .expect("Should place order successfully");

    // Cancel the order
    let cancel_result = client_guard
        .cancel_order_by_id(order.order_id.clone())
        .await;
    assert!(
        cancel_result.is_ok(),
        "Failed to cancel order: {:?}",
        cancel_result.err()
    );
}

/// Test fetching open orders
#[tokio::test]
#[ignore]
async fn test_fetch_open_orders() {
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![pair.clone()],
        api_key: std::env::var("BINANCE_API_KEY").unwrap_or_default(),
        secret_key: std::env::var("BINANCE_SECRET_KEY").unwrap_or_default(),
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    let open_orders = client_guard.fetch_open_orders().await;
    assert!(
        open_orders.is_ok(),
        "Failed to fetch open orders: {:?}",
        open_orders.err()
    );

    // Verify structure (may be empty if no open orders)
    let orders = open_orders.unwrap();
    for order in &orders {
        assert!(
            !order.order_id.as_str().is_empty(),
            "Order ID should not be empty"
        );
        assert_eq!(order.status, trading_core::types::common::OrderStatus::New);
    }
}

/// Test fetching trade history
#[tokio::test]
#[ignore]
async fn test_fetch_trades() {
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![pair.clone()],
        api_key: std::env::var("BINANCE_API_KEY").unwrap_or_default(),
        secret_key: std::env::var("BINANCE_SECRET_KEY").unwrap_or_default(),
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    let trades = client_guard.fetch_trades(Some(&pair)).await;
    assert!(trades.is_ok(), "Failed to fetch trades: {:?}", trades.err());

    // Verify structure (may be empty if no trades)
    let trades = trades.unwrap();
    for trade in &trades {
        assert_eq!(trade.pair, pair, "Trade pair should match");
        assert!(
            !trade.order_id.as_str().is_empty(),
            "Trade order ID should not be empty"
        );
        assert!(trade.price > dec!(0), "Trade price should be positive");
        assert!(
            trade.quantity > dec!(0),
            "Trade quantity should be positive"
        );
    }
}

/// Test amending an order
#[tokio::test]
#[ignore]
async fn test_amend_order() {
    let pair = Pair::new("BTC", "USDT");
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![pair.clone()],
        api_key: std::env::var("BINANCE_API_KEY").unwrap_or_default(),
        secret_key: std::env::var("BINANCE_SECRET_KEY").unwrap_or_default(),
    };

    let builder = ExchangeClientBuilder::new(config);
    let trading_client = builder.build_trading_client();
    let client_guard = trading_client.read().await;

    // Place an order first
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

    let order = client_guard
        .open_order(order_request)
        .await
        .expect("Should place order successfully");

    // Amend the order (change price)
    let mut amend_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: dec!(11000.0), // New price
        amount: dec!(0.001),
        timestamp: None,
        exchange_params: [("origClientOrderId".into(), order.order_id.as_str().into())]
            .iter()
            .cloned()
            .collect(),
    };
    amend_request
        .exchange_params
        .insert("timeInForce".into(), "GTC".into());

    let amend_result = client_guard
        .amend_order(order.order_id.clone(), amend_request)
        .await;
    assert!(
        amend_result.is_ok(),
        "Failed to amend order: {:?}",
        amend_result.err()
    );

    let amended_order = amend_result.unwrap();
    assert_eq!(
        amended_order.order_id, order.order_id,
        "Amended order should have same order ID"
    );
    assert_eq!(
        amended_order.price,
        dec!(11000.0),
        "Price should be updated"
    );

    // Clean up
    let _ = client_guard.cancel_order_by_id(order.order_id).await;
}

// Futures-specific tests (when futures support is added)
#[tokio::test]
#[ignore]
async fn test_fetch_positions() {
    // TODO: Implement when Binance Futures is integrated
    // This will test fetch_positions() method
}

#[tokio::test]
#[ignore]
async fn test_fetch_collateral_balance() {
    // TODO: Implement when Binance Futures is integrated
    // This will test fetch_collateral_balance() method
}

#[tokio::test]
#[ignore]
async fn test_set_leverage() {
    // TODO: Implement when Binance Futures is integrated
    // This will test set_leverage() method
}
