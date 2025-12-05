use async_trait::async_trait;
use core_logic::{
    data::public_data::{Orderbook, Trade},
    error::EngineError,
    redis::{RedisClient, RedisMessage, ORDERBOOK_CHANNEL, TRADE_CHANNEL},
};
use std::fmt::Debug;
use tracing::debug;

/// Trait that defines how messages (orderbooks, trades) should be processed.
#[async_trait]
pub trait WSMessageHandler: Send + Sync + Debug {
    async fn handle_orderbook(&self, orderbook: Orderbook) -> Result<(), EngineError>;
    async fn handle_trade(&self, trade: Trade) -> Result<(), EngineError>;
}

#[derive(Debug)]
/// **Handles incoming WebSocket messages and publishes them to Redis**
pub struct RedisMessageHandler {
    redis_client: RedisClient,
}

impl RedisMessageHandler {
    pub fn new(redis_client: RedisClient) -> Self {
        Self { redis_client }
    }
}

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
    use core_logic::data::public_data::{Orderbook, Trade};
    use rust_decimal::Decimal;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    struct MockRedisClient {
        pub published: Arc<Mutex<Vec<(String, RedisMessage)>>>,
    }

    impl MockRedisClient {
        fn new() -> Self {
            Self {
                published: Arc::new(Mutex::new(Vec::new())),
            }
        }

        async fn publish(&self, channel: &str, message: RedisMessage) -> Result<(), EngineError> {
            self.published
                .lock()
                .unwrap()
                .push((channel.to_string(), message));
            Ok(())
        }
    }

    #[derive(Debug)]
    struct MockRedisMessageHandler {
        redis_client: MockRedisClient,
    }

    impl MockRedisMessageHandler {
        fn new(redis_client: MockRedisClient) -> Self {
            Self { redis_client }
        }
    }

    #[async_trait::async_trait]
    impl WSMessageHandler for MockRedisMessageHandler {
        async fn handle_orderbook(&self, orderbook: Orderbook) -> Result<(), EngineError> {
            let message = RedisMessage::OrderbookMessage(orderbook);
            self.redis_client.publish(ORDERBOOK_CHANNEL, message).await
        }

        async fn handle_trade(&self, trade: Trade) -> Result<(), EngineError> {
            let message = RedisMessage::TradeMessage(trade);
            self.redis_client.publish(TRADE_CHANNEL, message).await
        }
    }

    #[tokio::test]
    async fn test_orderbook_is_published_to_redis() {
        let mock_client = MockRedisClient::new();
        let handler = MockRedisMessageHandler::new(mock_client.clone());

        let orderbook = Orderbook {
            bids: vec![],
            asks: vec![],
        };

        handler.handle_orderbook(orderbook.clone()).await.unwrap();

        let log = mock_client.published.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].0, ORDERBOOK_CHANNEL);
        assert_eq!(log[0].1, RedisMessage::OrderbookMessage(orderbook));
    }

    #[tokio::test]
    async fn test_trade_is_published_to_redis() {
        let mock_client = MockRedisClient::new();
        let handler = MockRedisMessageHandler::new(mock_client.clone());

        let trade = Trade {
            price: Decimal::new(123456, 2),
            quantity: Decimal::new(5, 1),
            time: 123456789,
            is_market_maker: true,
        };

        handler.handle_trade(trade.clone()).await.unwrap();

        let log = mock_client.published.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].0, TRADE_CHANNEL);
        assert_eq!(log[0].1, RedisMessage::TradeMessage(trade));
    }
}
