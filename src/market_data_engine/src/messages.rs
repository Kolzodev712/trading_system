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

        debug!("✅ Published orderbook to Redis.");
        Ok(())
    }

    async fn handle_trade(&self, trade: Trade) -> Result<(), EngineError> {
        let message = RedisMessage::TradeMessage(trade);
        self.redis_client.publish(TRADE_CHANNEL, message).await?;

        debug!("✅ Published trade to Redis.");
        Ok(())
    }
}
