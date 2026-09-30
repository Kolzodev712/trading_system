//! k-paper Exchange Engine External Integration Tests
//!
//! Tests the k-paper client against a running k-paper server (WebSocket + REST).
//! Order flow uses WebSocket (/ws/trade). fetch_balance, fetch_open_orders, fetch_trades use REST.
//! All tests are #[ignore] and require the server to be running.
//!
//! Run with: cargo test --test integration integration::external::exchange_engine::exchanges::k_paper -- --ignored
//!
//! Set PAPER_EXCHANGE_URL (default ws://127.0.0.1:19999/ws/trade) if the server is elsewhere.

use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use trading_core::{
    config::ChannelConfig,
    registry::symbol::SymbolRegistry,
    types::{
        common::{Instrument, InstrumentType, OrderType, Pair, Side},
        exchange::OrderRequest,
    },
    venue::cex::channels::cex_venue_update_channel,
    venue::cex::traits::TradingClient,
};

use exchange_engine::exchanges::k_paper::{
    client::{run_websocket_loop, KPaperClient, KPaperConfig, KPaperWebsocketLoop},
    PaperConfig,
};

/// Spawn k-paper WebSocket loop and return trading client. Waits for balance snapshot (15s timeout).
async fn spawn_paper_client(
    url: &str,
    pairs: Vec<Pair>,
) -> (
    Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
) {
    let paper_config = PaperConfig {
        paper_exchange_url: url.to_string(),
        pairs: pairs.clone(),
        instrument_type: InstrumentType::Spot,
        leverage: None,
    };
    let kpaper_config = KPaperConfig::from(paper_config);
    let (client, command_rx) = KPaperClient::new(kpaper_config.clone());
    let state = client.state();
    let trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
        Arc::new(RwLock::new(client));

    let channels = ChannelConfig::default();
    let (exchange_update_tx, _rx) = cex_venue_update_channel(channels.cex_venue_update_capacity);
    let symbol_registry = Arc::new(SymbolRegistry::from_pairs(pairs));
    let state_manager = Arc::new(exchange_engine::state::manager::ExchangeStateManager::new(
        exchange_update_tx,
        symbol_registry,
        trading_client.clone(),
        0,
        false,
    ));

    let (balance_ready_tx, balance_ready_rx) = tokio::sync::oneshot::channel();
    let balance_ready = Some(Arc::new(tokio::sync::Mutex::new(Some(balance_ready_tx))));
    let (_shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let loop_handle = tokio::spawn(async move {
        run_websocket_loop(KPaperWebsocketLoop {
            config: kpaper_config,
            command_rx,
            state,
            market_data_tx: None,
            exchange_state_manager: state_manager,
            shutdown_rx,
            balance_ready,
            connection_state: None,
        })
        .await
    });

    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(15), balance_ready_rx).await;
    (trading_client, loop_handle)
}

fn paper_url() -> String {
    std::env::var("PAPER_EXCHANGE_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:19999/ws/trade".to_string())
}

#[tokio::test]
#[ignore]
async fn test_fetch_balance() {
    let url = paper_url();
    let (trading_client, _handle) = spawn_paper_client(&url, vec![Pair::new("BTC", "USDT")]).await;

    let client = trading_client.read().await;
    let balances = client.fetch_balance().await;
    assert!(
        balances.is_ok(),
        "Failed to fetch balances: {:?}",
        balances.err()
    );
    let balances = balances.unwrap();
    assert!(!balances.is_empty(), "Should have at least one balance");
    for b in &balances {
        assert!(!b.symbol.to_string().is_empty());
        assert!(b.total >= b.free + b.used);
    }
}

