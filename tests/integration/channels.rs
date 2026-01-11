//! Channel communication tests

use rust_decimal_macros::dec;
use smol_str::SmolStr;
use trading_core::channels::exchange::exchange_update_channel;
use trading_core::channels::indicators::indicator_update_channel;
use trading_core::channels::market_data::market_data_update_channel;
use trading_core::types::common::{OrderId, OrderStatus, Pair, Side, Symbol};
use trading_core::types::exchange::Order;
use trading_core::types::market_data::StructuredOrderbook;

#[tokio::test]
async fn test_market_data_channel() {
    let (tx, mut rx) = market_data_update_channel(10);

    let orderbook = StructuredOrderbook {
        pair: Pair::new("BTC", "USDT"),
        bids: vec![],
        asks: vec![],
        timestamp: 1000,
    };

    tx.send(
        trading_core::channels::market_data::MarketDataUpdate::Orderbook {
            orderbook: orderbook.clone(),
        },
    )
    .unwrap();

    let received = rx.recv().await;
    assert!(received.is_ok());
}

#[tokio::test]
async fn test_indicator_channel() {
    let (tx, mut rx) = indicator_update_channel(10);

    let update = trading_core::channels::indicators::IndicatorUpdate {
        indicator_id: SmolStr::new("test"),
        indicator_name: SmolStr::new("SMA"),
        pair: SmolStr::new("BTCUSDT"),
        value: dec!(50000),
        timestamp: 1000,
        period: 5,
        is_composite: false,
    };

    tx.send(update.clone()).unwrap();

    let received = rx.recv().await;
    assert!(received.is_ok());
}

#[tokio::test]
async fn test_exchange_channel() {
    let (tx, mut rx) = exchange_update_channel(10);

    let order = Order {
        order_id: OrderId::new("test".into()),
        client_order_id: None,
        symbol: Symbol::from("BTCUSDT"),
        side: Side::Buy,
        price: dec!(50000),
        amount: dec!(0.001),
        internal_created_at: 0,
        exchange_created_at: None,
        status: OrderStatus::New,
        filled_amount: dec!(0),
        instrument_type: trading_core::types::common::InstrumentType::Spot,
    };

    tx.send(trading_core::channels::exchange::ExchangeUpdate::Order(
        order,
    ))
    .unwrap();

    let received = rx.recv().await;
    assert!(received.is_ok());
}
