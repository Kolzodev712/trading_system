//! Futures trading integration tests
//!
//! These tests verify futures trading functionality using mock clients.
//! They test the futures trait methods and ensure the infrastructure is properly set up.

use crate::helpers::internal::MockTradingClient;
use rust_decimal::Decimal;
use trading_core::traits::TradingClient;
use trading_core::types::common::{Instrument, InstrumentType, OrderType, Pair, Side, Symbol};
use trading_core::types::exchange::{OrderRequest, Position};
use trading_core::types::futures::{
    FuturesBalance, FuturesOrderRequest, LeverageInfo, MarginType, PositionMode,
};

#[tokio::test]
async fn test_mock_trading_client_futures_methods() {
    let pair = Pair::new("BTC", "USDT");
    let position = Position {
        instrument: Instrument::new(InstrumentType::Futures, pair.clone()),
        quantity: Decimal::new(1, 0), // Positive = long position
        avg_price: Decimal::new(50000, 0),
        current_price: Decimal::new(51000, 0),
        unrealized_pnl: Decimal::new(1000, 0),
        mark_price: Some(Decimal::new(51000, 0)),
        leverage: Some(10),
        margin_type: Some(MarginType::Cross),
        last_update: 1000000,
        index_price: None,
        realized_pnl: None,
        initial_margin: None,
        maintenance_margin: None,
        position_mode: None,
        liquidation_price: None,
    };

    let collateral_balance = FuturesBalance {
        asset: Symbol::from("USDT"),
        available_balance: Decimal::new(10000, 0),
        wallet_balance: Decimal::new(11000, 0),
        cross_wallet_balance: Decimal::new(11000, 0),
        cross_unrealized_pnl: Decimal::new(1000, 0),
        available_withdraw: Decimal::new(10000, 0),
        last_update: 1000000,
    };

    let leverage_info = LeverageInfo {
        pair: pair.clone(),
        leverage: 10,
        max_leverage: 125,
        last_update: 1000000,
    };

    let client = MockTradingClient::new()
        .with_positions(vec![position.clone()])
        .with_collateral_balances(vec![collateral_balance.clone()])
        .with_leverage_info(pair.clone(), leverage_info.clone())
        .with_position_mode(PositionMode::OneWay)
        .with_margin_type(pair.clone(), MarginType::Cross);

    // Test fetch_positions
    let positions = client.fetch_positions().await.unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].instrument.pair, pair);

    // Test fetch_position
    let fetched_position = client.fetch_position(&pair).await.unwrap();
    assert!(fetched_position.is_some());
    assert_eq!(fetched_position.unwrap().instrument.pair, pair);

    // Test fetch_collateral_balance
    let balances = client.fetch_collateral_balance().await.unwrap();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].asset, Symbol::from("USDT"));

    // Test get_leverage
    let fetched_leverage = client.get_leverage(&pair).await.unwrap();
    assert_eq!(fetched_leverage.leverage, 10);
    assert_eq!(fetched_leverage.max_leverage, 125);

    // Test set_leverage
    let new_leverage = client.set_leverage(&pair, 20).await.unwrap();
    assert_eq!(new_leverage.leverage, 20);

    // Test get_position_mode
    let mode = client.get_position_mode().await.unwrap();
    assert_eq!(mode, PositionMode::OneWay);

    // Test set_position_mode
    client.set_position_mode(PositionMode::Hedge).await.unwrap();

    // Test get_margin_type
    let margin_type = client.get_margin_type(&pair).await.unwrap();
    assert_eq!(margin_type, MarginType::Cross);

    // Test set_margin_type
    client
        .set_margin_type(&pair, MarginType::Isolated)
        .await
        .unwrap();

    // Test place_futures_order
    let futures_req = FuturesOrderRequest {
        base: OrderRequest {
            instrument: Instrument::new(InstrumentType::Futures, pair.clone()),
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: Decimal::new(50000, 0),
            amount: Decimal::new(1, 0),
            timestamp: None,
            exchange_params: std::collections::HashMap::new(),
        },
        position_side: None,
        reduce_only: None,
        close_position: None,
        stop_price: None,
        activation_price: None,
        callback_rate: None,
        working_type: None,
        price_protect: None,
    };

    let order = client.place_futures_order(futures_req).await.unwrap();
    assert_eq!(order.instrument.instrument_type(), InstrumentType::Futures);

    // Test cancel_all_futures_orders
    client.cancel_all_futures_orders(&pair).await.unwrap();

    // Test fetch_open_futures_orders
    let futures_orders = client.fetch_open_futures_orders(None).await.unwrap();
    assert!(futures_orders.is_empty()); // No orders configured in this mock
}