#[tokio::test]
#[ignore]
async fn test_test_open_order() {
    let url = paper_url();
    let (trading_client, _handle) = spawn_paper_client(&url, vec![Pair::new("BTC", "USDT")]).await;

    let pair = Pair::new("BTC", "USDT");
    let order_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: rust_decimal_macros::dec!(50000),
        amount: rust_decimal_macros::dec!(0.002),
        timestamp: None,
        exchange_params: std::collections::HashMap::new(),
    };

    let client = trading_client.read().await;
    let result = client.test_open_order(order_request).await;
    assert!(
        result.is_ok(),
        "test_open_order should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
#[ignore]
async fn test_place_order() {
    let url = paper_url();
    let (trading_client, _handle) = spawn_paper_client(&url, vec![Pair::new("BTC", "USDT")]).await;

    let pair = Pair::new("BTC", "USDT");
    let order_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: rust_decimal_macros::dec!(50000),
        amount: rust_decimal_macros::dec!(0.002),
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };

    let client = trading_client.read().await;
    let result = client.open_order(order_request).await;
    assert!(result.is_ok(), "Place order failed: {:?}", result.err());
    let order = result.unwrap();
    assert_eq!(order.instrument.pair, pair);
    assert_eq!(order.side, Side::Buy);
}

#[tokio::test]
#[ignore]
async fn test_cancel_order() {
    let url = paper_url();
    let (trading_client, _handle) = spawn_paper_client(&url, vec![Pair::new("BTC", "USDT")]).await;

    let pair = Pair::new("BTC", "USDT");
    let order_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: rust_decimal_macros::dec!(50000),
        amount: rust_decimal_macros::dec!(0.002),
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };

    let client = trading_client.read().await;
    let order = client.open_order(order_request).await.expect("place order");
    let cancel_result = client.cancel_order_by_id(order.order_id.clone()).await;
    assert!(
        cancel_result.is_ok(),
        "Cancel failed: {:?}",
        cancel_result.err()
    );
}

#[tokio::test]
#[ignore]
async fn test_fetch_open_orders() {
    let url = paper_url();
    let (trading_client, _handle) = spawn_paper_client(&url, vec![Pair::new("BTC", "USDT")]).await;

    let client = trading_client.read().await;
    let result = client.fetch_open_orders().await;
    assert!(
        result.is_ok(),
        "fetch_open_orders failed: {:?}",
        result.err()
    );
    let orders = result.unwrap();
    for o in &orders {
        assert!(!o.order_id.as_str().is_empty());
    }
}

#[tokio::test]
#[ignore]
async fn test_fetch_trades() {
    let url = paper_url();
    let pair = Pair::new("BTC", "USDT");
    let (trading_client, _handle) = spawn_paper_client(&url, vec![pair.clone()]).await;

    let client = trading_client.read().await;
    let result = client.fetch_trades(Some(&pair)).await;
    assert!(result.is_ok(), "fetch_trades failed: {:?}", result.err());
    let trades = result.unwrap();
    for t in &trades {
        assert_eq!(t.pair, pair);
    }
}

#[tokio::test]
#[ignore]
async fn test_amend_order() {
    let url = paper_url();
    let (trading_client, _handle) = spawn_paper_client(&url, vec![Pair::new("BTC", "USDT")]).await;

    let pair = Pair::new("BTC", "USDT");
    let order_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: rust_decimal_macros::dec!(50000),
        amount: rust_decimal_macros::dec!(0.002),
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };

    let client = trading_client.read().await;
    let order = client.open_order(order_request).await.expect("place order");

    let amend_request = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: rust_decimal_macros::dec!(49999),
        amount: order.amount,
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };
    let amend_result = client
        .amend_order(order.order_id.clone(), amend_request)
        .await;
    assert!(
        amend_result.is_ok(),
        "Amend failed: {:?}",
        amend_result.err()
    );
    let amended = amend_result.unwrap();
    assert_eq!(amended.order_id, order.order_id);
}

#[tokio::test]
#[ignore]
async fn test_cancel_all_orders() {
    let url = paper_url();
    let pair = Pair::new("BTC", "USDT");
    let (trading_client, _handle) = spawn_paper_client(&url, vec![pair.clone()]).await;

    let client = trading_client.read().await;
    let req1 = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: rust_decimal_macros::dec!(50000),
        amount: rust_decimal_macros::dec!(0.002),
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };
    let _ = client.open_order(req1).await;
    let req2 = OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: rust_decimal_macros::dec!(50100),
        amount: rust_decimal_macros::dec!(0.002),
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };
    let _ = client.open_order(req2).await;

    let cancel_all_result = client.cancel_all_orders(&pair).await;
    assert!(
        cancel_all_result.is_ok(),
        "cancel_all failed: {:?}",
        cancel_all_result.err()
    );
}
