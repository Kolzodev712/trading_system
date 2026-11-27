use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::debug;
use trading_core::{
    channels::market_data::{MarketDataUpdate, MarketDataUpdateSender},
    error::EngineError,
    types::market_data::{StructuredOrderbook, StructuredPublicTrade},
};

use crate::state::manager::MarketDataStateManager;

/// Trait that defines how messages (orderbooks, trades) should be processed.
#[async_trait]
pub trait WSMessageHandler: Send + Sync + Debug {
    async fn handle_orderbook(&self, orderbook: StructuredOrderbook) -> Result<(), EngineError>;
    async fn handle_trade(&self, trade: StructuredPublicTrade) -> Result<(), EngineError>;

    /// Handle orderbook with symbol (for backward compatibility - extracts symbol from pair)
    async fn handle_orderbook_with_symbol(
        &self,
        _symbol: &str,
        orderbook: StructuredOrderbook,
    ) -> Result<(), EngineError> {
        // Default implementation calls handle_orderbook
        self.handle_orderbook(orderbook).await
    }

    /// Handle trade with symbol (for backward compatibility - extracts symbol from pair)
    async fn handle_trade_with_symbol(
        &self,
        _symbol: &str,
        trade: StructuredPublicTrade,
    ) -> Result<(), EngineError> {
        // Default implementation calls handle_trade
        self.handle_trade(trade).await
    }
}

#[derive(Debug)]
/// **Handles incoming WebSocket messages and updates state manager, which publishes via broadcast channels**
///
/// This handler updates the MarketDataStateManager, which in turn:
/// 1. Updates in-memory state
/// 2. Publishes updates via broadcast channels
pub struct StateManagerMessageHandler {
    state_manager: Arc<MarketDataStateManager>,
}

impl StateManagerMessageHandler {
    pub fn new(state_manager: Arc<MarketDataStateManager>) -> Self {
        Self { state_manager }
    }
}

#[async_trait]
impl WSMessageHandler for StateManagerMessageHandler {
    async fn handle_orderbook(&self, orderbook: StructuredOrderbook) -> Result<(), EngineError> {
        // Extract symbol from pair for validation (use uppercase to match registry)
        let symbol_str = orderbook.pair.convert_to_smolstr();
        let symbol: trading_core::types::common::Symbol = symbol_str.into();
        self.state_manager
            .update_orderbook(&symbol, orderbook)
            .await
    }

    async fn handle_trade(&self, trade: StructuredPublicTrade) -> Result<(), EngineError> {
        // Extract symbol from pair for validation
        let symbol = trade.pair.convert_to_smolstr();
        let symbol: trading_core::types::common::Symbol = symbol.into();
        self.state_manager.add_public_trade(&symbol, trade).await
    }

    async fn handle_orderbook_with_symbol(
        &self,
        symbol: &str,
        orderbook: StructuredOrderbook,
    ) -> Result<(), EngineError> {
        let symbol: trading_core::types::common::Symbol = symbol.into();
        self.state_manager
            .update_orderbook(&symbol, orderbook)
            .await
    }

    async fn handle_trade_with_symbol(
        &self,
        symbol: &str,
        trade: StructuredPublicTrade,
    ) -> Result<(), EngineError> {
        let symbol: trading_core::types::common::Symbol = symbol.into();
        self.state_manager.add_public_trade(&symbol, trade).await
    }
}

#[derive(Debug)]
/// **Legacy handler: Handles incoming WebSocket messages and publishes them directly via broadcast channels**
///
/// This is kept for backward compatibility. New code should use `StateManagerMessageHandler`.
pub struct ChannelMessageHandler {
    market_data_tx: MarketDataUpdateSender,
}

impl ChannelMessageHandler {
    pub fn new(market_data_tx: MarketDataUpdateSender) -> Self {
        Self { market_data_tx }
    }
}

#[async_trait]
impl WSMessageHandler for ChannelMessageHandler {
    async fn handle_orderbook(&self, orderbook: StructuredOrderbook) -> Result<(), EngineError> {
        // ChannelMessageHandler is legacy - structured orderbook already has pair
        self.market_data_tx
            .send(MarketDataUpdate::Orderbook {
                orderbook: Arc::new(orderbook),
                trace_context: None,
            })
            .map_err(|e| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to send orderbook to channel: {error}",
                    error = e
                ))
            })?;

        debug!("Published orderbook to channel.");
        Ok(())
    }

    async fn handle_trade(&self, trade: StructuredPublicTrade) -> Result<(), EngineError> {
        // ChannelMessageHandler is legacy - structured trade already has pair
        self.market_data_tx
            .send(MarketDataUpdate::Trade {
                trade,
                trace_context: None,
            })
            .map_err(|e| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to send trade to channel: {error}",
                    error = e
                ))
            })?;

        debug!("Published trade to channel.");
        Ok(())
    }
}

// Keep Redis handler behind feature flag for future Web UI
#[cfg(feature = "redis")]
use trading_core::redis::{RedisClient, RedisMessage, ORDERBOOK_CHANNEL, TRADE_CHANNEL};

