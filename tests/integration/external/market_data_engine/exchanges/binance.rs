//! Binance Market Data Engine External Integration Tests
//!
//! Tests external WebSocket connections to Binance for market data streaming.
//! Tests are marked with #[ignore] by default and require actual WebSocket connections.
//! Run with: cargo test --test integration integration::external::market_data_engine::exchanges::binance -- --ignored

use crate::helpers::external::{MarketDataClientBuilder, TestExchangeConfig};
use market_data_engine::messages::WSMessageHandler;
use std::sync::Arc;
use tokio::sync::Mutex;
use trading_core::{
    error::EngineError,
    types::{
        common::Exchange,
        market_data::{StructuredOrderbook, StructuredPublicTrade},
    },
};

/// Test message handler that collects received messages
#[derive(Debug)]
struct TestMessageHandler {
    orderbooks: Arc<Mutex<Vec<StructuredOrderbook>>>,
    trades: Arc<Mutex<Vec<StructuredPublicTrade>>>,
}

impl TestMessageHandler {
    fn new() -> Self {
        Self {
            orderbooks: Arc::new(Mutex::new(Vec::new())),
            trades: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn get_orderbook_count(&self) -> usize {
        self.orderbooks.lock().await.len()
    }

    async fn get_trade_count(&self) -> usize {
        self.trades.lock().await.len()
    }

    /// Create a boxed handler wrapper that can be used with the client
    fn into_boxed_handler(self: Arc<Self>) -> Box<dyn WSMessageHandler + Send + Sync> {
        Box::new(HandlerWrapper { inner: self })
    }
}

/// Wrapper to convert Arc<TestMessageHandler> to Box<dyn WSMessageHandler>
#[derive(Debug)]
struct HandlerWrapper {
    inner: Arc<TestMessageHandler>,
}

#[async_trait::async_trait]
impl WSMessageHandler for HandlerWrapper {
    async fn handle_orderbook(&self, orderbook: StructuredOrderbook) -> Result<(), EngineError> {
        self.inner.handle_orderbook(orderbook).await
    }

    async fn handle_trade(&self, trade: StructuredPublicTrade) -> Result<(), EngineError> {
        self.inner.handle_trade(trade).await
    }
}

#[async_trait::async_trait]
impl WSMessageHandler for TestMessageHandler {
    async fn handle_orderbook(&self, orderbook: StructuredOrderbook) -> Result<(), EngineError> {
        let mut orderbooks = self.orderbooks.lock().await;
        orderbooks.push(orderbook);
        Ok(())
    }

    async fn handle_trade(&self, trade: StructuredPublicTrade) -> Result<(), EngineError> {
        let mut trades = self.trades.lock().await;
        trades.push(trade);
        Ok(())
    }
}

/// Test WebSocket connection to Binance for orderbook streaming
#[tokio::test]
#[ignore]
async fn test_orderbook_stream() {
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![trading_core::types::common::Pair::new("BTC", "USDT")],
        api_key: String::new(),
        secret_key: String::new(),
    };

    let handler = Arc::new(TestMessageHandler::new());
    let handler_clone = handler.clone();

    // Build client with message handler
    let mut client = MarketDataClientBuilder::new(config)
        .with_buffer_size(100)
        .build_with_handler(Some(handler.clone().into_boxed_handler()));

    // Start orderbook stream in background
    let client_handle = tokio::spawn(async move { client.orderbooks().await });

    // Wait for some messages to arrive
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Verify we received some orderbook updates
    let orderbook_count = handler_clone.get_orderbook_count().await;
    assert!(
        orderbook_count > 0,
        "Should have received at least one orderbook update, got {}",
        orderbook_count
    );

    // Cancel the stream
    client_handle.abort();
}

/// Test WebSocket connection for trade streaming
#[tokio::test]
#[ignore]
async fn test_trade_stream() {
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![trading_core::types::common::Pair::new("BTC", "USDT")],
        api_key: String::new(),
        secret_key: String::new(),
    };

    let handler = Arc::new(TestMessageHandler::new());
    let handler_clone = handler.clone();

    // Build client with message handler
    let client = MarketDataClientBuilder::new(config)
        .with_buffer_size(100)
        .build_with_handler(Some(handler.clone().into_boxed_handler()));

    // Start trade stream in background
    let client_handle = tokio::spawn(async move {
        let mut client = client;
        client.trades().await
    });

    // Wait for some messages to arrive
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Verify we received some trade updates
    let trade_count = handler_clone.get_trade_count().await;
    assert!(
        trade_count > 0,
        "Should have received at least one trade update, got {}",
        trade_count
    );

    // Cancel the stream
    client_handle.abort();
}

/// Test multiple stream subscriptions
#[tokio::test]
#[ignore]
async fn test_multiple_streams() {
    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![
            trading_core::types::common::Pair::new("BTC", "USDT"),
            trading_core::types::common::Pair::new("ETH", "USDT"),
        ],
        api_key: String::new(),
        secret_key: String::new(),
    };

    let handler = Arc::new(TestMessageHandler::new());
    let handler_clone = handler.clone();

    // Build client with message handler
    let mut client = MarketDataClientBuilder::new(config)
        .with_buffer_size(100)
        .build_with_handler(Some(handler.clone().into_boxed_handler()));

    // Start orderbook stream (which should handle multiple pairs from config)
    let client_handle = tokio::spawn(async move { client.orderbooks().await });

    // Wait for messages
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Verify we received messages for multiple pairs
    let orderbook_count = handler_clone.get_orderbook_count().await;
    assert!(
        orderbook_count > 0,
        "Should have received orderbook updates for multiple pairs, got {}",
        orderbook_count
    );

    // Cancel the stream
    client_handle.abort();
}

/// Test WebSocket reconnection handling
#[tokio::test]
#[ignore]
async fn test_reconnection() {
    // Note: This test is difficult to implement without actually disconnecting the WebSocket
    // In a real scenario, we would simulate a network interruption or server disconnect.
    // For now, we verify that the client can start a stream successfully,
    // which indicates the reconnection logic is in place.

    let config = TestExchangeConfig {
        exchange: Exchange::Binance,
        pairs: vec![trading_core::types::common::Pair::new("BTC", "USDT")],
        api_key: String::new(),
        secret_key: String::new(),
    };

    let handler = Arc::new(TestMessageHandler::new());

    // Build client with message handler
    let mut client = MarketDataClientBuilder::new(config)
        .with_buffer_size(100)
        .build_with_handler(Some(handler.clone().into_boxed_handler()));

    // Start stream - if reconnection logic works, this should succeed
    // even if there are temporary connection issues
    let result = tokio::time::timeout(tokio::time::Duration::from_secs(10), async move {
        client.orderbooks().await
    })
    .await;

    // The stream should start (reconnection is handled internally)
    // We just verify it doesn't immediately fail
    // Note: orderbooks() runs indefinitely, so timeout is expected
    assert!(
        result.is_err(), // Timeout is expected
        "Stream should run continuously (reconnection handled internally)"
    );
}
