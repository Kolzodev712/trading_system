//! Redis client for pub/sub (kept for backward compatibility during migration)
//! Note: Legacy Redis path; live system uses broadcast channels (see gateway / trading_networking)
use crate::error::EngineError;
use crate::types::market_data::{StructuredOrderbook, StructuredPublicTrade};
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::debug;

pub const ORDERBOOK_CHANNEL: &str = "orderbook";
pub const TRADE_CHANNEL: &str = "trade";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum RedisMessage {
    OrderbookMessage(StructuredOrderbook),
    TradeMessage(StructuredPublicTrade),
}

#[derive(Debug, Clone)]
pub struct RedisClient {
    client: Client,
}

impl RedisClient {
    pub fn new(url: &str) -> Result<Self, EngineError> {
        let client = redis::Client::open(url).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to create Redis client: {}", e))
        })?;
        Ok(Self { client })
    }

    pub async fn publish(&self, channel: &str, message: RedisMessage) -> Result<(), EngineError> {
        let mut connection = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("Failed to get Redis connection: {}", e))
            })?;

        let serialized_message = serde_json::to_string(&message)?;

        let _: () = connection
            .publish(channel, serialized_message)
            .await
            .map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("Failed to publish to Redis: {}", e))
            })?;

        Ok(())
    }

    pub async fn process_stream<F>(&self, channel: &str, mut handler: F) -> Result<(), EngineError>
    where
        F: FnMut(RedisMessage) -> Result<(), EngineError> + Send + 'static,
    {
        let mut pubsub_conn = self.client.get_async_pubsub().await.map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to get Redis pubsub: {}", e))
        })?;
        pubsub_conn.subscribe(channel).await.map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!(
                "Failed to subscribe to Redis channel: {}",
                e
            ))
        })?;

        let mut stream = pubsub_conn.on_message();
        while let Some(message) = stream.next().await {
            let payload: String = message.get_payload().map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!(
                    "Failed to get Redis message payload: {}",
                    e
                ))
            })?;
            let redis_message: RedisMessage =
                serde_json::from_str(&payload).map_err(EngineError::JsonError)?;

            debug!("Received message on {channel}: {redis_message:?}");

            if let Err(e) = handler(redis_message) {
                debug!("Error in message handler: {e}");
            }
        }

        Ok(())
    }
}