#[cfg(feature = "redis")]
#[derive(Debug)]
/// **Handles incoming WebSocket messages and publishes them to Redis**
///
/// This is kept behind the `redis` feature flag for potential future Web UI use.
pub struct RedisMessageHandler {
    redis_client: RedisClient,
}

#[cfg(feature = "redis")]
impl RedisMessageHandler {
    pub fn new(redis_client: RedisClient) -> Self {
        Self { redis_client }
    }
}

#[cfg(feature = "redis")]
#[async_trait]
impl WSMessageHandler for RedisMessageHandler {
    async fn handle_orderbook(&self, orderbook: StructuredOrderbook) -> Result<(), EngineError> {
        let message = RedisMessage::OrderbookMessage(orderbook);
        self.redis_client
            .publish(ORDERBOOK_CHANNEL, message)
            .await?;

        debug!("Published orderbook to Redis.");
        Ok(())
    }

    async fn handle_trade(&self, trade: StructuredPublicTrade) -> Result<(), EngineError> {
        let message = RedisMessage::TradeMessage(trade);
        self.redis_client.publish(TRADE_CHANNEL, message).await?;

        debug!("Published trade to Redis.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::{
        channels::market_data::{market_data_update_channel, MarketDataUpdate},
        types::market_data::{StructuredOrderbook, StructuredPublicTrade},
    };

    #[tokio::test]
    async fn test_channel_message_handler_orderbook() {
        let (tx, mut rx) = market_data_update_channel(100);
        let handler = ChannelMessageHandler::new(tx);

        use trading_core::types::common::Pair;
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![(Decimal::new(50000, 0), Decimal::new(1, 0))],
            asks: vec![(Decimal::new(51000, 0), Decimal::new(1, 0))],
            timestamp: 1234567890,
            update_kind: Default::default(),
            sequence: None,
        };

        handler.handle_orderbook(orderbook.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            MarketDataUpdate::Orderbook { orderbook: ob, .. } => assert_eq!(*ob, orderbook),
            _ => panic!("Expected Orderbook variant"),
        }
    }

    #[tokio::test]
    async fn test_channel_message_handler_trade() {
        let (tx, mut rx) = market_data_update_channel(100);
        let handler = ChannelMessageHandler::new(tx);

        use trading_core::types::common::Pair;
        let trade = StructuredPublicTrade {
            pair: Pair::new("BTC", "USDT"),
            price: Decimal::new(123456, 2),
            quantity: Decimal::new(5, 1),
            timestamp: 123456789,
            is_market_maker: true,
            trade_id: None,
        };

        handler.handle_trade(trade.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            MarketDataUpdate::Trade { trade: t, .. } => assert_eq!(t, trade),
            _ => panic!("Expected Trade variant"),
        }
    }

    #[tokio::test]
    async fn test_channel_message_handler_fan_out() {
        let (tx, _) = market_data_update_channel(100);
        let handler = ChannelMessageHandler::new(tx.clone());

        // Create multiple receivers to test fan-out
        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();

        use trading_core::types::common::Pair;
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![(Decimal::new(50000, 0), Decimal::new(1, 0))],
            asks: vec![(Decimal::new(51000, 0), Decimal::new(1, 0))],
            timestamp: 1234567890,
            update_kind: Default::default(),
            sequence: None,
        };

        handler.handle_orderbook(orderbook.clone()).await.unwrap();

        // Both receivers should get the update
        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        assert!(matches!(received1, MarketDataUpdate::Orderbook { .. }));
        assert!(matches!(received2, MarketDataUpdate::Orderbook { .. }));
    }

    #[tokio::test]
    async fn test_state_manager_message_handler() {
        use crate::state::manager::MarketDataStateManager;
        use trading_core::registry::symbol::SymbolRegistry;
        use trading_core::types::common::Pair;

        let (tx, mut rx) = market_data_update_channel(100);
        let pair = Pair::new("BTC", "USDT");
        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![pair.clone()]));
        let state_manager = Arc::new(MarketDataStateManager::new(tx, symbol_registry));
        let handler = StateManagerMessageHandler::new(state_manager.clone());

        let orderbook = StructuredOrderbook {
            pair: pair.clone(),
            bids: vec![(Decimal::new(50000, 0), Decimal::new(1, 0))],
            asks: vec![(Decimal::new(51000, 0), Decimal::new(1, 0))],
            timestamp: 1234567890,
            update_kind: Default::default(),
            sequence: None,
        };

        handler
            .handle_orderbook_with_symbol("BTCUSDT", orderbook.clone())
            .await
            .unwrap();

        // Verify state was updated
        let state_arc = state_manager.get_state();
        let state = state_arc.read().await;
        let stored_orderbook = state.get_orderbook(&pair).unwrap();
        assert_eq!(stored_orderbook.bids, orderbook.bids);

        // Verify channel received the update
        let received = rx.recv().await.unwrap();
        assert!(matches!(received, MarketDataUpdate::Orderbook { .. }));
    }
}
