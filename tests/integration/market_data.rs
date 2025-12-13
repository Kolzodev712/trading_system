//! Market data engine tests

use super::helpers::create_test_market_data_manager;
use rust_decimal_macros::dec;
use trading_core::types::common::Pair;
use trading_core::types::market_data::StructuredOrderbook;
use trading_core::types::market_data::StructuredPublicTrade;

#[tokio::test]
async fn test_market_data_state_manager_creation() {
    let (_, _rx) = create_test_market_data_manager();
    // Should not panic
    // Test passes if we get here
}

#[tokio::test]
async fn test_market_data_orderbook_update() {
    let (manager, _rx) = create_test_market_data_manager();

    let orderbook = StructuredOrderbook {
        pair: Pair::new("BTC", "USDT"),
        bids: vec![(dec!(50000), dec!(1.0))],
        asks: vec![(dec!(50100), dec!(1.0))],
        timestamp: 1000,
    };

    let symbol = trading_core::types::common::Symbol::from("BTCUSDT");
    manager
        .update_orderbook(&symbol, orderbook.clone())
        .await
        .unwrap();

    // Check state using getter method
    let state = manager.get_state();
    let state_guard = state.read().await;
    let stored_orderbook = state_guard.get_orderbook(&Pair::new("BTC", "USDT"));
    assert!(stored_orderbook.is_some());
}

#[tokio::test]
async fn test_market_data_trade_update() {
    let (manager, _rx) = create_test_market_data_manager();

    let trade = StructuredPublicTrade {
        pair: Pair::new("BTC", "USDT"),
        price: dec!(50000),
        quantity: dec!(0.1),
        is_market_maker: false,
        timestamp: 1000,
        trade_id: None,
    };

    let symbol = trading_core::types::common::Symbol::from("BTCUSDT");
    manager
        .add_public_trade(&symbol, trade.clone())
        .await
        .unwrap();

    // Check state using getter method
    let state = manager.get_state();
    let state_guard = state.read().await;
    let trades = state_guard.get_public_trades(&Pair::new("BTC", "USDT"));
    assert!(!trades.is_empty());
}

#[tokio::test]
async fn test_market_data_state_initialization() {
    let state = market_data_engine::state::state::MarketDataState::new();
    // State is initialized - verify by checking getters return empty
    let pairs = state.get_pairs_with_orderbooks();
    assert!(pairs.is_empty());
}
