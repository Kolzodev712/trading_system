use async_trait::async_trait;
use std::fmt::Debug;
use tracing::debug;
use trading_core::{
    channels::market_data::{MarketDataUpdate, MarketDataUpdateSender},
    error::EngineError,
    types::market_data::{LegacyOrderbook as Orderbook, LegacyTrade as Trade},
};

/// Trait that defines how messages (orderbooks, trades) should be processed.
#[async_trait]
pub trait WSMessageHandler: Send + Sync + Debug {
    async fn handle_orderbook(&self, orderbook: Orderbook) -> Result<(), EngineError>;
    async fn handle_trade(&self, trade: Trade) -> Result<(), EngineError>;
}

#[derive(Debug)]
/// **Handles incoming WebSocket messages and publishes them via broadcast channels**
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
    async fn handle_orderbook(&self, orderbook: Orderbook) -> Result<(), EngineError> {
        self.market_data_tx
            .send(MarketDataUpdate::Orderbook(orderbook))
            .map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!(
                    "Failed to send orderbook to channel: {error}",
                    error = e
                ))
            })?;

        debug!("Published orderbook to channel.");
        Ok(())
    }

    async fn handle_trade(&self, trade: Trade) -> Result<(), EngineError> {
        self.market_data_tx
            .send(MarketDataUpdate::Trade(trade))
            .map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!(
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
    async fn handle_orderbook(&self, orderbook: Orderbook) -> Result<(), EngineError> {
        let message = RedisMessage::OrderbookMessage(orderbook);
        self.redis_client
            .publish(ORDERBOOK_CHANNEL, message)
            .await?;

        debug!("Published orderbook to Redis.");
        Ok(())
    }

    async fn handle_trade(&self, trade: Trade) -> Result<(), EngineError> {
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
        types::market_data::{LegacyOrderbook as Orderbook, LegacyTrade as Trade},
    };

    #[tokio::test]
    async fn test_channel_message_handler_orderbook() {
        let (tx, mut rx) = market_data_update_channel(100);
        let handler = ChannelMessageHandler::new(tx);

        let orderbook = Orderbook {
            bids: vec![Decimal::new(50000, 0), Decimal::new(1, 0)],
            asks: vec![Decimal::new(51000, 0), Decimal::new(1, 0)],
        };

        handler.handle_orderbook(orderbook.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            MarketDataUpdate::Orderbook(ob) => assert_eq!(ob, orderbook),
            _ => panic!("Expected Orderbook variant"),
        }
    }

    #[tokio::test]
    async fn test_channel_message_handler_trade() {
        let (tx, mut rx) = market_data_update_channel(100);
        let handler = ChannelMessageHandler::new(tx);

        let trade = Trade {
            price: Decimal::new(123456, 2),
            quantity: Decimal::new(5, 1),
            time: 123456789,
            is_market_maker: true,
        };

        handler.handle_trade(trade.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            MarketDataUpdate::Trade(t) => assert_eq!(t, trade),
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

        let orderbook = Orderbook {
            bids: vec![Decimal::new(50000, 0)],
            asks: vec![Decimal::new(51000, 0)],
        };

        handler.handle_orderbook(orderbook.clone()).await.unwrap();

        // Both receivers should get the update
        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        assert!(matches!(received1, MarketDataUpdate::Orderbook(_)));
        assert!(matches!(received2, MarketDataUpdate::Orderbook(_)));
    }
}
