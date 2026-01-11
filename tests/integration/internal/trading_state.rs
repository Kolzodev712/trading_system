//! Trading state management tests

use crate::helpers::internal::create_test_order;
use rust_decimal_macros::dec;
use trading_core::types::common::{OrderId, OrderStatus, Pair};
use trading_engine::state::TradingState;

#[tokio::test]
async fn test_trading_state_order_management() {
    let mut state = TradingState::new();

    let order = create_test_order("test123", dec!(50000), dec!(0.001));

    state.add_order(order.clone());
    assert_eq!(state.active_orders.len(), 1);
    assert_eq!(state.pending_orders.len(), 1);

    state.update_order_status(&order.order_id, OrderStatus::Filled, Some(order.amount));
    assert_eq!(state.pending_orders.len(), 0);

    let updated_order = state.active_orders.get(&order.order_id).unwrap();
    assert_eq!(updated_order.status, OrderStatus::Filled);
}

#[tokio::test]
async fn test_trading_state_position_management() {
    let mut state = TradingState::new();
    let pair = Pair::new("BTC", "USDT");

    // Add position
    state.update_position(pair.clone(), dec!(0.001), dec!(50000));
    let position = state.get_position(&pair);
    assert!(position.is_some());
    assert_eq!(position.unwrap().quantity, dec!(0.001));

    // Update position
    state.update_position(pair.clone(), dec!(0.0005), dec!(51000));
    let position = state.get_position(&pair).unwrap();
    assert_eq!(position.quantity, dec!(0.0015));

    // Reduce position
    state.update_position(pair.clone(), dec!(-0.001), dec!(52000));
    let position = state.get_position(&pair).unwrap();
    assert_eq!(position.quantity, dec!(0.0005));
}

#[tokio::test]
async fn test_trading_state_stale_order_detection() {
    let mut state = TradingState::new();

    let order = create_test_order("stale_order", dec!(50000), dec!(0.001));

    state.add_order(order.clone());
    state.pending_orders.insert(order.order_id.clone(), 0); // Very old timestamp

    let stale = state.get_stale_orders(1); // 1 second timeout
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0], order.order_id);
}

#[tokio::test]
async fn test_trading_state_multiple_positions() {
    let mut state = TradingState::new();

    state.update_position(Pair::new("BTC", "USDT"), dec!(0.001), dec!(50000));
    state.update_position(Pair::new("ETH", "USDT"), dec!(1.0), dec!(3000));

    assert_eq!(state.positions.len(), 2);
    assert!(state.get_position(&Pair::new("BTC", "USDT")).is_some());
    assert!(state.get_position(&Pair::new("ETH", "USDT")).is_some());
}

#[tokio::test]
async fn test_trading_state_order_removal() {
    let mut state = TradingState::new();

    let order_id = OrderId::new("remove_test".into());
    let order = create_test_order("remove_test", dec!(50000), dec!(0.001));

    state.add_order(order);
    assert_eq!(state.active_orders.len(), 1);

    state.remove_order(&order_id);
    assert_eq!(state.active_orders.len(), 0);
    assert_eq!(state.pending_orders.len(), 0);
}
