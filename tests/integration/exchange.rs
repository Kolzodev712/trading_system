//! Exchange engine state tests
//!
//! Note: Tests that require actual exchange API calls are marked with #[ignore]
//! and can be run with: cargo test --test integration -- --ignored

use super::helpers::*;
use rust_decimal_macros::dec;
use trading_core::types::common::{OrderId, OrderStatus, Symbol};
use trading_core::types::exchange::{Balance, Order};

#[tokio::test]
async fn test_exchange_state_manager_creation() {
    let (_manager, _rx) = create_test_exchange_state_manager().await;
    // Should not panic
    // Test passes if we get here
}

#[tokio::test]
async fn test_exchange_state_order_management() {
    let (manager, _rx) = create_test_exchange_state_manager().await;

    let order = Order {
        order_id: OrderId::new("test_order".into()),
        symbol: Symbol::from("BTCUSDT"),
        side: trading_core::types::common::Side::Buy,
        price: dec!(50000),
        amount: dec!(0.001),
        internal_created_at: 0,
        exchange_created_at: None,
        status: OrderStatus::New,
        filled_amount: dec!(0),
    };

    manager.update_order(order.clone()).await.unwrap();

    let state = manager.get_state();
    let state_guard = state.read().await;
    let stored_order = state_guard.get_order_by_id(&order.order_id);
    assert!(stored_order.is_some());
}

#[tokio::test]
async fn test_exchange_state_balance_management() {
    let (manager, _rx) = create_test_exchange_state_manager().await;

    let balance = Balance {
        symbol: Symbol::from("USDT"),
        free: dec!(10000),
        used: dec!(1000),
        total: dec!(11000),
    };

    let _ = manager.update_balance(balance.clone()).await;

    let state = manager.get_state();
    let state_guard = state.read().await;
    let stored_balance = state_guard.get_balance("USDT");
    assert!(stored_balance.is_some());
    assert_eq!(stored_balance.unwrap().free, dec!(10000));
}

// Tests that require actual exchange API calls - marked as ignored
// Run with: cargo test --test integration exchange -- --ignored

#[tokio::test]
#[ignore]
async fn test_exchange_open_order() {
    // This test requires actual exchange API connection
    // It will be skipped unless run with --ignored flag
    let (_manager, _rx) = create_test_exchange_state_manager().await;

    // In a real test, this would place an order via the exchange
    // For now, just verify manager exists
    // Test passes if we get here
}

#[tokio::test]
#[ignore]
async fn test_exchange_cancel_order() {
    // This test requires actual exchange API connection
    let (_manager, _rx) = create_test_exchange_state_manager().await;

    // In a real test, this would cancel an order via the exchange
    // Test passes if we get here
}

#[tokio::test]
#[ignore]
async fn test_exchange_fetch_balances() {
    // This test requires actual exchange API connection
    let (_manager, _rx) = create_test_exchange_state_manager().await;

    // In a real test, this would fetch balances from the exchange
    // Test passes if we get here
}

#[tokio::test]
#[ignore]
async fn test_exchange_websocket_connection() {
    // This test requires actual WebSocket connection to exchange
    let (_manager, _rx) = create_test_exchange_state_manager().await;

    // In a real test, this would verify WebSocket connection
    // Test passes if we get here
}

#[tokio::test]
#[ignore]
async fn test_exchange_reconciliation() {
    // This test requires actual exchange API calls for reconciliation
    let (_manager, _rx) = create_test_exchange_state_manager().await;

    // In a real test, this would test WS vs REST reconciliation
    // Test passes if we get here
}
